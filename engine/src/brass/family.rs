//! The players in the engine: `Section` at 44.1 kHz, the soft limit on the sum per channel (every clip the ear accepted
//! went through it), the microphones, the engine's rate.

use super::bell;
use super::player::{Aftertouch, GlideMode, Live};
use super::rate::Stream;
use super::section::{Ev, Section, VOICES};
use super::shape::SeededDraws;
use super::streams::CTRL;
use super::tables::Tables;
use crate::convolution::Convolver;
use crate::instrument::Instrument;
use std::sync::Arc;

/// Each player's place in the streams, apart enough that two never read the same stretch of noise.
const OFFSET: usize = 6007;
const SEED: u64 = 1;
const OUT: usize = 4096;
/// the room's feed for a direct sound as loud as what the filter leaves of it (the engine's room's energy, `tests/brass_mics.rs`)
const ROOM_FEED: f64 = 0.209;
/// a microphone's handover, samples at 44.1 kHz: 20 ms
const MIC_FADE: usize = 882;
const CONV_LARGER: [usize; 1] = [1024];

/// The pair's filters, each channel of the section through its own microphone.
struct MicPath {
    conv: [Convolver; 2],
    out: [Vec<Vec<f32>>; 2],
}

impl MicPath {
    fn new(firs: [Vec<f32>; 2]) -> MicPath {
        let [a, b] = firs;
        MicPath {
            conv: [Convolver::new(&[a], CTRL, &CONV_LARGER), Convolver::new(&[b], CTRL, &CONV_LARGER)],
            out: [vec![vec![0.0; CTRL]], vec![vec![0.0; CTRL]]],
        }
    }

    fn run(&mut self, l: &[f32; CTRL], r: &[f32; CTRL]) {
        for c in 0..2 {
            self.out[c][0].fill(0.0);
            self.conv[c].process(if c == 0 { l } else { r }, &mut self.out[c]);
        }
    }

    fn clear(&mut self) {
        self.conv.iter_mut().for_each(Convolver::clear);
    }
}

/// The mutes by kind, the value of `BrassParams::mute` (the plugin's Mute): the same value names the same mute on every
/// instrument, so a switch keeps it where the next one has it.
pub const MUTE_KINDS: [&str; 7] = ["open", "straight", "cup", "wah", "wah_stem", "bucket", "plunger"];

/// What a player sets (`Params::brass`), applied per block; the default is the accepted sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrassParams {
    /// the mute by its kind (`MUTE_KINDS`), from the next tongued note
    pub mute: u32,
    pub growl: f32,
    pub flutter: f32,
    /// above 0 it replaces the player's
    pub vibrato: f32,
    /// the hand over the plunger, 0 shut … 1 open (None: the virtual player's, or half)
    pub plunger: Option<f32>,
    /// the air, 0 … 127 (None: from the keys)
    pub air: Option<f32>,
    pub virtual_player: bool,
    pub solo: bool,
    /// 0 off, 1 auto, 2 on
    pub glide_mode: u32,
    pub glide_hold: bool,
    /// s for a minor third
    pub glide_time: f32,
    /// 0 growl, 1 flutter
    pub aftertouch_to: u32,
    /// the air over the keys' or the Air's, 0 … 1: up to the top over its travel (the mod wheel's tie)
    pub air_added: f32,
}

impl Default for BrassParams {
    fn default() -> Self {
        BrassParams {
            mute: 0,
            growl: 0.0,
            flutter: 0.0,
            vibrato: 0.0,
            plunger: None,
            air: None,
            virtual_player: true,
            solo: false,
            glide_mode: 1,
            glide_hold: false,
            glide_time: 0.22,
            aftertouch_to: 0,
            air_added: 0.0,
        }
    }
}

/// The players the engine's own random numbers make.
pub fn section(tables: &Arc<Tables>, mute: usize) -> Section {
    let streams = tables.streams();
    let voices = (0..VOICES)
        .map(|i| Live::new(tables.clone(), streams.clone(), Box::new(SeededDraws { seed: SEED + 100 * i as u64 }), mute, true, i * OFFSET))
        .collect();
    Section::new(tables.clone(), voices)
}

/// Per key the tuning re the fit, semitones: concert pitch and the temperament or scale.
fn tuning_of(t: &crate::physics::Tuning) -> Arc<[f64; 128]> {
    let mut x = [0.0; 128];
    for (k, s) in x.iter_mut().enumerate() {
        let cents = match &t.keys {
            Some(keys) if (21..=108).contains(&k) => keys[k - 21],
            _ => t.temperament[k % 12],
        };
        *s = 12.0 * t.pitch_ratio.log2() + cents / 100.0;
    }
    Arc::new(x)
}

pub struct Family {
    /// None: Recording, the fitted sound with no filter and no delay
    mic: Option<MicPath>,
    /// the room's level at this position, as the direct sound's (the total's level)
    room_k: f32,
    /// the path before a handover, heard out over `fading` samples
    mic_old: Option<MicPath>,
    fading: usize,
    room: Convolver,
    room_out: Vec<Vec<f32>>,
    tune: Arc<[f64; 128]>,
    tables: Arc<Tables>,
    section: Section,
    stream: Stream<3>,
    /// what has come out, not yet taken: left, right, the room's feed
    fifo: Box<[[f32; 3]; OUT]>,
    head: usize,
    len: usize,
    solo: bool,
    slide: bool,
    latency: usize,
}

/// What the brass adds to the engine's latency at `sr`, in its samples: a note waits for the end of its block (half a
/// block on the mean, as at 44.1 kHz in samples but not in seconds), behind what the fifo holds, then through the
/// resampler. Held to 44.1 kHz's in seconds, where it adds nothing.
fn latency(sr: f64) -> usize {
    let base = crate::engine::BASE as f64;
    let extra = Stream::<1>::new(sr).latency_out() as f64 + mean_hold(sr) + base / 2.0 * (sr / super::core::SR - 1.0);
    extra.round().max(0.0) as usize
}

/// The fifo's mean fill when a block is asked for, in output samples: blocks of `CTRL` at 44.1 kHz come out as more or
/// fewer samples at the engine's rate, and a note waits behind what is there (none at 44.1 kHz, the blocks one size).
fn mean_hold(sr: f64) -> f64 {
    let mut s = Stream::<1>::new(sr);
    let (mut len, mut sum, calls) = (0usize, 0usize, 512);
    for _ in 0..calls {
        sum += len;
        while len < crate::engine::BASE {
            for _ in 0..CTRL {
                s.push([0.0], |_| len += 1);
            }
        }
        len -= crate::engine::BASE;
    }
    sum as f64 / calls as f64
}

impl Family {
    pub fn new(inst: &Instrument, sr: f64) -> Option<Family> {
        let tables = inst.brass.clone()?;
        let mut section = section(&tables, 0);
        let tune = tuning_of(&inst.tuning);
        section.voices.iter_mut().for_each(|v| v.set_tuning(tune.clone()));
        let slide = tables.slide;
        let a = tables.bell_radius;
        // the room's share at the recording's position is what the total's level holds to
        let setup = inst.tuning.mics_for(inst.body());
        let axis = match inst.body() { crate::mic::Body::Bell(pose) => pose.axis(), _ => [0.0, 1.0, 0.0] };
        let firs = setup.map(|s| bell::pair_firs(&s, a, axis, inst.tuning.keep_direct, bell::ROOM_AT_REF_DB));
        // Room at 0 dB stands 14 dB under the direct sound at Recording: `room_feed` the instrument's own balance, as measured at its export
        let room_k = firs.as_ref().map_or(1.0, |f| f.1 as f32) * (ROOM_FEED * tables.room_feed) as f32;
        let mic = firs.map(|f| MicPath::new(f.0));
        Some(Family {
            mic,
            room_k,
            mic_old: None,
            fading: 0,
            room: Convolver::new(&[bell::room_fir(a)], CTRL, &CONV_LARGER),
            room_out: vec![vec![0.0; CTRL]],
            tune, tables, section, stream: Stream::new(sr), fifo: Box::new([[0.0; 3]; OUT]), head: 0, len: 0, solo: false, slide,
            latency: latency(sr) })
    }

    /// What the resampling to the engine's rate delays the sound by, in the engine's samples (none at 44.1 kHz): the
    /// resampler's half and what waits in the fifo when a note comes, on the mean.
    pub fn latency(&self) -> usize {
        self.latency
    }

    pub fn tables(&self) -> &Tables {
        &self.tables
    }

    /// Voices that went non-finite and were started again.
    pub fn resets(&self) -> u64 {
        self.section.resets
    }

    /// Does this key sound?
    pub fn sounding(&self, key: u8) -> bool {
        self.section.voices.iter().any(|v| v.cur == Some(key))
    }

    pub fn post(&mut self, e: Ev) {
        self.section.post(e);
    }

    fn apply(&mut self, p: &BrassParams, variation: f32) {
        let s = &mut self.section;
        // by its kind: one the instrument lacks plays open, as the window says
        let mute = MUTE_KINDS.get(p.mute as usize).and_then(|k| self.tables.mutes.iter().position(|m| m == k)).unwrap_or(0);
        let glide_mode = match p.glide_mode {
            _ if !self.slide => GlideMode::Off,
            0 => GlideMode::Off,
            2 => GlideMode::On,
            _ => GlideMode::Auto,
        };
        let after = if p.aftertouch_to == 1 { Aftertouch::Flutter } else { Aftertouch::Growl };
        for v in &mut s.voices {
            v.next_mute = mute;
            v.growl = f64::from(p.growl);
            v.flutter = f64::from(p.flutter);
            v.vib_knob = f64::from(p.vibrato);
            v.plunger_cc = p.plunger.map(f64::from);
            v.pedal = p.air.map(f64::from);
            v.virtual_player = p.virtual_player;
            v.glide = p.glide_hold;
            v.glide_t = f64::from(p.glide_time);
            v.glide_mode = glide_mode;
            v.aftertouch_to = after;
            v.wheel = f64::from(p.air_added) * 127.0;
            v.variation = f64::from(variation);
        }
        if p.solo != self.solo {
            self.solo = p.solo;
            s.post(Ev::Solo(p.solo));
        }
    }

    /// `n` samples at the engine's rate into `out` (left, right) and `room` (the room's feed).
    pub fn render(&mut self, p: &BrassParams, variation: f32, n: usize, l: &mut [f64], r: &mut [f64], room: &mut [f64]) {
        self.apply(p, variation);
        let mut mix = [[0.0; CTRL]; 2];
        while self.len < n {
            self.section.step(&mut mix);
            let (mut a, mut b, mut c) = ([0.0f32; CTRL], [0.0f32; CTRL], [0.0f32; CTRL]);
            for k in 0..CTRL {
                let (x, y) = (mix[0][k].tanh(), mix[1][k].tanh());
                a[k] = x as f32;
                b[k] = y as f32;
                c[k] = (0.5 * (x + y)) as f32;
            }
            self.room_out[0].fill(0.0);
            self.room.process(&c, &mut self.room_out);
            // the microphones, the one before fading out under a handover
            let (dry_a, dry_b) = (a, b);
            if let Some(m) = &mut self.mic {
                m.run(&dry_a, &dry_b);
                a.copy_from_slice(&m.out[0][0]);
                b.copy_from_slice(&m.out[1][0]);
            }
            if self.fading > 0 {
                let (oa, ob) = match &mut self.mic_old {
                    Some(m) => {
                        m.run(&dry_a, &dry_b);
                        let mut x = [0.0f32; CTRL];
                        let mut y = [0.0f32; CTRL];
                        x.copy_from_slice(&m.out[0][0]);
                        y.copy_from_slice(&m.out[1][0]);
                        (x, y)
                    }
                    None => (dry_a, dry_b),
                };
                for k in 0..CTRL {
                    let left = self.fading.saturating_sub(k) as f32 / MIC_FADE as f32;
                    a[k] = a[k] * (1.0 - left) + oa[k] * left;
                    b[k] = b[k] * (1.0 - left) + ob[k] * left;
                }
                self.fading = self.fading.saturating_sub(CTRL);
            }
            for k in 0..CTRL {
                let (fifo, head, len) = (&mut self.fifo, self.head, &mut self.len);
                self.stream.push([f64::from(a[k]), f64::from(b[k]), f64::from(self.room_k * self.room_out[0][k])], |y| {
                    if *len < OUT {
                        fifo[(head + *len) % OUT] = [y[0] as f32, y[1] as f32, y[2] as f32];
                        *len += 1;
                    }
                });
            }
        }
        for j in 0..n {
            let [a, b, c] = self.fifo[self.head];
            l[j] = f64::from(a);
            r[j] = f64::from(b);
            room[j] = f64::from(c);
            self.head = (self.head + 1) % OUT;
        }
        self.len -= n;
    }

    /// Everything that sounds gone: no state rings on.
    pub fn clear(&mut self) {
        self.section.clear();
        self.stream.clear();
        self.room.clear();
        if let Some(m) = &mut self.mic {
            m.clear();
        }
        self.fading = 0;
        self.head = 0;
        self.len = 0;
    }

    /// A handover on one instrument (a microphone moved): the players go on.
    pub fn take_over(&mut self, old: &mut Family) {
        std::mem::swap(&mut self.section, &mut old.section);
        for v in &mut self.section.voices {
            v.set_tuning(self.tune.clone());
        }
        std::mem::swap(&mut self.stream, &mut old.stream);
        std::mem::swap(&mut self.fifo, &mut old.fifo);
        self.head = old.head;
        self.len = old.len;
        self.solo = old.solo;
        // the path before fades out; the one before that goes with the old engine, off the audio thread
        std::mem::swap(&mut self.mic_old, &mut old.mic);
        self.fading = MIC_FADE;
        std::mem::swap(&mut self.room, &mut old.room);
    }
}
