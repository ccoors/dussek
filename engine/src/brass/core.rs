//! The sound of one player: `rt.c` in Rust. The lips and the bore's modes at 4 × the base rate, the mouthpiece
//! pressure steepening in a short delay line, down to the base rate, the part of the impedance the modes leave out
//! (FIR from the flow), out through the bell (FIR). A note's change crossfades the filters over 10 ms.
//! Double precision inside; the filters are the export's 32-bit tables, against a 64-bit history.

use super::tables::Lips;
use std::sync::Arc;

pub const OS: usize = 4;
pub const MAXM: usize = 64;
const HMAX: usize = 4096;
const DMAX: usize = 256;
const SD: usize = 32;
const SD_D: usize = 24;
const FADE: i32 = 441;
pub const SR: f64 = 44100.0;

/// What the block's controls ramp over: from `.0` to `.1`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ramp {
    pub pm: (f64, f64),
    pub lip: (f64, f64),
    pub burst: (f64, f64),
}

/// The bore's modes per oversampled step: each its pole's step `E` and its residue `r` (`rc T / 2`).
#[derive(Clone)]
pub struct ModeBank {
    pub n: usize,
    pub er: [f64; MAXM],
    pub ei: [f64; MAXM],
    pub rr: [f64; MAXM],
    pub ri: [f64; MAXM],
}

impl Default for ModeBank {
    fn default() -> Self {
        ModeBank { n: 0, er: [0.0; MAXM], ei: [0.0; MAXM], rr: [0.0; MAXM], ri: [0.0; MAXM] }
    }
}

/// An instrument's constants in the loop.
#[derive(Clone, Copy, Debug)]
pub struct CoreParams {
    pub width: f64,
    pub y0: f64,
    pub soft: f64,
    pub rho: f64,
    pub nl: f64,
}

pub struct Core {
    xr: [f64; MAXM],
    xi: [f64; MAXM],
    ly: [f64; 8],
    lv: [f64; 8],
    uprev: f64,
    pprev: f64,
    noise_pos: usize,
    sd: [f64; SD],
    si: usize,
    dp: Box<[f64; 2 * DMAX]>,
    du: Box<[f64; 2 * DMAX]>,
    dpos: usize,
    ht: Box<[f64; 2 * HMAX]>,
    hu: Box<[f64; 2 * HMAX]>,
    hpos_t: usize,
    hpos_u: usize,
    h: Box<[f32]>,
    nh: usize,
    h_old: Box<[f32]>,
    nh_old: usize,
    r: Option<Arc<[f32]>>,
    r_old: Option<Arc<[f32]>>,
    fade: i32,
    /// the portable path even where the CPU has the wider one (the test that they agree)
    pub force_scalar: bool,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

/// The dot product of a 32-bit filter with a 64-bit history, in lanes the compiler vectorizes.
#[inline(always)]
fn dot<const FMA: bool>(h: &[f32], x: &[f64]) -> f64 {
    const L: usize = 4;
    let mut acc = [0.0f64; L];
    let mut hc = h.chunks_exact(L);
    let mut xc = x.chunks_exact(L);
    for (a, b) in (&mut hc).zip(&mut xc) {
        for l in 0..L {
            acc[l] = if FMA { f64::from(a[l]).mul_add(b[l], acc[l]) } else { acc[l] + f64::from(a[l]) * b[l] };
        }
    }
    let mut t = acc[0] + acc[1] + acc[2] + acc[3];
    for (a, b) in hc.remainder().iter().zip(xc.remainder()) {
        t += f64::from(*a) * *b;
    }
    t
}

impl Core {
    pub fn new() -> Core {
        Core {
            xr: [0.0; MAXM],
            xi: [0.0; MAXM],
            ly: [0.0; 8],
            lv: [0.0; 8],
            uprev: 0.0,
            pprev: 0.0,
            noise_pos: 0,
            sd: [0.0; SD],
            si: 0,
            dp: Box::new([0.0; 2 * DMAX]),
            du: Box::new([0.0; 2 * DMAX]),
            dpos: 0,
            ht: Box::new([0.0; 2 * HMAX]),
            hu: Box::new([0.0; 2 * HMAX]),
            hpos_t: 0,
            hpos_u: 0,
            h: vec![0.0; HMAX].into_boxed_slice(),
            nh: 0,
            h_old: vec![0.0; HMAX].into_boxed_slice(),
            nh_old: 0,
            r: None,
            r_old: None,
            fade: 0,
            force_scalar: false,
        }
    }

    /// New filters, the old ones fading out over 10 ms (none before: at once). Fills the bell's in with `fill`.
    pub fn change_filters(&mut self, nh: usize, r: Arc<[f32]>, fill: impl FnOnce(&mut [f32])) {
        std::mem::swap(&mut self.h, &mut self.h_old);
        self.nh_old = self.nh;
        self.fade = if self.r.is_some() { FADE } else { 0 };
        self.r_old = self.r.replace(r);
        self.nh = nh;
        fill(&mut self.h[..nh]);
    }

    /// `change_filters` from a slice.
    pub fn set_filters(&mut self, h: &[f32], r: Arc<[f32]>) {
        self.change_filters(h.len(), r, |to| to.copy_from_slice(h));
    }

    /// Where in the noise it reads.
    pub fn set_noise_pos(&mut self, at: usize) {
        self.noise_pos = at;
    }

    /// The bell's filter as it stands: the player's blend moves it in place (no crossfade: its steps are small).
    pub fn h_mut(&mut self) -> &mut [f32] {
        &mut self.h[..self.nh]
    }

    /// The state from nothing, the filters kept and no fade (a voice that went non-finite).
    pub fn reset(&mut self) {
        self.xr = [0.0; MAXM];
        self.xi = [0.0; MAXM];
        self.ly = [0.0; 8];
        self.lv = [0.0; 8];
        self.uprev = 0.0;
        self.pprev = 0.0;
        self.noise_pos = 0;
        self.sd = [0.0; SD];
        self.si = 0;
        self.dp.fill(0.0);
        self.du.fill(0.0);
        self.dpos = 0;
        self.ht.fill(0.0);
        self.hu.fill(0.0);
        self.hpos_t = 0;
        self.hpos_u = 0;
        self.fade = 0;
    }

    /// The state not a number (the test of what follows it).
    pub fn poison(&mut self) {
        self.xr[0] = f64::NAN;
        self.uprev = f64::NAN;
    }

    /// `out.len()` base-rate samples.
    #[allow(clippy::too_many_arguments)]
    pub fn process(&mut self, ctl: &Ramp, kick: f64, mu: f64, modes: &ModeBank, lips: &Lips, p: &CoreParams, noise: &[f32], dec: &[f64], out: &mut [f64]) {
        #[cfg(target_arch = "x86_64")]
        if !self.force_scalar && std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma") {
            // SAFETY: the CPU has them, just checked
            unsafe { self.process_avx2(ctl, kick, mu, modes, lips, p, noise, dec, out) };
            return;
        }
        self.body::<false>(ctl, kick, mu, modes, lips, p, noise, dec, out);
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    #[allow(clippy::too_many_arguments)]
    unsafe fn process_avx2(&mut self, ctl: &Ramp, kick: f64, mu: f64, modes: &ModeBank, lips: &Lips, p: &CoreParams, noise: &[f32], dec: &[f64], out: &mut [f64]) {
        self.body::<true>(ctl, kick, mu, modes, lips, p, noise, dec, out);
    }

    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn body<const FMA: bool>(&mut self, ctl: &Ramp, kick: f64, mu: f64, modes: &ModeBank, lips: &Lips, p: &CoreParams, noise: &[f32], dec: &[f64], out: &mut [f64]) {
        let t_step = 1.0 / (SR * OS as f64);
        let m = modes.n;
        let (er, ei, rr, ri) = (&modes.er[..m], &modes.ei[..m], &modes.rr[..m], &modes.ri[..m]);
        let g_sum: f64 = rr.iter().map(|x| 2.0 * x).sum();
        let total = out.len() * OS;
        let ndec = dec.len();
        let nlip = lips.ratio.len();
        let mut xhr = [0.0; MAXM];
        let mut xhi = [0.0; MAXM];
        for (i, o_sample) in out.iter_mut().enumerate() {
            for o in 0..OS {
                let k = i * OS + o;
                let w = k as f64 / total as f64;
                let pm = ctl.pm.0 + (ctl.pm.1 - ctl.pm.0) * w;
                let lf = ctl.lip.0 + (ctl.lip.1 - ctl.lip.0) * w;
                let bu = ctl.burst.0 + (ctl.burst.1 - ctl.burst.0) * w;
                let dp = pm - self.pprev;
                let mut y = p.y0;
                for l in 0..nlip {
                    let wl = 2.0 * std::f64::consts::PI * lf * lips.ratio[l];
                    self.lv[l] += t_step * (lips.sign[l] * lips.comp[l] * dp / mu - wl / lips.q[l] * self.lv[l] - wl * wl * self.ly[l]);
                    if l == 0 && kick != 0.0 {
                        self.lv[l] += kick / total as f64; // over the block: at once it clicks
                    }
                    self.ly[l] += t_step * self.lv[l];
                    y += self.ly[l];
                }
                let s = p.width * 0.5 * (y + (y * y + p.soft * p.soft).sqrt());
                let up = self.uprev;
                for j in 0..m {
                    // E x + r u E
                    let (ax, ay) = (self.xr[j] + rr[j] * up, self.xi[j] + ri[j] * up);
                    xhr[j] = er[j] * ax - ei[j] * ay;
                    xhi[j] = er[j] * ay + ei[j] * ax;
                }
                let mut ph = 0.0;
                for x in &xhr[..m] {
                    ph += 2.0 * *x;
                }
                let pp = pm - ph;
                let mut uk = 0.0;
                if s > 0.0 {
                    let b = 2.0 * s * s * g_sum / p.rho;
                    let c = 2.0 * s * s * pp.abs() / p.rho;
                    uk = (-b / 2.0 + (b * b / 4.0 + c).sqrt()).copysign(pp);
                }
                uk += f64::from(noise[self.noise_pos]) * bu * uk.abs();
                self.noise_pos += 1;
                if self.noise_pos >= noise.len() {
                    self.noise_pos = 0;
                }
                for j in 0..m {
                    self.xr[j] = xhr[j] + rr[j] * uk;
                    self.xi[j] = xhi[j] + ri[j] * uk;
                }
                let pk = ph + g_sum * uk;
                self.uprev = uk;
                self.pprev = pk;
                // steepening: a sample arrives earlier the higher its pressure
                self.sd[self.si] = pk;
                let ri0 = (self.si + SD - SD_D) % SD;
                let adv = (p.nl * self.sd[ri0]).min((SD_D - 2) as f64).max(-((SD - SD_D - 2) as f64));
                let pos = (self.si + SD - SD_D) as f64 + adv;
                let a = pos.floor();
                let fr = pos - a;
                let a = a as usize;
                let ys = self.sd[a % SD] * (1.0 - fr) + self.sd[(a + 1) % SD] * fr;
                self.si = (self.si + 1) % SD;
                // the anti-alias filter's history, for the base rate
                self.dpos = if self.dpos == 0 { ndec - 1 } else { self.dpos - 1 };
                self.dp[self.dpos] = ys;
                self.dp[self.dpos + ndec] = ys;
                self.du[self.dpos] = uk;
                self.du[self.dpos + ndec] = uk;
            }
            let mut pb = 0.0;
            let mut ub = 0.0;
            for j in 0..ndec {
                pb += dec[j] * self.dp[self.dpos + j];
                ub += dec[j] * self.du[self.dpos + j];
            }
            self.hpos_u = if self.hpos_u == 0 { HMAX - 1 } else { self.hpos_u - 1 };
            self.hu[self.hpos_u] = ub;
            self.hu[self.hpos_u + HMAX] = ub;
            let hist_u = &self.hu[self.hpos_u..];
            let mut a = self.r.as_ref().map_or(0.0, |r| dot::<FMA>(r, hist_u));
            if self.fade > 0 {
                // the part the modes leave out crossfades with the bell's filter
                let g = f64::from(self.fade) / f64::from(FADE);
                let old = self.r_old.as_ref().map_or(0.0, |r| dot::<FMA>(r, hist_u));
                a = a * (1.0 - g) + old * g;
            }
            a += pb;
            self.hpos_t = if self.hpos_t == 0 { HMAX - 1 } else { self.hpos_t - 1 };
            self.ht[self.hpos_t] = a;
            self.ht[self.hpos_t + HMAX] = a;
            let hist_t = &self.ht[self.hpos_t..];
            let mut y = dot::<FMA>(&self.h[..self.nh], hist_t);
            if self.fade > 0 {
                let yo = dot::<FMA>(&self.h_old[..self.nh_old], hist_t);
                let g = f64::from(self.fade) / f64::from(FADE);
                y = y * (1.0 - g) + yo * g;
                self.fade -= 1;
            }
            *o_sample = y;
        }
    }
}
