//! Sympathetic resonance (`proto/resonance.py`): one complex one-pole per partial of
//! every key (unisons merged), `y[n] = b·x[n] + p·y[n−1]` with b = −2·TRANSFER·κ/sr and
//! x the summed vertical bridge force; driven only while the key's strings are free and
//! not sounding by their own voice, otherwise just decaying (with the damper's extra loss
//! when it rests, and the buff's when its pads are on: `set_buff`, at once on every
//! resonator of a buffed string).
//!
//! All resonators of all keys form one float32 bank; the ones that are driven or still
//! ringing are kept densely at positions 0..n (`id`/`pos` map both ways), an undriven
//! one has b = 0 and is dropped once it has decayed.
//!
//! A key's drive fades in and out over DRIVE_RAMP_S (its dampers landing or lifting, its own
//! voice starting or dying away): switched in one sample, each resonator's forced response to
//! the other strings stopped or started at once and it rang on at its own frequency, over the
//! bank a broadband burst (half-pedalling: to 16 kHz, 35–50 dB over the bank's level before).

use realfft::num_complex::Complex64;

use crate::instrument::Instrument;
use crate::kernel::{self, LANES, Lane};

/// an undriven resonator is dropped below this amplitude (bridge-force units)
const SILENT: f32 = 1e-12;
/// a key's drive fades in or out over this long, linearly sample by sample (in steps it rang
/// at the steps' rate: 16 samples, 3 kHz and its harmonics)
const DRIVE_RAMP_S: f64 = 0.02;
const NONE: u32 = u32::MAX;
/// the blocks between two culls (`Bank::cull`)
const CULL_EVERY: u32 = 4;

/// Per-key condition of the bank, set by the engine.
#[derive(Clone, Copy, PartialEq)]
pub struct KeyState {
    pub driven: bool,
    /// damper engagement: 0 lifted … 1 resting
    pub engage: f64,
}

pub struct Bank {
    // per resonator id (keys in order, `first[key]..first[key + 1]`)
    first: Vec<usize>,
    b: Vec<(f32, f32)>,
    pole: [Vec<(f32, f32)>; 2],
    /// per id: (loss 1/s, damper loss 1/s, angular step rad/sample), for partial damping
    loss: Vec<(f64, f64, f64)>,
    /// per id: the buff's extra loss (1/s; 0 on strings without pads) and the poles with
    /// it (lifted, resting)
    buff_loss: Vec<f64>,
    buff_pole: [Vec<(f32, f32)>; 2],
    buff: bool,
    rate: f64,
    pos: Vec<u32>,
    state: Vec<KeyState>,
    /// user factor on the coupling b
    coupling: f32,
    /// the drive's fade: per position its share now (0 … 1, b × it), the ids fading (each
    /// once: `fading`), the share's step per sample; per position the drive's step this run
    /// and per lane chunk whether one of its resonators fades
    share: Vec<f32>,
    ramp: Vec<u32>,
    fading: Vec<bool>,
    ramp_step: f32,
    dbr: Vec<Lane>,
    dbi: Vec<Lane>,
    chunk_fades: Vec<bool>,
    /// calls of `cull` since it last looked
    cull_tick: u32,
    // per position
    n: usize,
    id: Vec<u32>,
    key_of: Vec<u16>,
    sr: Vec<Lane>,
    si: Vec<Lane>,
    pr: Vec<Lane>,
    pi: Vec<Lane>,
    br: Vec<Lane>,
    bi: Vec<Lane>,
}

impl Bank {
    pub fn new(inst: &Instrument, sr: f64) -> Self {
        let mut first = vec![0];
        let mut b = Vec::new();
        let mut pole = [Vec::new(), Vec::new()];
        let mut loss = Vec::new();
        let mut buff_loss = Vec::new();
        let mut buff_pole = [Vec::new(), Vec::new()];
        let mut key_of = Vec::new();
        for (key, n) in inst.notes.iter().enumerate() {
            let s = &n.sym;
            for i in 0..s.f.len() {
                if s.f[i] >= 0.45 * sr {
                    continue;
                }
                let bb = s.kappa[i] * (-2.0 * inst.transfer / sr);
                b.push((bb.re as f32, bb.im as f32));
                let pad = s.buff_sigma.get(i).copied().unwrap_or(0.0);
                for (d, extra) in [0.0, s.damper_sigma[i]].into_iter().enumerate() {
                    let p = Complex64::new(-(s.sigma[i] + extra) / sr, 2.0 * std::f64::consts::PI * s.f[i] / sr).exp();
                    pole[d].push((p.re as f32, p.im as f32));
                    let q = Complex64::new(-(s.sigma[i] + extra + pad) / sr, 2.0 * std::f64::consts::PI * s.f[i] / sr).exp();
                    buff_pole[d].push((q.re as f32, q.im as f32));
                }
                buff_loss.push(pad);
                loss.push((s.sigma[i], s.damper_sigma[i], 2.0 * std::f64::consts::PI * s.f[i] / sr));
                key_of.push(key as u16);
            }
            first.push(b.len());
        }
        let total = b.len();
        let chunks = kernel::padded(total) / LANES;
        let z = || vec![[0.0f32; LANES]; chunks];
        Bank {
            first,
            b,
            pole,
            loss,
            buff_loss,
            buff_pole,
            buff: false,
            rate: sr,
            pos: vec![NONE; total],
            state: vec![KeyState { driven: false, engage: 1.0 }; inst.notes.len()],
            coupling: 1.0,
            share: vec![0.0; chunks * LANES],
            ramp: Vec::with_capacity(total),
            fading: vec![false; total],
            ramp_step: (1.0 / (DRIVE_RAMP_S * sr)).min(1.0) as f32,
            dbr: z(),
            dbi: z(),
            chunk_fades: vec![false; chunks],
            cull_tick: 0,
            n: 0,
            id: vec![NONE; total],
            key_of,
            sr: z(),
            si: z(),
            pr: z(),
            pi: z(),
            br: z(),
            bi: z(),
        }
    }

    fn put(&mut self, p: usize, id: usize) {
        let st = self.state[self.key_of[id] as usize];
        let pad = if self.buff { self.buff_loss[id] } else { 0.0 };
        let (pr, pi) = if st.engage == 0.0 || st.engage == 1.0 {
            let poles = if pad > 0.0 { &self.buff_pole } else { &self.pole };
            poles[(st.engage == 1.0) as usize][id]
        } else {
            let (sig, sig_d, w) = self.loss[id];
            let q = Complex64::new(-(sig + pad + st.engage * sig_d) / self.rate, w).exp();
            (q.re as f32, q.im as f32)
        };
        if self.ramp_step >= 1.0 {
            // no fade (DRIVE_RAMP_S within a sample): switched at once
            self.share[p] = if st.driven { 1.0 } else { 0.0 };
        }
        let g = self.share[p];
        let (br, bi) = (self.b[id].0 * self.coupling * g, self.b[id].1 * self.coupling * g);
        if g != if st.driven { 1.0 } else { 0.0 } {
            // on its way to full drive or to none
            if !self.fading[id] {
                self.fading[id] = true;
                self.ramp.push(id as u32);
            }
        }
        let (c, l) = (p / LANES, p % LANES);
        self.pr[c][l] = pr;
        self.pi[c][l] = pi;
        self.br[c][l] = br;
        self.bi[c][l] = bi;
    }

    /// Scale the coupling to the strings (drive of every resonator).
    pub fn set_coupling(&mut self, c: f64) {
        self.coupling = c as f32;
        for p in 0..self.n {
            self.put(p, self.id[p] as usize);
        }
    }

    /// Put the buff's pads on (or off) every buffed string's resonators, ringing ones at once.
    pub fn set_buff(&mut self, on: bool) {
        if on == self.buff {
            return;
        }
        self.buff = on;
        for p in 0..self.n {
            let id = self.id[p] as usize;
            if self.buff_loss[id] > 0.0 {
                self.put(p, id);
            }
        }
    }

    /// Change a key's condition (from the next sample on).
    pub fn set_key(&mut self, key: usize, st: KeyState) {
        if self.state[key] == st {
            return;
        }
        self.state[key] = st;
        for id in self.first[key]..self.first[key + 1] {
            match self.pos[id] {
                NONE if st.driven => {
                    let p = self.n;
                    self.n += 1;
                    self.id[p] = id as u32;
                    self.pos[id] = p as u32;
                    self.share[p] = 0.0;
                    self.put(p, id);
                }
                NONE => {}
                p => self.put(p as usize, id),
            }
        }
    }

    /// Take over the ringing resonators of `old` (the same keys compiled for another
    /// tuning): key conditions, coupling, and each state at its new pole (a key's
    /// partials beyond the new rate limit are dropped).
    pub fn adopt(&mut self, old: &Bank) {
        // start empty (a new bank already drives the keys without dampers)
        for &id in &self.ramp {
            self.fading[id as usize] = false;
        }
        self.ramp.clear();
        for p in 0..self.n {
            self.pos[self.id[p] as usize] = NONE;
            self.id[p] = NONE;
            let (c, l) = (p / LANES, p % LANES);
            for a in [&mut self.sr, &mut self.si, &mut self.pr, &mut self.pi, &mut self.br, &mut self.bi] {
                a[c][l] = 0.0;
            }
        }
        self.n = 0;
        self.state.copy_from_slice(&old.state);
        self.coupling = old.coupling;
        self.buff = old.buff;
        for p in 0..old.n {
            let id = old.id[p] as usize;
            let key = old.key_of[id] as usize;
            let to = self.first[key] + (id - old.first[key]);
            if to >= self.first[key + 1] {
                continue;
            }
            let q = self.n;
            self.n += 1;
            self.id[q] = to as u32;
            self.pos[to] = q as u32;
            self.share[q] = old.share[p];
            self.put(q, to);
            let (c, l, oc, ol) = (q / LANES, q % LANES, p / LANES, p % LANES);
            self.sr[c][l] = old.sr[oc][ol];
            self.si[c][l] = old.si[oc][ol];
        }
        // driven keys whose resonators had not been running
        for key in 0..self.state.len() {
            if self.state[key].driven {
                for id in self.first[key]..self.first[key + 1] {
                    if self.pos[id] == NONE {
                        let q = self.n;
                        self.n += 1;
                        self.id[q] = id as u32;
                        self.pos[id] = q as u32;
                        // at rest: no forced response to stop or start (the handover's crossfade)
                        self.share[q] = 1.0;
                        self.put(q, id);
                    }
                }
            }
        }
    }

    /// Run acc.len() samples driven by x; the bank's bridge force goes into the lanes.
    pub fn run(&mut self, x: &[f32], acc: &mut [Lane]) {
        if self.n == 0 || x.is_empty() {
            return;
        }
        let c = self.n.div_ceil(LANES);
        if self.ramp.is_empty() {
            kernel::driven(&mut self.sr[..c], &mut self.si[..c], &self.pr[..c], &self.pi[..c], &self.br[..c], &self.bi[..c], x, acc);
            return;
        }
        // drives fading: their resonators' chunks with the drive moving sample by sample
        let len = x.len() as f32;
        self.chunk_fades[..c].fill(false);
        for &id in &self.ramp {
            let p = self.pos[id as usize];
            if p == NONE {
                continue;
            }
            let (p, id) = (p as usize, id as usize);
            let target = if self.state[self.key_of[id] as usize].driven { 1.0 } else { 0.0 };
            let g = self.share[p];
            let end = if target > g { (g + self.ramp_step * len).min(target) } else { (g - self.ramp_step * len).max(target) };
            let (ch, l) = (p / LANES, p % LANES);
            self.dbr[ch][l] = self.b[id].0 * self.coupling * (end - g) / len;
            self.dbi[ch][l] = self.b[id].1 * self.coupling * (end - g) / len;
            self.chunk_fades[ch] = true;
        }
        let mut a = 0;
        while a < c {
            let fades = self.chunk_fades[a];
            let mut b = a + 1;
            while b < c && self.chunk_fades[b] == fades {
                b += 1;
            }
            if fades {
                kernel::driven_ramp(&mut self.sr[a..b], &mut self.si[a..b], &self.pr[a..b], &self.pi[a..b], &mut self.br[a..b], &mut self.bi[a..b], &self.dbr[a..b], &self.dbi[a..b], x, acc);
            } else {
                kernel::driven(&mut self.sr[a..b], &mut self.si[a..b], &self.pr[a..b], &self.pi[a..b], &self.br[a..b], &self.bi[a..b], x, acc);
            }
            a = b;
        }
        self.advance_ramp(len);
    }

    /// The fading drives `n` samples on (as `run` moved them): their shares and exact drives
    /// set, the steps cleared, the finished ones dropped from the list.
    fn advance_ramp(&mut self, n: f32) {
        let mut k = 0;
        while k < self.ramp.len() {
            let id = self.ramp[k] as usize;
            let p = self.pos[id];
            let target = if self.state[self.key_of[id] as usize].driven { 1.0 } else { 0.0 };
            let done = p == NONE || {
                let p = p as usize;
                let g = &mut self.share[p];
                *g = if target > *g { (*g + self.ramp_step * n).min(target) } else { (*g - self.ramp_step * n).max(target) };
                let g = *g;
                let (c, l) = (p / LANES, p % LANES);
                self.br[c][l] = self.b[id].0 * self.coupling * g;
                self.bi[c][l] = self.b[id].1 * self.coupling * g;
                self.dbr[c][l] = 0.0;
                self.dbi[c][l] = 0.0;
                g == target
            };
            if done {
                self.fading[id] = false;
                self.ramp.swap_remove(k);
            } else {
                k += 1;
            }
        }
    }

    /// Drop undriven resonators that have decayed.
    /// Drop the undriven resonators that have decayed, every CULL_EVERY calls (once per engine block:
    /// a silent one runs a few ms longer, the pass costs a quarter).
    pub fn cull(&mut self) {
        self.cull_tick += 1;
        if self.cull_tick < CULL_EVERY {
            return;
        }
        self.cull_tick = 0;
        let mut p = 0;
        while p < self.n {
            let (c, l) = (p / LANES, p % LANES);
            // (one fading in has no drive yet, nor a state)
            let undriven = self.br[c][l] == 0.0 && self.bi[c][l] == 0.0 && !self.fading[self.id[p] as usize];
            if !undriven || self.sr[c][l].abs().max(self.si[c][l].abs()) >= SILENT {
                p += 1;
                continue;
            }
            let last = self.n - 1;
            self.pos[self.id[p] as usize] = NONE;
            let (lc, ll) = (last / LANES, last % LANES);
            if p != last {
                self.id[p] = self.id[last];
                self.pos[self.id[p] as usize] = p as u32;
                for a in [&mut self.sr, &mut self.si, &mut self.pr, &mut self.pi, &mut self.br, &mut self.bi] {
                    a[c][l] = a[lc][ll];
                }
                self.share[p] = self.share[last];
            }
            self.share[last] = 0.0;
            for a in [&mut self.sr, &mut self.si, &mut self.pr, &mut self.pi, &mut self.br, &mut self.bi] {
                a[lc][ll] = 0.0;
            }
            self.id[last] = NONE;
            self.n = last;
        }
    }

    /// Silence: every resonator's state to zero (the driven ones stay, at rest).
    pub fn clear(&mut self) {
        self.cull_tick = 0;
        let c = self.n.div_ceil(LANES);
        for a in [&mut self.sr, &mut self.si] {
            a[..c].iter_mut().for_each(|l| *l = [0.0; LANES]);
        }
    }

    /// Key `key`'s resonators' amplitudes per partial (bridge-force units), into `out` (the rest
    /// zero): the visualization's ring.
    pub fn amplitudes(&self, key: usize, out: &mut [f32]) {
        out.fill(0.0);
        for id in self.first[key]..self.first[key + 1] {
            let (i, p) = (id - self.first[key], self.pos[id]);
            if i < out.len() && p != NONE {
                let (c, l) = (p as usize / LANES, p as usize % LANES);
                out[i] = self.sr[c][l].hypot(self.si[c][l]);
            }
        }
    }

    pub fn running(&self) -> usize {
        self.n
    }
}
