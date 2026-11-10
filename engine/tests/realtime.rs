//! The audio thread's work allocates nothing: an allocation there waits on the allocator's lock,
//! which a build (every core, allocating) holds often, and an audio block late is an xrun. And a
//! build writes little memory: the audio thread waits on the same memory (a felt's measurement
//! writing 85 MB per note slowed its blocks 3–4×). A counting allocator for this test binary
//! counts on the thread being looked at only.
mod common;

use dussek::{Engine, Event, RadiationCache, Tuning};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if COUNTING.with(|c| c.get()) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(l.size(), Ordering::Relaxed);
        }
        // SAFETY: as the caller's
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if COUNTING.with(|c| c.get()) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: as the caller's
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if COUNTING.with(|c| c.get()) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(n, Ordering::Relaxed);
        }
        // SAFETY: as the caller's
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const SR: f64 = 48000.0;

fn dir(name: &str) -> String {
    format!("{}/../instruments/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// the allocations (and frees) on this thread while `f` runs
fn counted(f: impl FnOnce()) -> usize {
    let before = ALLOCS.load(Ordering::Relaxed);
    COUNTING.with(|c| c.set(true));
    f();
    COUNTING.with(|c| c.set(false));
    ALLOCS.load(Ordering::Relaxed) - before
}

/// the bytes allocated on this thread while `f` runs
fn bytes(f: impl FnOnce()) -> usize {
    let before = BYTES.load(Ordering::Relaxed);
    COUNTING.with(|c| c.set(true));
    f();
    COUNTING.with(|c| c.set(false));
    BYTES.load(Ordering::Relaxed) - before
}

/// `blocks` blocks of 256 samples
fn play(e: &mut Engine, blocks: usize) {
    let (mut l, mut r) = ([0.0f32; 256], [0.0f32; 256]);
    for _ in 0..blocks {
        e.process(&mut l, &mut r);
    }
}

#[test]
fn a_pedal_press_allocates_nothing() {
    // the dampers lifting kick the strings near the bass (every morph step replays the pedal on
    // the new engine): a vector per kicked string, 53 at a time
    let Some(mut e) = common::engine(&dir("grand_a"), SR, Tuning::default()) else { return };
    for key in [36, 48, 60] {
        let speed = e.hammer_speed(key, 90.0);
        e.queue(0, Event::NoteOn { key, speed });
    }
    play(&mut e, 4);
    let n = counted(|| {
        let t = e.now();
        e.queue(t, Event::PedalDown);
        play(&mut e, 4);
    });
    assert_eq!(n, 0, "a pedal press allocated {n} times on the audio thread");
}

#[test]
fn playing_allocates_nothing() {
    // a dense passage: chords, releases, the three pedals up and down, then another engine of
    // the same instrument adopting the sounding notes (a tuning rebuild)
    let Some(mut e) = common::engine(&dir("grand_a"), SR, Tuning::default()) else { return };
    let mut next = common::engine(&dir("grand_a"), SR, Tuning::default()).unwrap();
    play(&mut e, 2);
    let n = counted(|| {
        for round in 0..12u8 {
            let t = e.now();
            for k in 0..6u8 {
                let key = 21 + (round * 13 + k * 11) % 88;
                let speed = e.hammer_speed(key, 40.0 + 12.0 * k as f64);
                e.queue(t + 32 * k as u64, Event::NoteOn { key, speed });
            }
            e.queue(t + 300, if round % 2 == 0 { Event::PedalDown } else { Event::PedalUp });
            e.queue(t + 400, Event::Pedal { depth: 0.5 });
            e.queue(t + 500, if round % 3 == 0 { Event::SostenutoDown } else { Event::SostenutoUp });
            e.queue(t + 600, Event::SoftPedal { depth: (round % 2) as f32 });
            for k in 0..3u8 {
                e.queue(t + 2000, Event::KeyUp { key: 21 + (round * 13 + k * 11) % 88 });
            }
            play(&mut e, 12);
        }
        assert!(next.adopt(&mut e), "the same instrument and tuning adopt");
        play(&mut next, 8);
    });
    assert_eq!(n, 0, "the audio thread allocated {n} times");
}

#[test]
fn brass_allocates_nothing() {
    // a line, a chord, harmony under a slur, the wheels, a mute, the sound off, a microphone moved under a sounding note
    let Some(mut e) = common::engine(&dir("tenor_trombone"), SR, Tuning::default()) else { return };
    let mut next = common::engine(&dir("tenor_trombone"), SR, Tuning { mic: dussek::mic::MicPosition::Audience, ..Tuning::default() }).unwrap();
    play(&mut e, 2);
    let n = counted(|| {
        for round in 0..6u8 {
            let t = e.now();
            for k in 0..3u8 {
                let key = 50 + round + 4 * k;
                let speed = e.hammer_speed(key, 90.0);
                e.queue(t + 8 * k as u64, Event::NoteOn { key, speed });
            }
            e.queue(t + 300, Event::ModWheel { value: 0.5 });
            e.queue(t + 400, Event::PitchBend { value: 0.3 });
            e.queue(t + 500, Event::ChannelPressure { value: 0.8 });
            let mut p = e.params();
            p.brass.mute = u32::from(round) % 7;
            p.brass.growl = 0.4;
            e.set_params(p);
            play(&mut e, 12);
            for k in 0..3u8 {
                e.queue(e.now(), Event::KeyUp { key: 50 + round + 4 * k });
            }
            play(&mut e, 4);
        }
        let t = e.now();
        let speed = e.hammer_speed(60, 90.0);
        e.queue(t, Event::NoteOn { key: 60, speed });
        play(&mut e, 12);
        assert!(next.adopt(&mut e), "one instrument takes over");
        play(&mut next, 8);
        next.silence();
        play(&mut next, 8);
    });
    assert_eq!(n, 0, "the audio thread allocated {n} times");
}

#[test]
fn a_measuring_engine_writes_little_memory() {
    // a level table renders 704 notes of 0.2 s, each its own engine from a clone of the
    // instrument: each copied every note's knock and release noise (85 MB, at the recording's
    // own rate), 59 GB in 2 s beside the audio thread
    let Some(inst) = common::load(&dir("grand_a")) else { return };
    let sr = inst.reference_sr;
    let mut cache = RadiationCache::default();
    Engine::for_window(inst.clone(), sr, Tuning::default(), &mut cache, 0.2).expect("engine");
    let n = bytes(|| {
        Engine::for_window(inst.clone(), sr, Tuning::default(), &mut cache, 0.2).expect("engine");
    });
    // the bursts shared: 19 MB (the instrument's clone 5, the voices' state); the audio thread's
    // blocks then as beside a plain morph build (2026-10-23: p99 2.9 ms, none over 5.3)
    assert!(n < 24_000_000, "a measuring engine allocated {:.1} MB", n as f64 / 1e6);
}
