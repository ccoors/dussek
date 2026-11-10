//! A row of players behind one set of keys and controllers (`Section` in `live.py`): Auto, a line one player and a chord a
//! section; Solo, one player, the last key wins. Each has its own place in the streams and its own seat.

use super::player::{Aftertouch, DT, GlideMode, Live};
use super::streams::CTRL;
use super::tables::Tables;
use std::f64::consts::PI;
use std::sync::Arc;

/// Keys this close are a chord (s); a later key this near one held is a slur (semitones), further is more harmony.
const CHORD_S: f64 = 0.03;
const SLUR_SPAN: f64 = 5.0;
const GLIDE_SPAN: f64 = 6.0;
/// A key still down under a slur: a silent player starts on it (s after the slur), speaks unheard (s), then fades in (s).
const TAKE_S: f64 = 0.07;
const HIDE_S: f64 = 0.09;
const RISE_S: f64 = 0.06;
pub const VOICES: usize = 6;
const SEATS: [f64; VOICES] = [0.0, -0.3, 0.3, -0.6, 0.6, -0.15]; // across the stereo image; the first alone is the mono soloist
const QUEUE: usize = 256;

#[derive(Clone, Copy, Debug)]
pub enum Ev {
    On(u8, f64),
    Off(u8),
    Bend(f64),
    Wheel(f64),
    Air(f64),
    Vibrato(f64),
    Growl(f64),
    Flutter(f64),
    Glide(bool),
    GlideTime(f64),
    Plunger(f64),
    Aftertouch(f64),
    AllOff,
    Solo(bool),
}

#[derive(Clone, Copy)]
struct Down {
    key: u8,
    voice: usize,
    vel: f64,
    when: f64,
}

#[derive(Clone, Copy)]
struct Taking {
    key: u8,
    voice: usize,
    when: f64,
}

pub struct Section {
    pub voices: Vec<Live>,
    pans: Vec<(f64, f64)>,
    pub solo: bool,
    keys: Vec<Down>,
    harmony: [bool; 128],
    taking: Vec<Taking>,
    fade: Vec<f64>,
    last_on: f64,
    now: f64,
    queue: Box<[Option<Ev>; QUEUE]>,
    head: usize,
    tail: usize,
    trim: f64,
    /// voices that went non-finite and were started again
    pub resets: u64,
    tables: Arc<Tables>,
}

impl Section {
    pub fn new(tables: Arc<Tables>, voices: Vec<Live>) -> Section {
        let pans = SEATS[..voices.len()]
            .iter()
            .map(|p| {
                // the middle stays at 1 in both channels: the soloist's level as it was
                let a = PI / 4.0 * (1.0 + p);
                (a.cos() * 2f64.sqrt(), a.sin() * 2f64.sqrt())
            })
            .collect();
        let n = voices.len();
        Section {
            voices,
            pans,
            solo: false,
            keys: Vec::with_capacity(128),
            harmony: [false; 128],
            taking: Vec::with_capacity(128),
            fade: vec![1.0; n],
            last_on: -9.0,
            now: 0.0,
            queue: Box::new([None; QUEUE]),
            head: 0,
            tail: 0,
            trim: 1.0,
            resets: 0,
            tables,
        }
    }

    /// An event for the next steps (the oldest stays: a full queue drops the newest).
    pub fn post(&mut self, e: Ev) {
        let next = (self.tail + 1) % QUEUE;
        if next != self.head {
            self.queue[self.tail] = Some(e);
            self.tail = next;
        }
    }

    pub fn sounding(&self) -> usize {
        self.voices.iter().filter(|v| v.cur.is_some()).count()
    }

    fn handle(&mut self, e: Ev) {
        match e {
            Ev::On(m, v) => self.note_on(m, v),
            Ev::Off(m) => self.note_off(m),
            Ev::Bend(x) => self.voices.iter_mut().for_each(|v| v.pb = x),
            Ev::Wheel(x) => self.voices.iter_mut().for_each(|v| v.wheel = x),
            Ev::Air(x) => self.voices.iter_mut().for_each(|v| v.pedal = Some(x)),
            Ev::Vibrato(x) => self.voices.iter_mut().for_each(|v| v.vib_knob = x),
            Ev::Growl(x) => self.voices.iter_mut().for_each(|v| v.growl = x),
            Ev::Flutter(x) => self.voices.iter_mut().for_each(|v| v.flutter = x),
            Ev::Glide(x) => self.voices.iter_mut().for_each(|v| v.glide = x),
            Ev::GlideTime(x) => self.voices.iter_mut().for_each(|v| v.glide_t = x),
            Ev::Plunger(x) => self.voices.iter_mut().for_each(|v| v.plunger_cc = Some(x)),
            Ev::Aftertouch(x) => self.voices.iter_mut().for_each(|v| v.aftertouch(x)),
            Ev::AllOff => {
                self.keys.clear();
                for v in &mut self.voices {
                    v.held.clear();
                    v.release();
                }
            }
            Ev::Solo(s) => {
                self.quiet();
                self.solo = s;
            }
        }
    }

    pub fn set_mute(&mut self, mute: usize) {
        self.voices.iter_mut().for_each(|v| v.next_mute = mute);
    }

    pub fn set_virtual(&mut self, on: bool) {
        self.voices.iter_mut().for_each(|v| v.virtual_player = on);
    }

    pub fn set_variation(&mut self, x: f64) {
        self.voices.iter_mut().for_each(|v| v.variation = x);
    }

    pub fn set_glide_mode(&mut self, m: GlideMode) {
        self.voices.iter_mut().for_each(|v| v.glide_mode = m);
    }

    pub fn set_aftertouch_to(&mut self, a: Aftertouch) {
        self.voices.iter_mut().for_each(|v| v.aftertouch_to = a);
    }

    /// The air and the plunger back to the keys.
    pub fn clear_air(&mut self) {
        self.voices.iter_mut().for_each(|v| v.pedal = None);
    }

    pub fn clear_plunger(&mut self) {
        self.voices.iter_mut().for_each(|v| v.plunger_cc = None);
    }

    fn quiet(&mut self) {
        self.keys.clear();
        self.harmony = [false; 128];
        self.taking.clear();
        self.fade.fill(1.0);
        for v in &mut self.voices {
            v.held.clear();
            v.release();
        }
    }

    /// Everything that sounds gone, the keys up, no state ringing on.
    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.keys.clear();
        self.harmony = [false; 128];
        self.taking.clear();
        self.fade.fill(1.0);
        self.trim = 1.0;
        for v in &mut self.voices {
            v.clear();
        }
    }

    fn free(&self) -> Option<usize> {
        self.voices.iter().position(|v| v.held.is_empty())
    }

    fn steal(&mut self) -> usize {
        // every player busy: the oldest key's gives way (usually a chord on its way up)
        let mut oldest = 0;
        for (i, d) in self.keys.iter().enumerate() {
            if d.when < self.keys[oldest].when {
                oldest = i;
            }
        }
        let v = self.keys[oldest].voice;
        self.keys.retain(|d| d.voice != v);
        self.voices[v].held.clear();
        v
    }

    fn down(&self, m: u8) -> Option<usize> {
        self.keys.iter().position(|d| d.key == m)
    }

    fn note_on(&mut self, m: u8, v: f64) {
        if !self.tables.plays(m) {
            return;
        }
        if self.solo {
            self.voices[0].note_on(m, v);
            return;
        }
        if self.down(m).is_some() {
            self.note_off(m);
        }
        let gap = self.now - self.last_on;
        let one = if self.keys.len() == 1 { Some(self.keys[0]) } else { None };
        let span = if self.voices[0].glide || self.voices[0].glide_mode == GlideMode::On { GLIDE_SPAN } else { SLUR_SPAN };
        let t = match one {
            Some(d) if gap > CHORD_S && f64::from(i32::from(m) - i32::from(d.key)).abs() <= span && !self.harmony[usize::from(d.key)] => d.voice,
            _ => match self.free() {
                Some(f) => f,
                None => self.steal(),
            },
        };
        self.fade[t] = 1.0;
        self.voices[t].note_on(m, v);
        self.keys.push(Down { key: m, voice: t, vel: v, when: self.now });
        self.last_on = self.now;
    }

    fn note_off(&mut self, m: u8) {
        if self.solo {
            self.voices[0].note_off(m);
        } else if let Some(i) = self.down(m) {
            let d = self.keys.remove(i);
            self.voices[d.voice].note_off(m);
        }
        self.harmony[usize::from(m)] = false;
    }

    fn under(&self, m: u8) -> bool {
        // is this key down but given up to a slur?
        match self.down(m) {
            Some(i) => {
                let t = &self.voices[self.keys[i].voice];
                t.held.len() > 1 && t.held.last() != Some(m)
            }
            None => false,
        }
    }

    fn keep(&mut self) {
        // the weak spot of a slur: it may have been harmony. A key left down under one keeps its note
        let mut i = 0;
        while i < self.taking.len() {
            if !self.under(self.taking[i].key) {
                // legato after all: nothing was heard of it
                let w = self.taking.remove(i).voice;
                self.voices[w].held.clear();
                self.voices[w].release();
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.keys.len() {
            let d = self.keys[i];
            if self.under(d.key) {
                match self.taking.iter().position(|x| x.key == d.key) {
                    None => {
                        let last = self.voices[d.voice].held.last();
                        let since = last.and_then(|l| self.down(l)).map(|j| self.keys[j].when);
                        if let (Some(w), Some(when)) = (self.free(), since) {
                            if self.now - when > TAKE_S {
                                self.fade[w] = 0.0;
                                self.voices[w].note_on(d.key, d.vel);
                                self.taking.push(Taking { key: d.key, voice: w, when: self.now });
                            }
                        }
                    }
                    Some(j) if self.now - self.taking[j].when > HIDE_S => {
                        let w = self.taking.remove(j).voice;
                        self.voices[d.voice].held.remove(d.key);
                        self.keys[i] = Down { key: d.key, voice: w, vel: d.vel, when: self.now };
                        self.harmony[usize::from(d.key)] = true;
                        self.fade[w] = DT / RISE_S;
                    }
                    Some(_) => {}
                }
            }
            i += 1;
        }
    }

    /// One control step: two channels, each player's output gain and seat applied.
    pub fn step(&mut self, mix: &mut [[f64; CTRL]; 2]) {
        // a chord's players start one per step: the setting up is the slow part
        let mut started = false;
        while self.head != self.tail {
            let e = self.queue[self.head].take();
            if let Some(Ev::On(..)) = e {
                if started {
                    self.queue[self.head] = e;
                    break;
                }
                started = true;
            }
            self.head = (self.head + 1) % QUEUE;
            if let Some(e) = e {
                self.handle(e);
            }
        }
        self.now += DT;
        if !self.solo && (!self.taking.is_empty() || self.keys.len() > 1) {
            self.keep();
        }
        mix[0].fill(0.0);
        mix[1].fill(0.0);
        // the row's level: down at once as players join, back slowly; a chord still ends up louder than one player
        let n = self
            .voices
            .iter()
            .zip(&self.fade)
            .filter(|(v, f)| v.cur.is_some() && **f > 0.0 && v.releasing.is_none_or(|r| r < v.rel_len))
            .count();
        let want = (n.max(1) as f64).powf(-0.5);
        let trim0 = self.trim;
        self.trim += (want - self.trim) * if want < self.trim { 0.1 } else { 0.01 };
        let dtrim = (self.trim - trim0) / CTRL as f64;
        let mut x = [0.0; CTRL];
        for i in 0..self.voices.len() {
            let v = &mut self.voices[i];
            if v.cur.is_none() {
                v.now += DT;
                continue;
            }
            v.step(&mut x);
            if !x.iter().sum::<f64>().is_finite() {
                v.reset();
                self.resets += 1;
                continue;
            }
            let f = self.fade[i];
            if f == 0.0 {
                continue; // speaking unheard
            }
            let g = v.gain;
            let (l, r) = self.pans[i];
            let f1 = (f + DT / RISE_S).min(1.0);
            for k in 0..CTRL {
                let mut y = x[k] * g * (trim0 + dtrim * k as f64);
                if f < 1.0 {
                    y *= f + (f1 - f) * k as f64 / CTRL as f64;
                }
                mix[0][k] += y * l;
                mix[1][k] += y * r;
            }
            if f < 1.0 {
                self.fade[i] = f1;
            }
        }
    }
}
