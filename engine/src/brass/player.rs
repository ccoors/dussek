//! One player (`Live` in `live.py`): the keys and controllers in, a control step of sound out. One note at a time, the
//! last key wins. Every number carried over by name; a note's setup does not allocate.

use super::core::{Core, CoreParams, MAXM, ModeBank, OS, Ramp, SR};
use super::shape::{DrawSource, Shape, level_of};
use super::streams::{CTRL, Streams};
use super::tables::{Modes, Note, Tables};
use std::f64::consts::PI;
use std::sync::Arc;

pub const DT: f64 = CTRL as f64 / SR;
/// control steps of the fade before a note that starts from rest (4.4 ms)
const DUCK: usize = 3;
const GLIDE_SPAN: f64 = 6.0; // a tritone, the slide's whole travel
const GLIDE_T: f64 = 0.22; // s for a minor third
const H_X: [f64; 3] = [0.315, 0.59, 0.9]; // the air each table's filter was fitted at
const LEVEL_X: [f64; 4] = [0.315, 0.55, 0.9, 1.0]; // the air's anchors of the level, 40 to 127
const GROWL: f64 = 0.3;
const FLUTTER_DEPTH: f64 = 0.6;
const SLUR_KICK: f64 = 0.5;
const SLUR_HARD: f64 = 100.0 / 127.0;
const GAIN: f64 = 0.25;
const TUNE_FOLLOW: f64 = 0.6; // how much of the slides' difference the note needs as the air moves
// a slur: the lips ease past the note (cents, s) and the air dips, all after the key
const SLACK_DOWN: f64 = 200.0;
const SLACK_UP: f64 = 100.0;
const SLACK_T: f64 = 0.04;
const DIP: f64 = 0.8;
const DIP_T: f64 = 0.04;
// the attack by dynamics (compare5.attack_for)
const KICK: f64 = 0.5;
const BURST: f64 = 5.0;

/// The glide's mode (`Live.glide_mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlideMode {
    Off,
    Auto,
    On,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aftertouch {
    Growl,
    Flutter,
}

#[derive(Clone, Copy, Debug, Default)]
struct Moving {
    lp: [f64; 3],
    ce: [f64; 3],
    top: f64,
    st: [(f64, f64); 3],
}

#[derive(Clone, Copy, Debug)]
struct Glide {
    a: f64,
    b: f64,
    t0: f64,
    length: f64,
}

pub struct Held {
    k: [u8; 128],
    n: usize,
}

impl Held {
    fn new() -> Held {
        Held { k: [0; 128], n: 0 }
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    pub fn len(&self) -> usize {
        self.n
    }
    pub fn last(&self) -> Option<u8> {
        if self.n > 0 { Some(self.k[self.n - 1]) } else { None }
    }
    pub fn clear(&mut self) {
        self.n = 0;
    }
    pub fn remove(&mut self, m: u8) {
        let mut j = 0;
        for i in 0..self.n {
            if self.k[i] != m {
                self.k[j] = self.k[i];
                j += 1;
            }
        }
        self.n = j;
    }
    fn push(&mut self, m: u8) {
        self.remove(m);
        if self.n < 128 {
            self.k[self.n] = m;
            self.n += 1;
        }
    }
}

/// `mode_cents`: where this dynamics' slide puts the note's mode, in cents from f.
fn mode_cents(modes: &Modes, f: f64) -> f64 {
    let mut best = (f64::INFINITY, f);
    for im in modes.sp_im.iter() {
        let fm = im.abs() / (2.0 * PI);
        let d = (fm / f).ln().abs();
        if d < best.0 {
            best = (d, fm);
        }
    }
    1200.0 * (best.1 / f).log2()
}

pub struct Live {
    tables: Arc<Tables>,
    streams: Streams,
    draws: Box<dyn DrawSource>,
    core: Core,
    params: CoreParams,
    bank: ModeBank,
    // the current dynamics' modes per oversampled step, before the pitch's ratio
    st_re: [f64; MAXM],
    st_im: [f64; MAXM],
    rt_re: [f64; MAXM],
    rt_im: [f64; MAXM],
    m_count: usize,
    pub mute: usize,
    pub next_mute: usize,
    pub virtual_player: bool,
    /// how much a note differs from the last in the player's hands (1: as fitted)
    pub variation: f64,
    t_idx: usize,
    /// keys down, oldest first
    pub held: Held,
    /// the note sounding
    pub cur: Option<u8>,
    tau: f64,
    kick: f64,
    pub releasing: Option<f64>,
    pub rel_len: f64,
    /// control steps left of the fade before a note that starts from rest (`attack_from_rest`)
    duck: usize,
    /// the note waiting for the fade, and whether its key came up meanwhile
    pending: Option<(u8, f64)>,
    pending_off: bool,
    pm: f64,
    lip: f64,
    burst: f64,
    prev_ratio: Option<f64>,
    pub pb: f64,
    pub vib_knob: f64,
    pub pedal: Option<f64>,
    pub wheel: f64,
    pub glide: bool,
    pub glide_t: f64,
    pub glide_mode: GlideMode,
    gl: Option<Glide>,
    gl_g: f64,
    level_from: f64,
    trim_a: f64,
    trim_b: f64,
    at_a: Moving,
    at_b: Option<Moving>,
    at_now: Option<Moving>,
    lip_trim: f64,
    smooth_air: Option<f64>,
    air_now: Option<f64>,
    pub growl: f64,
    pub flutter: f64,
    at_growl: f64,
    at_flutter: f64,
    pub aftertouch_to: Aftertouch,
    vib_phase: f64,
    fl_phase: f64,
    count: u32,
    level: Option<f64>,
    last_air: f64,
    level_to: f64,
    level_now: f64,
    level_g: f64,
    pub plunger_cc: Option<f64>,
    opened: f64,
    h_x: f64,
    last_off: f64,
    pub now: f64,
    pub gain: f64,
    shape: Shape,
    key: u8,
    v: f64,
    slur: bool,
    base: u8,
    place: usize,
    warp: f64,
    mu: f64,
    settings: [(f64, f64); 3],
    modes_c: [f64; 3],
    mode_here: f64,
    dyn_here: usize,
    lip_from: f64,
    down: bool,
    first: f64,
    /// the attack's rise to `first`, s
    rise: f64,
    rise_from: f64,
    scoop: f64,
    burst_add: f64,
    /// a voice that went non-finite starts its state again from here in the noise
    noise_at: usize,
    /// per key the tuning re the fit, semitones (concert pitch, temperament)
    tune: Arc<[f64; 128]>,
}

impl Live {
    /// `offset`: where in the streams this player reads.
    pub fn new(tables: Arc<Tables>, streams: Streams, draws: Box<dyn DrawSource>, mute: usize, virtual_player: bool, offset: usize) -> Live {
        let gain = GAIN * 10f64.powf((tables.makeup_db[mute] + tables.gain_db) / 20.0);
        let params = CoreParams { width: tables.width, y0: tables.y0, soft: tables.soft, rho: tables.rho, nl: tables.nl };
        let glide_mode = if tables.slide { GlideMode::Auto } else { GlideMode::Off };
        let noise_at = offset.wrapping_mul(977) % streams.noise.len();
        let mut core = Core::new();
        core.set_noise_pos(noise_at);
        Live {
            t_idx: offset % streams.wp.len(),
            tables,
            streams,
            draws,
            core,
            params,
            bank: ModeBank::default(),
            st_re: [0.0; MAXM],
            st_im: [0.0; MAXM],
            rt_re: [0.0; MAXM],
            rt_im: [0.0; MAXM],
            m_count: 0,
            mute,
            next_mute: mute,
            virtual_player,
            variation: 1.0,
            held: Held::new(),
            cur: None,
            tau: 0.0,
            kick: 0.0,
            releasing: None,
            duck: 0,
            pending: None,
            pending_off: false,
            rel_len: 0.0,
            pm: 0.0,
            lip: 200.0,
            burst: 1.0,
            prev_ratio: None,
            pb: 0.0,
            vib_knob: 0.0,
            pedal: None,
            wheel: 0.0,
            glide: false,
            glide_t: GLIDE_T,
            glide_mode,
            gl: None,
            gl_g: 0.0,
            level_from: 0.0,
            trim_a: 0.0,
            trim_b: 0.0,
            at_a: Moving::default(),
            at_b: None,
            at_now: None,
            lip_trim: 0.0,
            smooth_air: None,
            air_now: None,
            growl: 0.0,
            flutter: 0.0,
            at_growl: 0.0,
            at_flutter: 0.0,
            aftertouch_to: Aftertouch::Growl,
            vib_phase: 0.0,
            fl_phase: 0.0,
            count: 0,
            level: None,
            last_air: 80.0,
            level_to: 0.0,
            level_now: 0.0,
            level_g: 1.0,
            plunger_cc: None,
            opened: 0.0,
            h_x: 0.0,
            last_off: -9.0,
            now: 0.0,
            gain,
            shape: Shape::default(),
            key: 0,
            v: 0.0,
            slur: false,
            base: 0,
            place: 0,
            warp: 0.0,
            mu: 0.0,
            settings: [(0.0, 0.0); 3],
            modes_c: [0.0; 3],
            mode_here: 0.0,
            dyn_here: 1,
            lip_from: 0.0,
            down: false,
            first: 1.0,
            rise: 0.008,
            rise_from: 0.0,
            scoop: 0.0,
            burst_add: 0.0,
            noise_at,
            tune: Arc::new([0.0; 128]),
        }
    }

    pub fn set_tuning(&mut self, tune: Arc<[f64; 128]>) {
        self.tune = tune;
    }

    fn note_table(&self) -> &Note {
        self.tables.note(self.mute, self.base)
    }

    fn near(d: f64) -> usize {
        let x = ((d - 0.2) / 0.7).clamp(0.0, 1.0);
        let (mut best, mut at) = (f64::INFINITY, 0);
        for (i, v) in [0.0, 0.55, 1.0].iter().enumerate() {
            let e = (v - x).abs();
            if e < best {
                best = e;
                at = i;
            }
        }
        at
    }

    fn glides(&self, m: u8, v: f64) -> bool {
        // does this slur go by the slide? When held down for it; or, left to the player, a soft key after a long note, close by
        if !self.tables.slide || self.cur.is_none() || self.releasing.is_some() {
            return false;
        }
        if (i32::from(m) - i32::from(self.base)).abs() as f64 > GLIDE_SPAN {
            return false;
        }
        if self.glide || self.glide_mode == GlideMode::On {
            return true;
        }
        let cur = self.cur.unwrap_or(m);
        self.glide_mode == GlideMode::Auto && self.virtual_player && v <= 70.0 && self.tau >= 0.35 && (i32::from(m) - i32::from(cur)).abs() <= 5
    }

    fn at_of(&self, key: u8, settings: [(f64, f64); 3]) -> Moving {
        let a = self.tables.at(self.tables.place(key));
        Moving { lp: a.lp, ce: a.ce, top: a.top, st: settings }
    }

    fn setting_of(&self, key: u8, d: usize) -> (f64, f64) {
        let (dc, mut air) = self.note_table().settings[d];
        air *= self.tables.speak_air(key, d);
        match self.tables.cell(self.mute, key, d) {
            Some(o) => (dc + o.0, air * o.1),
            None => (dc, air),
        }
    }

    fn slide_to(&mut self, m: u8) {
        // the same note, the tube drawn out: nothing starts, the pitch moves as the modes and lips do
        let now = self.warp;
        let near = self.tables.near(m);
        // this mode goes where the key's own table has its mode: where the lips' setting for the key plays in tune
        let n = self.tables.note(self.mute, near);
        let there = mode_cents(&n.modes[self.dyn_here], n.f);
        let cur = self.cur.unwrap_or(m);
        let off = (there - self.mode_here) / 100.0;
        let span = (f64::from(i32::from(m) - i32::from(cur))).abs().max(0.5);
        self.gl = Some(Glide { a: now, b: f64::from(i32::from(m) - i32::from(self.base)) + off, t0: self.now, length: self.glide_t * (span / 3.0).sqrt() });
        self.trim_a = self.lip_trim;
        self.trim_b = off; // the lips stay on the key, not on that offset
        self.at_a = if self.at_b.is_some() { self.at_now.unwrap_or_default() } else { self.at_of(self.base, self.settings) };
        // the air and lips go where the key's own table has them: a higher note needs more, or it dies on the way up
        let st = [self.setting_of(m, 0), self.setting_of(m, 1), self.setting_of(m, 2)];
        self.at_b = Some(self.at_of(near, st));
        self.at_now = Some(self.at_a);
        self.level_from = self.level_now;
        self.gl_g = 0.0;
        self.cur = Some(m);
        self.key = m;
    }

    /// A tongued note on a release still sounding, with `attack_from_rest`: the last note's state would ring into this attack,
    /// so it fades out over `DUCK` control steps and the note then starts from rest.
    fn waits(&self, slur: bool) -> bool {
        self.pending.is_some() || (self.tables.attack_from_rest && !slur && self.releasing.is_some() && self.cur.is_some())
    }

    fn start(&mut self, m: u8, v: f64, slur: bool) {
        if self.waits(slur) {
            // a key during the fade takes the waiting note's place
            if self.pending.is_none() {
                self.duck = DUCK;
            }
            self.pending = Some((m, v));
            self.pending_off = false;
            return;
        }
        self.start_now(m, v, slur);
    }

    fn start_now(&mut self, m: u8, v: f64, slur: bool) {
        if slur && self.glides(m, v) {
            self.slide_to(m);
            return;
        }
        self.gl = None;
        self.at_b = None;
        self.at_now = None;
        self.lip_trim = 0.0;
        let base = self.tables.near(m);
        let warp = f64::from(i32::from(m) - i32::from(base));
        let fresh = self.now - self.last_off > 1.0;
        let d = self.draws.draw(self.count);
        self.shape = Shape::new(&self.tables, &d, m, v, slur, fresh, self.level, self.last_air, self.variation);
        self.level = Some(self.shape.level);
        self.count += 1;
        let prev_lip = self.lip;
        self.key = m;
        self.v = v;
        self.slur = slur;
        self.base = base;
        self.place = self.tables.place(base);
        self.warp = warp;
        self.tau = 0.0;
        self.releasing = None;
        let dl = self.dyn_level(0.0) / 127.0;
        let dd = Self::near(dl);
        let tables = self.tables.clone();
        let n = tables.note(self.mute, base);
        self.mu = tables.lip_mu(n.f);
        // the lips' and air's setting for this key on this mute, per dynamics: followed as the air moves
        self.settings = [self.setting_of(m, 0), self.setting_of(m, 1), self.setting_of(m, 2)];
        let md = &n.modes[dd];
        self.m_count = md.len().min(MAXM);
        let t_step = 1.0 / (SR * OS as f64);
        for j in 0..self.m_count {
            self.st_re[j] = md.sp_re[j] * t_step;
            self.st_im[j] = md.sp_im[j] * t_step;
            self.rt_re[j] = md.rc_re[j] * t_step / 2.0;
            self.rt_im[j] = md.rc_im[j] * t_step / 2.0;
        }
        if n.open.is_some() && !slur {
            self.opened = 0.0;
        }
        self.h_x = dl.max(0.2);
        let (hx, opened) = (self.h_x, self.opened);
        self.core.change_filters(n.h[0].len(), n.r.clone(), |to| leaving(n, hx, opened, to));
        // where each dynamics' slide puts the note's mode: more air plays sharper, the player corrects by ear
        self.modes_c = [mode_cents(&n.modes[0], n.f), mode_cents(&n.modes[1], n.f), mode_cents(&n.modes[2], n.f)];
        self.mode_here = self.modes_c[dd];
        self.dyn_here = dd;
        self.lip_from = prev_lip;
        self.down = slur && self.cur.is_some_and(|c| m < c);
        self.cur = Some(m);
        let vel = v / 100.0;
        if slur {
            self.kick = SLUR_KICK * (0.6 + 0.4 * SLUR_HARD);
            self.first = 1.0;
            self.scoop = 0.0;
            self.burst_add = 0.0;
            return;
        }
        let x = ((dl - 0.2) / 0.7).clamp(0.0, 1.0);
        let lean = 1.3 - 0.6 * x;
        self.first = (if tables.attack_lean { lean } else { lean.max(1.0) }).max(self.push(m, dl));
        // the pressure's rise: a few periods of the note, else a low note's bore rings louder than it then holds
        self.rise = super::tables::attack_rise(tables.attack_rise_periods, tables.attack_rise_hz, n.f);
        self.rise_from = super::tables::attack_share(tables.attack_rise_from, tables.attack_rise_periods, tables.attack_rise_hz, n.f);
        self.scoop = 300.0 - 180.0 * x;
        if let Some(span) = tables.scoop_span {
            // lips slack by more than the way to the next mode down catch on that mode first
            let mut below = f64::NEG_INFINITY;
            for im in md.sp_im.iter() {
                let fm = im.abs() / (2.0 * PI);
                if fm < n.f * 0.97 && fm > below {
                    below = fm;
                }
            }
            if below > f64::NEG_INFINITY {
                self.scoop = self.scoop.min(span * 1200.0 * (n.f / below).log2());
            }
        }
        self.kick = KICK * (0.4 + 0.6 * vel.min(1.2));
        self.burst_add = BURST * vel.min(1.2);
        self.gain = GAIN * 10f64.powf((tables.makeup_db[self.mute] + tables.gain_db) / 20.0);
        self.level_now = self.level_db(dl.max(0.2)); // a new note starts at its own level
        self.level_g = 10f64.powf(self.level_now / 20.0);
    }

    fn level_db(&self, x: f64) -> f64 {
        // the output's level for this key at this air, between the fitted anchors
        if !self.tables.has_level {
            return 0.0;
        }
        let l = self.tables.level(self.mute, self.tables.place(self.cur.unwrap_or(self.base)));
        let mut y = crate::noise::interp(x, &LEVEL_X, &l.anchors) + l.step[self.dyn_here];
        if self.gl.is_some() {
            // a glide takes the level along with the air
            y = self.level_from + (y - self.level_from) * self.gl_g;
        }
        y
    }

    fn push(&self, key: u8, x: f64) -> f64 {
        // the attack's push for this key at this air: at least what it takes to speak at once
        let (a, b, c) = (self.tables.speak_first(key, 0), self.tables.speak_first(key, 1), self.tables.speak_first(key, 2));
        let w = ((x - 0.2) / 0.7).clamp(0.0, 1.0);
        if w < 0.55 { a + (b - a) * w / 0.55 } else { b + (c - b) * (w - 0.55) / 0.45 }
    }

    pub fn note_on(&mut self, m: u8, v: f64) {
        if !self.tables.plays(m) {
            return;
        }
        let slur = !self.held.is_empty() && self.cur.is_some();
        self.held.push(m);
        if !slur && self.next_mute != self.mute && !self.waits(slur) {
            self.mute = self.next_mute;
        }
        self.start(m, v, slur);
    }

    pub fn note_off(&mut self, m: u8) {
        let was_last = self.held.last() == Some(m);
        self.held.remove(m);
        if was_last {
            if let Some(k) = self.held.last() {
                // back to the key still down: a slur
                self.start(k, self.v, true);
            } else if self.pending.is_some() {
                // the waiting note's key came up: it still sounds, and releases at once
                self.pending_off = true;
            } else {
                self.release();
            }
        }
    }

    pub fn release(&mut self) {
        if self.pending.take().is_some() {
            // the note waiting for the fade never starts; the fade ends in silence
            self.pending_off = false;
            return;
        }
        if self.cur.is_none() {
            return;
        }
        self.releasing = Some(0.0);
        let r: f64 = if self.tau > 0.8 { 60.0 } else { 40.0 };
        self.rel_len = 0.02 + 0.6 * (r / 127.0).powi(2);
        self.last_air = self.air_now.unwrap_or(80.0);
        self.last_off = self.now;
    }

    pub fn aftertouch(&mut self, v: f64) {
        match self.aftertouch_to {
            Aftertouch::Growl => {
                self.at_growl = v / 127.0;
                self.at_flutter = 0.0;
            }
            Aftertouch::Flutter => {
                self.at_flutter = v / 127.0;
                self.at_growl = 0.0;
            }
        }
    }

    fn dyn_level(&self, t: f64) -> f64 {
        // the air (0-127): a knob or pedal if one moved, else the keys; the wheel adds to that
        let b = if let Some(p) = self.pedal {
            p
        } else if self.virtual_player {
            self.shape.air(t)
        } else {
            level_of(&self.tables, self.v, self.key).min(127.0)
        };
        b + (127.0 - b) * self.wheel / 127.0 // the wheel's whole travel, from there to the top
    }

    /// One control step into `out` (before the output's gain).
    pub fn step(&mut self, out: &mut [f64; CTRL]) {
        self.step_note(out);
        if self.duck == 0 {
            return;
        }
        let total = (DUCK * CTRL) as f64;
        let done = ((DUCK - self.duck) * CTRL) as f64;
        for (k, x) in out.iter_mut().enumerate() {
            *x *= 0.5 + 0.5 * (PI * (done + k as f64 + 1.0) / total).cos();
        }
        self.duck -= 1;
        if self.duck > 0 {
            return;
        }
        // faded out: the state cleared (the release would come back at its level), the note waiting started from rest
        self.reset();
        if let Some((m, v)) = self.pending.take() {
            self.mute = self.next_mute;
            self.start_now(m, v, false);
            if std::mem::take(&mut self.pending_off) {
                self.release();
            }
        } else {
            // the note waiting was let go: the last one ends with the fade (its release's air would blow the bore again)
            self.cur = None;
            self.releasing = None;
            self.pm = 0.0;
        }
    }

    fn step_note(&mut self, out: &mut [f64; CTRL]) {
        self.now += DT;
        self.t_idx = (self.t_idx + 1) % self.streams.wp.len();
        let i = self.t_idx;
        let (mut pm1, mut lip1, mut burst1, mut ratio) = (0.0, self.lip, 1.0, 1.0);
        if self.cur.is_some() {
            let t = self.tau;
            let target = self.dyn_level(t);
            // controllers come in steps: eased over about 10 ms (from the note's own level at its start)
            let l_air = match self.smooth_air {
                Some(s) if t != 0.0 => s + (target - s) * 0.14,
                _ => target,
            };
            self.smooth_air = Some(l_air);
            self.air_now = Some(l_air);
            if self.releasing.is_none_or(|r| r == 0.0 || r < self.rel_len) {
                self.last_air = l_air;
            }
            let d = l_air / 127.0;
            if let Some(gl) = self.gl {
                let g = ((self.now - gl.t0) / gl.length).min(1.0);
                let g = 0.5 - 0.5 * (PI * g).cos();
                self.gl_g = g;
                self.warp = gl.a + (gl.b - gl.a) * g;
                self.lip_trim = self.trim_a + (self.trim_b - self.trim_a) * g;
                let (a, b) = (self.at_a, self.at_b.unwrap_or_default());
                let mix = |p: f64, q: f64| p + (q - p) * g;
                let mut m = Moving { top: mix(a.top, b.top), ..Moving::default() };
                for j in 0..3 {
                    m.lp[j] = mix(a.lp[j], b.lp[j]);
                    m.ce[j] = mix(a.ce[j], b.ce[j]);
                    m.st[j] = (mix(a.st[j].0, b.st[j].0), mix(a.st[j].1, b.st[j].1));
                }
                self.at_now = Some(m);
                if self.now - gl.t0 >= gl.length {
                    self.gl = None;
                }
            }
            let (lp, ce, top, st) = match &self.at_now {
                Some(m) => (m.lp, m.ce, m.top, m.st),
                None => {
                    let a = self.tables.at(self.place);
                    (a.lp, a.ce, a.top, self.settings)
                }
            };
            let x = d.max(0.2);
            let (l, c);
            if x <= 0.55 {
                let w = (x - 0.2) / 0.35;
                l = lp[0] + (lp[1] - lp[0]) * w;
                c = ce[0] + (ce[1] - ce[0]) * w;
            } else if x <= 0.9 {
                let w = (x - 0.55) / 0.35;
                l = lp[1] + (lp[2] - lp[1]) * w;
                c = ce[1] + (ce[2] - ce[1]) * w;
            } else {
                l = lp[2] + top.ln() * (x - 0.9) / 0.1;
                c = ce[2];
            }
            let w = ((x - 0.2) / 0.7).clamp(0.0, 1.0);
            let (dc, air);
            if w < 0.55 {
                let g = w / 0.55;
                dc = st[0].0 + (st[1].0 - st[0].0) * g;
                air = st[0].1 + (st[1].1 - st[0].1) * g;
            } else {
                let g = (w - 0.55) / 0.45;
                dc = st[1].0 + (st[2].0 - st[1].0) * g;
                air = st[1].1 + (st[2].1 - st[1].1) * g;
            }
            let mut p = l.exp() * air * (d / 0.2).min(1.0);
            self.level_to = self.level_db(x);
            let tables = self.tables.clone();
            let n = tables.note(self.mute, self.base);
            if n.open.is_some() {
                let want = if let Some(c) = self.plunger_cc {
                    c
                } else if self.virtual_player {
                    // the hand opens with the note, wider the harder it was struck: a key alone says doo-wah; never quite shut
                    (0.25 + 0.75 * (self.v / 127.0).powf(1.5)) * (t / 0.14).min(1.0)
                } else {
                    0.5
                };
                self.opened += (want - self.opened) * 0.08;
                self.h_x = -1.0;
            }
            if (x - self.h_x).abs() > 0.004 {
                self.h_x = if n.open.is_none() { x } else { -1.0 };
                leaving(n, x, self.opened, self.core.h_mut());
            }
            let mc = self.modes_c;
            let tune_c = self.mode_here - if w < 0.55 { mc[0] + (mc[1] - mc[0]) * w / 0.55 } else { mc[1] + (mc[2] - mc[1]) * (w - 0.55) / 0.45 };
            let target_lip = n.f * (c + dc - 0.1) * 2f64.powf(-self.lip_trim / 12.0);
            // the attack or the slur
            let mut env;
            let mut lip;
            if self.slur {
                let g = (t / 0.025).min(1.0);
                lip = self.lip_from + (target_lip - self.lip_from) * g;
                lip *= 2f64.powf(-(if self.down { SLACK_DOWN } else { SLACK_UP }) * (-t / SLACK_T).exp() / 1200.0);
                env = if t < DIP_T { 1.0 - DIP * (0.5 - 0.5 * (2.0 * PI * t / DIP_T).cos()) } else { 1.0 };
                burst1 = 1.0;
            } else {
                let rise = self.rise;
                let from = self.rise_from;
                env = if t < rise { self.first * (from + (1.0 - from) * (PI / 2.0 * t / rise).sin().powi(2)) } else { self.first + (1.0 - self.first) * (1.0 - (-(t - rise) / 0.08).exp()) };
                lip = target_lip * 2f64.powf(-self.scoop * (-t / 0.04).exp() / 1200.0);
                burst1 = 1.0 + self.burst_add * (-t / 0.03).exp();
            }
            if let Some(r) = self.releasing.as_mut() {
                *r += DT;
                env *= (1.0 - *r / self.rel_len).max(0.0);
            }
            // vibrato
            let mut semis = self.warp + 2.0 * self.pb - TUNE_FOLLOW * tune_c / 100.0 + self.tune[usize::from(self.key.min(127))];
            let (depth, rate_cc) = if self.vib_knob > 0.0 {
                (self.vib_knob, 64.0)
            } else if self.virtual_player {
                (self.shape.vibrato(t) / 127.0, self.shape.vibrato_rate(t))
            } else {
                (0.0, 64.0)
            };
            self.vib_phase += 2.0 * PI * (3.5 + 4.5 * rate_cc / 127.0) * DT;
            let wob = self.vib_phase.sin();
            lip *= 1.0 + 0.012 * depth * wob;
            semis += 0.35 * depth * wob;
            p *= 1.0 + 0.10 * depth * wob;
            if self.virtual_player {
                semis += self.shape.bend_cents(t) / 100.0;
            }
            // growl
            let gr = self.growl.max(self.at_growl);
            if gr > 0.0 {
                let b = self.streams.band[i];
                p *= (1.0 + GROWL * gr * b).max(0.3);
                if b > 0.3 {
                    burst1 += 4.0 * gr;
                }
            }
            // flutter
            let fl = self.flutter.max(self.at_flutter);
            if fl > 0.0 {
                self.fl_phase += 2.0 * PI * 25.0 * (1.0 + 0.1 * self.streams.jit[i]) * DT;
                let c1 = 0.5 + 0.5 * self.fl_phase.cos();
                let shut = c1 * c1;
                let f2 = (1.6 * fl).min(1.0);
                p *= 1.0 - FLUTTER_DEPTH * f2 * shut;
                burst1 += 6.0 * fl * shut;
                semis -= 0.1 * f2;
            }
            p *= env * (1.0 + self.streams.wp[i]);
            lip *= 1.0 + self.streams.wl[i];
            pm1 = p;
            lip1 = lip;
            ratio = 2f64.powf(semis / 12.0);
            self.tau += DT;
            if self.releasing.is_some_and(|r| r > self.rel_len + 0.6) {
                self.cur = None;
            }
        }
        // eased: the air's and the key's changes must not click
        let g0 = self.level_g;
        if self.tables.has_level {
            self.level_now += (self.level_to - self.level_now) * 0.1;
            self.level_g = 10f64.powf(self.level_now / 20.0);
        }
        if self.m_count == 0 {
            out.fill(0.0);
            return;
        }
        let n = self.m_count;
        self.bank.n = n;
        for j in 0..n {
            let (e, (si, co)) = ((self.st_re[j] * ratio).exp(), (self.st_im[j] * ratio).sin_cos());
            self.bank.er[j] = e * co;
            self.bank.ei[j] = e * si;
            self.bank.rr[j] = self.rt_re[j] * ratio;
            self.bank.ri[j] = self.rt_im[j] * ratio;
        }
        let ramp = Ramp { pm: (self.pm, pm1), lip: (self.prev_ratio.map_or(self.lip, |r| self.lip * r), lip1 * ratio), burst: (self.burst, burst1) };
        let tables = self.tables.clone();
        self.core.process(&ramp, self.kick, self.mu, &self.bank, &tables.lips, &self.params, &self.streams.noise, &tables.dec, out);
        self.kick = 0.0;
        self.pm = pm1;
        self.lip = lip1;
        self.burst = burst1;
        self.prev_ratio = Some(ratio);
        if self.tables.has_level {
            let dg = (self.level_g - g0) / CTRL as f64;
            for (k, x) in out.iter_mut().enumerate() {
                *x *= g0 + dg * k as f64;
            }
        }
    }

    /// The state not a number (the test of what follows it).
    pub fn poison(&mut self) {
        self.core.poison();
    }

    /// Nothing sounds, no key is down.
    pub fn clear(&mut self) {
        self.held.clear();
        self.cur = None;
        self.releasing = None;
        self.duck = 0;
        self.pending = None;
        self.pending_off = false;
        self.m_count = 0;
        self.level = None;
        self.pm = 0.0;
        self.lip = 200.0;
        self.burst = 1.0;
        self.prev_ratio = None;
        self.smooth_air = None;
        self.reset();
    }

    /// The model blew up: the state starts again, rather than the speakers.
    pub fn reset(&mut self) {
        self.core.reset();
        self.core.set_noise_pos(self.noise_at);
    }
}

/// What leaves the bell at this air: between the tables' filters, or all the brightening falls at one velocity, where a
/// note changes table; and the plunger's hand between shut and open.
fn leaving(n: &Note, x: f64, opened: f64, to: &mut [f32]) {
    let i = if x < H_X[1] { 0 } else { 1 };
    let g = ((x - H_X[i]) / (H_X[i + 1] - H_X[i])).clamp(0.0, 1.0) as f32;
    let (a, b) = (&n.h[i], &n.h[i + 1]);
    if let Some(open) = &n.open {
        let (oa, ob) = (&open[i], &open[i + 1]);
        let o = opened as f32;
        for (j, t) in to.iter_mut().enumerate() {
            let h = a[j] + (b[j] - a[j]) * g;
            let q = oa[j] + (ob[j] - oa[j]) * g;
            *t = h + (q - h) * o;
        }
    } else {
        for (t, (p, q)) in to.iter_mut().zip(a.iter().zip(b.iter())) {
            *t = *p + (*q - *p) * g;
        }
    }
}
