//! Telemetry for the plugin's diagnostics view: frames of the sounding voices' partials
//! and the output audio, in a shared region (a plain byte area the plugin maps into
//! both the DSP and the UI). Writer: the engine on the audio thread; reader: the UI.
//! No allocation, no locks, no syscalls.
//!
//! Frames: a ring of `RING_FRAMES` slots, strictly one writer and one reader. The writer
//! drops the *newest* frame when the ring is full. The reader takes the newest frame:
//! it publishes `read = write − 1` (letting the writer reclaim older slots early), copies
//! slot (write − 1), and publishes `read = write`. The copy is protected by the final
//! store plus the writer being unable to reach that slot while `read ≤ write − 1`.
//! The mapping must outlive every copy of a `Region`.
//!
//! Audio: a ring of `AUDIO_FRAMES` stereo frames; the reader keeps its own cursor and
//! jumps forward to recent data when it has been overrun (with a margin of `MAX_BLOCK`).
//!
//! Notes: a ring of `NOTE_SLOTS` incoming note-on velocities (the plugin's Touch tab shows where
//! they fall on the velocity curve), written whether or not the telemetry is enabled; the reader
//! keeps its own cursor like the audio's.
//!
//! Values: `VALUE_SLOTS` slots the plugin's DSP side fills with the parameter values MIDI set
//! where the host takes no change request from the plugin; the UI passes them on to the host.
//! A slot is one atomic word (a sequence above, the value's bits below): never torn, the newest
//! wins. Written whether or not the telemetry is enabled.

use std::ptr;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering::{Acquire, Relaxed, Release}, fence};

pub const MAX_VOICES: usize = 24;
pub const MAX_PARTIALS: usize = 48;
pub const RING_FRAMES: usize = 8;
pub const AUDIO_FRAMES: usize = 16384; // a power of two
/// the longest block `push_audio` accepts
pub const MAX_BLOCK: usize = 1024;
pub const KEY_WORDS: usize = 8;
/// the notes ring's slots; a reader gets at most the newest `NOTE_SLOTS - 1` (the oldest slot may
/// be under the next write)
pub const NOTE_SLOTS: usize = 64;
pub const VALUE_SLOTS: usize = 128;
pub const KIND_HORIZ: u32 = 1;
pub const KIND_COPY: u32 = 2;
pub const FRAME_HZ: f64 = 30.0;
/// partials per key in the visualization's `KeyFrame`
pub const VIZ_PARTIALS: usize = 32;
/// `KeyFrame::flags`: the key's strings have a damper; struck by a hammer (else plucked, or a bar)
pub const KEY_DAMPER: u32 = 1;
pub const KEY_HAMMER: u32 = 2;

const MAGIC: u32 = 0x3154_4550; // "PET1"
const VERSION: u32 = 4;
const HEADER_BYTES: usize = 64;

/// One mode of a string: frequency (Hz), decay rate σ (1/s), amplitude (model units ×
/// the output calibration), `kind` bits `KIND_*`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Partial {
    pub freq: f32,
    pub decay: f32,
    pub amp: f32,
    pub kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VoiceFrame {
    pub key: u8,
    /// register (set of strings; 0 is the main one)
    pub reg: u8,
    /// partials listed (≤ MAX_PARTIALS, strongest first)
    pub count: u16,
    /// the hammer speed of the last strike (m/s; 0 if none)
    pub speed: f32,
    pub level: f32,
    /// live modes of the voice (all, not only the listed)
    pub modes: u32,
    pub partials: [Partial; MAX_PARTIALS],
}

/// One key for the visualization (the plugin's Visualization panel): its first register's strings
/// (or bar) and the sympathetic bank's ring on them.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KeyFrame {
    /// the engine position of the last strike or pluck (0: none since the engine started)
    pub struck_at: u64,
    /// its hammer speed (m/s; a pluck's or a mallet's speed as the engine got it)
    pub speed: f32,
    /// the damper's engagement, 0 lifted … 1 resting on the strings
    pub engage: f32,
    /// the speaking length (m) and the strike or pluck point re it
    pub length: f32,
    pub point: f32,
    /// `KEY_*` bits
    pub flags: u32,
    /// the strings' displacement per partial (m: the vertical modes over their bridge weights)
    pub amp: [f32; VIZ_PARTIALS],
    /// the sympathetic bank's ring per partial, on the same scale
    pub ring: [f32; VIZ_PARTIALS],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Frame {
    /// frames built since the engine attached the region (restarts at every attach: for
    /// debugging only, nothing may rely on it across attachments)
    pub counter: u64,
    /// engine sample position
    pub position: u64,
    pub rate: f32,
    pub pedal: f32,
    pub voices: u32,
    pub keys: [u32; KEY_WORDS],
    pub voice: [VoiceFrame; MAX_VOICES],
    pub key: [KeyFrame; 88],
}

impl Frame {
    /// a zeroed frame on the heap (a frame is 43 kB: not for the stack of a thread that cannot spare it)
    pub fn boxed() -> Box<Frame> {
        // SAFETY: Frame is plain old data; all-zero is a valid value
        unsafe { Box::<Frame>::new_zeroed().assume_init() }
    }
}

/// Key `i` (0 = A0 … 87 = C8): word `i / 12`, bits `2·(i % 12)` (bit 0 held, bit 1 ringing).
pub fn pack_keys(held: &[bool; 88], ringing: &[bool; 88]) -> [u32; KEY_WORDS] {
    let mut w = [0u32; KEY_WORDS];
    for i in 0..88 {
        let bits = held[i] as u32 | (ringing[i] as u32) << 1;
        w[i / 12] |= bits << (2 * (i % 12));
    }
    w
}

#[repr(C)]
struct Header {
    magic: u32,
    version: u32,
    enabled: AtomicU32,
    dropped: AtomicU32,
    write: AtomicU64,
    read: AtomicU64,
    audio_write: AtomicU64,
    /// pid of the creating process (0: none); read by the plugin's sweep of stale regions
    owner: AtomicU32,
    /// notes written (the next slot: `notes_write % NOTE_SLOTS`)
    notes_write: AtomicU64,
}

pub fn region_size() -> usize {
    values_offset() + VALUE_SLOTS * 8
}

fn values_offset() -> usize {
    (notes_offset() + NOTE_SLOTS).next_multiple_of(8)
}

fn notes_offset() -> usize {
    HEADER_BYTES + RING_FRAMES * size_of::<Frame>() + AUDIO_FRAMES * 2 * size_of::<f32>()
}

const _: () = assert!(size_of::<Header>() <= HEADER_BYTES);

/// A view of a mapped region. Copyable: the writer (engine) and the reader (UI) each hold one.
#[derive(Clone, Copy)]
pub struct Region {
    base: *mut u8,
}

// SAFETY: all shared state is behind atomics or the single-writer/single-reader protocol
unsafe impl Send for Region {}

impl Region {
    /// Prepare a fresh region (zeroed, with magic): the creator, once, not real-time.
    ///
    /// # Safety
    /// `base` must point to `size` writable bytes, 8-aligned, not in use by a reader or writer.
    pub unsafe fn init(base: *mut u8, size: usize) -> Option<Region> {
        if base.is_null() || size < region_size() || base as usize % 8 != 0 {
            return None;
        }
        // SAFETY: by the caller's contract
        unsafe {
            ptr::write_bytes(base, 0, region_size());
            let h = base.cast::<Header>();
            (*h).magic = MAGIC;
            (*h).version = VERSION;
        }
        Some(Region { base })
    }

    /// A view of a region someone else initialized; `None` if it is too small or not ours.
    ///
    /// # Safety
    /// `base` must point to `size` readable and writable bytes, 8-aligned.
    pub unsafe fn open(base: *mut u8, size: usize) -> Option<Region> {
        if base.is_null() || size < region_size() || base as usize % 8 != 0 {
            return None;
        }
        // SAFETY: by the caller's contract
        let h = unsafe { &*base.cast::<Header>() };
        (h.magic == MAGIC && h.version == VERSION).then_some(Region { base })
    }

    fn h(&self) -> &Header {
        // SAFETY: `base` was validated by init/open
        unsafe { &*self.base.cast::<Header>() }
    }

    fn frame_ptr(&self, slot: usize) -> *mut Frame {
        // SAFETY: slot < RING_FRAMES, inside the region
        unsafe { self.base.add(HEADER_BYTES).cast::<Frame>().add(slot) }
    }

    /// the two f32 bit patterns of audio slot `at` (< AUDIO_FRAMES), as atomics
    fn audio_slot(&self, at: usize) -> &[AtomicU32; 2] {
        // SAFETY: inside the region, 8-aligned; AtomicU32 has the layout of u32
        unsafe {
            &*self
                .base
                .add(HEADER_BYTES + RING_FRAMES * size_of::<Frame>() + 8 * at)
                .cast::<[AtomicU32; 2]>()
        }
    }

    fn note_slot(&self, at: usize) -> &AtomicU8 {
        // SAFETY: at < NOTE_SLOTS, inside the region; AtomicU8 has the layout of u8
        unsafe { &*self.base.add(notes_offset() + at).cast::<AtomicU8>() }
    }

    /// Writer: an incoming note-on's velocity (1–127), whether or not the telemetry is enabled.
    /// Real-time safe.
    pub fn push_note(&self, velocity: u8) {
        let h = self.h();
        let w = h.notes_write.load(Relaxed);
        // pairs with the `fence(Acquire)` in `read_notes`: a reader that sees this slot's store
        // also sees `notes_write` at w or later in its lap check (as `push_audio`'s)
        fence(Release);
        self.note_slot(w as usize % NOTE_SLOTS).store(velocity, Relaxed);
        h.notes_write.store(w + 1, Release);
    }

    /// The notes written so far; a reader that starts here gets only the notes played afterwards.
    pub fn notes_head(&self) -> u64 {
        self.h().notes_write.load(Acquire)
    }

    /// Reader: the velocities after `cursor`, oldest first, into `out` (as many as fit); the
    /// cursor advances. Behind by more than the ring, it jumps to the newest; what the writer may
    /// have overwritten while we copied (a slot reused by a write that has started) is dropped.
    pub fn read_notes(&self, cursor: &mut u64, out: &mut [u8]) -> usize {
        let h = self.h();
        let w = h.notes_write.load(Acquire);
        if *cursor > w {
            *cursor = w; // the region was re-initialized
        }
        *cursor = (*cursor).max(w.saturating_sub(NOTE_SLOTS as u64 - 1));
        let n = ((w - *cursor) as usize).min(out.len());
        for (i, o) in out.iter_mut().enumerate().take(n) {
            *o = self.note_slot((*cursor as usize + i) % NOTE_SLOTS).load(Relaxed);
        }
        fence(Acquire);
        // a slot is intact while no write has started on it: index j > (written now) − NOTE_SLOTS
        let w2 = h.notes_write.load(Relaxed);
        let lost = ((w2 + 1).saturating_sub(NOTE_SLOTS as u64)).saturating_sub(*cursor).min(n as u64) as usize;
        out.copy_within(lost..n, 0);
        *cursor += n as u64;
        n - lost
    }

    fn value_slot(&self, at: usize) -> &AtomicU64 {
        // SAFETY: at < VALUE_SLOTS, inside the region, 8-aligned; AtomicU64 has the layout of u64
        unsafe { &*self.base.add(values_offset() + 8 * at).cast::<AtomicU64>() }
    }

    /// Writer (one: the plugin's audio thread): slot `index`'s value, its sequence one up. Out of
    /// range: nothing. Real-time safe.
    pub fn set_value(&self, index: usize, value: f32) {
        if index >= VALUE_SLOTS {
            return;
        }
        let slot = self.value_slot(index);
        let seq = (slot.load(Relaxed) >> 32).wrapping_add(1) & 0xffff_ffff;
        slot.store(seq << 32 | value.to_bits() as u64, Release);
    }

    /// Reader: slot `index`'s (sequence, value); the sequence 0: never set. `None` out of range.
    pub fn read_value(&self, index: usize) -> Option<(u32, f32)> {
        (index < VALUE_SLOTS).then(|| {
            let w = self.value_slot(index).load(Acquire);
            ((w >> 32) as u32, f32::from_bits(w as u32))
        })
    }

    pub fn enabled(&self) -> bool {
        self.h().enabled.load(Relaxed) != 0
    }

    pub fn set_enabled(&self, on: bool) {
        self.h().enabled.store(on as u32, Relaxed);
    }

    pub fn dropped(&self) -> u32 {
        self.h().dropped.load(Relaxed)
    }

    /// Writer: count a frame that was not published because the ring is full (also used when the
    /// writer skips building one). Real-time safe.
    pub fn count_drop(&self) {
        self.h().dropped.fetch_add(1, Relaxed);
    }

    /// The creator's process id as recorded by `set_owner` (0: none).
    pub fn owner(&self) -> u32 {
        self.h().owner.load(Relaxed)
    }

    /// Creator: record the owning process, so that a later instance can tell a region
    /// left behind by a dead process from a live one.
    pub fn set_owner(&self, pid: u32) {
        self.h().owner.store(pid, Relaxed);
    }

    /// The writer's published audio position (frames since the region was initialized); a
    /// reader that starts here sees only audio written from now on.
    pub fn audio_head(&self) -> u64 {
        self.h().audio_write.load(Acquire)
    }

    /// Writer: whether `push_frame` has a free slot (the same loads it uses); lets the writer
    /// skip building a frame nobody could take.
    pub fn has_frame_space(&self) -> bool {
        let h = self.h();
        h.write.load(Relaxed) - h.read.load(Acquire) < RING_FRAMES as u64
    }

    /// Writer: publish a frame, or drop it (counted) if the ring is full.
    pub fn push_frame(&self, f: &Frame) {
        let h = self.h();
        let w = h.write.load(Relaxed);
        if w - h.read.load(Acquire) >= RING_FRAMES as u64 {
            self.count_drop();
            return;
        }
        // SAFETY: the reader never touches slot w % RING_FRAMES while w − read < RING_FRAMES
        unsafe { ptr::copy_nonoverlapping(f, self.frame_ptr(w as usize % RING_FRAMES), 1) };
        h.write.store(w + 1, Release);
    }

    /// Reader: the newest frame not yet taken; false if there is none.
    pub fn read_latest(&self, out: &mut Frame) -> bool {
        let h = self.h();
        let w = h.write.load(Acquire);
        if w == h.read.load(Relaxed) {
            return false;
        }
        h.read.store(w - 1, Release); // frees older slots for the writer; does not by itself guard the copy
        // SAFETY: slot (w − 1) % RING_FRAMES was completely written before `write` became w, and
        // the writer cannot reach it until `read` passes w − 1 (the final store below)
        unsafe { ptr::copy_nonoverlapping(self.frame_ptr((w - 1) as usize % RING_FRAMES), out, 1) };
        h.read.store(w, Release);
        true
    }

    /// Writer: append output frames. A block longer than `MAX_BLOCK` is stored and published in
    /// chunks of at most `MAX_BLOCK` (the reader's lap margin relies on that bound).
    pub fn push_audio(&self, block: &[[f32; 2]]) {
        let h = self.h();
        for chunk in block.chunks(MAX_BLOCK) {
            let w = h.audio_write.load(Relaxed);
            #[cfg(test)]
            PUSH_CHUNK.with(|h| {
                if let Some(f) = h.borrow().as_ref() {
                    f()
                }
            });
            // pairs with the `fence(Acquire)` in `read_audio`: a reader that sees any slot store of
            // this chunk also sees the previous `audio_write` value or later (so its lap check
            // cannot miss that this chunk began overwriting old data), even on weakly ordered CPUs
            fence(Release);
            for (i, s) in chunk.iter().enumerate() {
                let at = (w as usize + i) & (AUDIO_FRAMES - 1);
                let p = self.audio_slot(at);
                p[0].store(s[0].to_bits(), Relaxed);
                p[1].store(s[1].to_bits(), Relaxed);
            }
            h.audio_write.store(w + chunk.len() as u64, Release);
        }
    }

    /// Reader: interleaved stereo frames after `cursor` into `out` (as many as fit); the
    /// cursor advances. The writer fills a block before publishing it, so up to `MAX_BLOCK`
    /// frames beyond the published position may already be overwritten: only the part of
    /// the ring `AUDIO_FRAMES - MAX_BLOCK` behind the published position is trusted. When
    /// the reader is further behind (before or after the copy), the cursor jumps to recent
    /// data and nothing read is returned (what was lost is skipped, never mixed in).
    pub fn read_audio(&self, cursor: &mut u64, out: &mut [f32]) -> usize {
        let h = self.h();
        let safe = (AUDIO_FRAMES - MAX_BLOCK) as u64;
        let jump_to = |w: u64| w - (AUDIO_FRAMES / 2) as u64;
        let w = h.audio_write.load(Acquire);
        if *cursor > w {
            *cursor = w; // the region was re-initialized
        }
        if w.saturating_sub(*cursor) > safe {
            *cursor = jump_to(w);
        }
        let n = (w.saturating_sub(*cursor) as usize).min(out.len() / 2);
        for i in 0..n {
            let p = self.audio_slot((*cursor as usize + i) & (AUDIO_FRAMES - 1));
            out[2 * i] = f32::from_bits(p[0].load(Relaxed));
            out[2 * i + 1] = f32::from_bits(p[1].load(Relaxed));
        }
        // the writer may have lapped us while we copied: then the start of what we read is not ours
        #[cfg(test)]
        MID_COPY.with(|h| {
            if let Some(f) = h.borrow().as_ref() {
                f()
            }
        });
        fence(Acquire);
        let w2 = h.audio_write.load(Relaxed);
        if w2 < *cursor {
            *cursor = w2; // re-initialized during the copy: nothing of it is trusted
            return 0;
        }
        if w2 - *cursor > safe {
            *cursor = jump_to(w2);
            return 0;
        }
        *cursor += n as u64;
        n
    }
}

// test hooks (this thread only): `MID_COPY` runs between the copy and the lap re-check of
// `read_audio`; `PUSH_CHUNK` runs before each chunk of `push_audio` is stored
#[cfg(test)]
thread_local! {
    static MID_COPY: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
    static PUSH_CHUNK: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// 8-byte aligned memory standing in for the mapped region
    fn memory() -> Vec<u64> {
        vec![0u64; region_size() / 8 + 1]
    }

    fn region(mem: &mut Vec<u64>) -> Region {
        unsafe { Region::init(mem.as_mut_ptr().cast(), mem.len() * 8).expect("init") }
    }

    fn frame(n: u64) -> Box<Frame> {
        let mut f = Frame::boxed();
        f.counter = n;
        f.position = n;
        f.voice[0].partials[0].amp = n as f32;
        f.voice[MAX_VOICES - 1].partials[MAX_PARTIALS - 1].freq = n as f32;
        f
    }

    #[test]
    fn layout_is_the_documented_one() {
        assert_eq!(std::mem::size_of::<Partial>(), 16);
        assert_eq!(std::mem::size_of::<VoiceFrame>(), 784);
        assert_eq!(std::mem::size_of::<Frame>(), 44224);
        assert_eq!(std::mem::size_of::<KeyFrame>(), 288);
        assert_eq!(std::mem::offset_of!(Frame, key), 18880);
        assert_eq!(region_size(), 64 + RING_FRAMES * 44224 + AUDIO_FRAMES * 8 + NOTE_SLOTS + VALUE_SLOTS * 8);
    }

    #[test]
    fn values_mirror_the_latest_with_a_sequence() {
        let mut mem = memory();
        let r = region(&mut mem);
        assert!(!r.enabled()); // written and read whether or not the telemetry is on
        assert_eq!(r.read_value(5), Some((0, 0.0)));
        assert_eq!(r.read_value(VALUE_SLOTS - 1), Some((0, 0.0)));
        r.set_value(5, 0.5);
        r.set_value(5, 0.25);
        assert_eq!(r.read_value(5), Some((2, 0.25)));
        r.set_value(6, -3.5);
        assert_eq!(r.read_value(6), Some((1, -3.5)));
        assert_eq!(r.read_value(5), Some((2, 0.25))); // the neighbor untouched
        assert_eq!(r.read_value(VALUE_SLOTS), None);
        r.set_value(VALUE_SLOTS, 1.0); // out of range: nothing
        assert_eq!(r.read_value(VALUE_SLOTS - 1), Some((0, 0.0)));
    }

    #[test]
    fn the_header_fits_and_keeps_its_old_fields() {
        assert!(size_of::<Header>() <= HEADER_BYTES);
        assert_eq!(std::mem::offset_of!(Header, audio_write), 32); // unchanged by the owner field
    }

    #[test]
    fn owner_drops_and_audio_head_are_recorded() {
        let mut mem = memory();
        let r = region(&mut mem);
        assert_eq!((r.owner(), r.dropped(), r.audio_head()), (0, 0, 0));
        r.set_owner(4242);
        assert_eq!(r.owner(), 4242);
        r.count_drop();
        r.count_drop();
        assert_eq!(r.dropped(), 2);
        r.push_audio(&vec![[0.5f32, 0.5]; 300]);
        assert_eq!(r.audio_head(), 300);
        // a re-initialized region forgets all of it
        let r = region(&mut mem);
        assert_eq!((r.owner(), r.dropped(), r.audio_head()), (0, 0, 0));
    }

    #[test]
    fn keys_pack_two_bits_each_below_2_pow_24() {
        let (mut held, mut ringing) = ([false; 88], [false; 88]);
        held[0] = true;
        ringing[0] = true;
        ringing[1] = true;
        held[12] = true;
        held[87] = true;
        let w = pack_keys(&held, &ringing);
        assert_eq!(w[0] & 0b1111, 0b1011); // key 0: held+ringing (0b11), key 1: ringing only (0b10)
        assert_eq!(w[1] & 0b11, 0b01); // key 12: word 1, slot 0, held only
        assert_eq!(w[7] >> (2 * (87 % 12)) & 0b11, 0b01);
        assert!(w.iter().all(|&x| x < 1 << 24));
    }

    #[test]
    fn init_and_open_validate_the_region() {
        let mut mem = memory();
        let r = region(&mut mem);
        assert!(unsafe { Region::open(mem.as_mut_ptr().cast(), mem.len() * 8) }.is_some());
        assert!(unsafe { Region::open(mem.as_mut_ptr().cast(), 100) }.is_none()); // too small
        assert!(unsafe { Region::init(std::ptr::null_mut(), 1 << 20) }.is_none());
        let mut blank = memory();
        assert!(unsafe { Region::open(blank.as_mut_ptr().cast(), blank.len() * 8) }.is_none()); // no magic
        assert!(!r.enabled());
        r.set_enabled(true);
        assert!(r.enabled());
    }

    #[test]
    fn frames_arrive_and_a_full_ring_drops_the_newest() {
        let mut mem = memory();
        let r = region(&mut mem);
        let mut out = Frame::boxed();
        assert!(!r.read_latest(&mut out)); // nothing yet
        assert!(r.has_frame_space());
        for n in 1..=RING_FRAMES as u64 {
            r.push_frame(&frame(n));
        }
        assert_eq!(r.dropped(), 0);
        assert!(!r.has_frame_space());
        r.push_frame(&frame(99)); // full: dropped
        assert_eq!(r.dropped(), 1);
        assert!(r.read_latest(&mut out)); // the newest that was kept
        assert_eq!(out.counter, RING_FRAMES as u64);
        assert!(!r.read_latest(&mut out)); // consumed
        assert!(r.has_frame_space());
        r.push_frame(&frame(100)); // room again
        assert!(r.read_latest(&mut out));
        assert_eq!(out.counter, 100);
    }

    #[test]
    fn reader_never_sees_a_torn_frame() {
        let mut mem = memory();
        let r = region(&mut mem);
        let stop = Arc::new(AtomicBool::new(false));
        let writer = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut n = 1u64;
                while !stop.load(Ordering::Relaxed) {
                    r.push_frame(&frame(n));
                    n += 1;
                    if n % 7 == 0 {
                        std::thread::yield_now();
                    }
                }
                n
            })
        };
        let (mut out, mut last, mut seen) = (Frame::boxed(), 0u64, 0u32);
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 300 {
            if r.read_latest(&mut out) {
                let c = out.counter;
                assert_eq!(out.position, c);
                assert_eq!(out.voice[0].partials[0].amp, c as f32);
                assert_eq!(out.voice[MAX_VOICES - 1].partials[MAX_PARTIALS - 1].freq, c as f32);
                assert!(c >= last, "frames went backwards");
                last = c;
                seen += 1;
            }
        }
        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
        assert!(seen > 10);
    }

    #[test]
    fn audio_comes_back_in_order() {
        let mut mem = memory();
        let r = region(&mut mem);
        let block: Vec<[f32; 2]> = (0..100).map(|i| [i as f32, -(i as f32)]).collect();
        r.push_audio(&block);
        let (mut cursor, mut out) = (0u64, vec![0.0f32; 400]);
        let n = r.read_audio(&mut cursor, &mut out);
        assert_eq!(n, 100);
        assert_eq!((out[0], out[1], out[2 * 99], out[2 * 99 + 1]), (0.0, 0.0, 99.0, -99.0));
        assert_eq!(r.read_audio(&mut cursor, &mut out), 0); // nothing new
    }

    #[test]
    fn audio_reader_jumps_after_an_overrun() {
        let mut mem = memory();
        let r = region(&mut mem);
        let block: Vec<[f32; 2]> = (0..AUDIO_FRAMES as u32).map(|i| [i as f32, 0.0]).collect();
        for _ in 0..3 {
            r.push_audio(&block); // 3 × the ring: a reader at 0 is far behind
        }
        let (mut cursor, mut out) = (0u64, vec![0.0f32; 2 * 1024]);
        let n = r.read_audio(&mut cursor, &mut out);
        assert!(n > 0 && n <= 1024);
        // the jump lands half a ring behind the published position, then the n frames read
        assert_eq!(cursor, 3 * AUDIO_FRAMES as u64 - (AUDIO_FRAMES / 2) as u64 + n as u64);
        // what it returned is consecutive recent data (a ramp mod AUDIO_FRAMES)
        for i in 1..n {
            let (a, b) = (out[2 * (i - 1)], out[2 * i]);
            assert!(b == a + 1.0 || (a == (AUDIO_FRAMES - 1) as f32 && b == 0.0));
        }
    }


    #[test]
    fn a_long_block_is_published_in_chunks_of_at_most_max_block() {
        let mut mem = memory();
        let r = region(&mut mem);
        let heads = Arc::new(std::sync::Mutex::new(Vec::<u64>::new()));
        let h2 = heads.clone();
        // the head the reader could see at the start of every chunk
        PUSH_CHUNK.with(|h| *h.borrow_mut() = Some(Box::new(move || h2.lock().unwrap().push(r.h().audio_write.load(Relaxed)))));
        let block: Vec<[f32; 2]> = (0..3000).map(|i| [i as f32, -(i as f32)]).collect();
        r.push_audio(&block);
        PUSH_CHUNK.with(|h| *h.borrow_mut() = None);
        assert_eq!(*heads.lock().unwrap(), vec![0, 1024, 2048], "published after each chunk");
        assert_eq!(r.h().audio_write.load(Relaxed), 3000);
        let (mut cursor, mut out) = (0u64, vec![0.0f32; 2 * 4000]);
        assert_eq!(r.read_audio(&mut cursor, &mut out), 3000);
        assert!((0..3000).all(|i| out[2 * i] == i as f32 && out[2 * i + 1] == -(i as f32)));
        // a block longer than the whole ring is safe too: the reader jumps and reads consecutive data
        let huge: Vec<[f32; 2]> = (0..3 * AUDIO_FRAMES as u32 + 5).map(|i| [i as f32, 0.0]).collect();
        r.push_audio(&huge);
        let n = r.read_audio(&mut cursor, &mut out);
        assert!(n > 0);
        for i in 1..n {
            assert_eq!(out[2 * i], out[2 * (i - 1)] + 1.0);
        }
    }

    #[test]
    fn a_writer_index_that_goes_backwards_does_not_underflow() {
        let mut mem = memory();
        let r = region(&mut mem);
        let block: Vec<[f32; 2]> = (0..2000).map(|i| [i as f32, 0.0]).collect();
        r.push_audio(&block);
        let (mut cursor, mut out) = (1500u64, vec![0.0f32; 2 * 512]);
        // the region is re-initialized between the two loads of the writer position
        MID_COPY.with(|h| *h.borrow_mut() = Some(Box::new(move || r.h().audio_write.store(100, Relaxed))));
        let n = r.read_audio(&mut cursor, &mut out);
        MID_COPY.with(|h| *h.borrow_mut() = None);
        assert_eq!(n, 0, "nothing trusted from a region that restarted");
        assert_eq!(cursor, 100, "the cursor follows the writer back");
        // and before the first load: a cursor ahead of the writer
        cursor = 5000;
        assert_eq!(r.read_audio(&mut cursor, &mut out), 0);
        assert_eq!(cursor, 100);
    }

    #[test]
    fn audio_concurrent_reads_are_consecutive() {
        const WRAP: u64 = 1 << 20;
        let mut mem = memory();
        let r = region(&mut mem);
        let stop = Arc::new(AtomicBool::new(false));
        let writer = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let (mut n, mut block) = (0u64, vec![[0.0f32; 2]; MAX_BLOCK]);
                while !stop.load(Ordering::Relaxed) {
                    for (i, s) in block.iter_mut().enumerate() {
                        *s = [((n + i as u64) % WRAP) as f32, 0.0];
                    }
                    r.push_audio(&block);
                    n += MAX_BLOCK as u64;
                }
            })
        };
        let (mut cursor, mut out, mut calls, mut got) = (0u64, vec![0.0f32; 2 * 4096], 0u32, 0u32);
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 500 {
            let n = r.read_audio(&mut cursor, &mut out);
            for i in 1..n {
                let (a, b) = (out[2 * (i - 1)], out[2 * i]);
                assert!(b == a + 1.0 || (a == (WRAP - 1) as f32 && b == 0.0), "mixed data");
            }
            got += (n > 0) as u32;
            calls += 1;
            if calls % 64 == 0 {
                std::thread::sleep(std::time::Duration::from_micros(200)); // fall behind, so laps happen
            }
        }
        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
        assert!(got > 10);
    }

    #[test]
    fn audio_lapped_during_the_copy_is_rejected() {
        let mut mem = memory();
        let r = region(&mut mem);
        let ring = AUDIO_FRAMES;
        let block: Vec<[f32; 2]> = (0..MAX_BLOCK).map(|i| [i as f32, 0.0]).collect();
        for _ in 0..ring / MAX_BLOCK - 1 {
            r.push_audio(&block); // published position: ring − MAX_BLOCK, the edge of the safe zone
        }
        let mut cursor = 0u64;
        let mut out = vec![0.0f32; 2 * 64];
        // one more block lands while the reader is copying: the cursor is now past the margin
        MID_COPY.with(|h| *h.borrow_mut() = Some(Box::new(move || r.push_audio(&block))));
        let n = r.read_audio(&mut cursor, &mut out);
        MID_COPY.with(|h| *h.borrow_mut() = None);
        assert_eq!(n, 0, "a lapped copy is rejected");
        assert!(cursor >= (ring / 2) as u64, "cursor jumped to recent data");
        // and the next read is clean
        let n = r.read_audio(&mut cursor, &mut out);
        assert!(n > 0);
        for i in 1..n {
            assert_eq!(out[2 * i], out[2 * (i - 1)] + 1.0);
        }
    }
}
