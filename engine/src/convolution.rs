//! Non-uniformly partitioned FFT convolution (overlap-save), one input to several output
//! channels, float32. Latency: one base block (`base`).
//!
//! Stage 0 runs blocks of `base` samples over the head of the impulse response; a
//! further stage with block B covers the response from offset B − base on, which hides
//! its own block latency. A stage's output for a
//! completed input block lands in an output ring, at the samples the engine is about to
//! emit or later. The products with older input spectra (all partitions but the first)
//! are computed ahead, spread over the base blocks of the preceding period, so the work
//! per base block stays even.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

struct Stage {
    block: usize,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    /// IR partitions per channel: [channel][partition][bin], scaled by 1/fft size
    parts: Vec<Vec<Vec<Complex32>>>,
    /// spectra of past input blocks (frequency-domain delay line); `fdl[head]` newest
    fdl: Vec<Vec<Complex32>>,
    /// per spectrum: its block was silent (all zeros, the spectrum zero; its products skipped), and
    /// how many are
    silent: Vec<bool>,
    silent_count: usize,
    head: usize,
    /// time-domain input: previous block, then the block being collected
    time: Vec<f32>,
    fill: usize,
    /// products with partitions 1.. per channel, accumulated ahead of the block's end
    pre: Vec<Vec<Complex32>>,
    /// partitions 1..done already in `pre`
    done: usize,
    spec: Vec<Complex32>,
    out: Vec<f32>,
    scratch_fwd: Vec<Complex32>,
    scratch_inv: Vec<Complex32>,
}

impl Stage {
    fn new(irs: &[Vec<f32>], block: usize, from: usize, to: usize, planner: &mut RealFftPlanner<f32>) -> Self {
        let n = 2 * block;
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let bins = block + 1;
        let p = (to - from).div_ceil(block);
        let mut scratch_fwd = fwd.make_scratch_vec();
        let zero = Complex32::new(0.0, 0.0);
        // per channel only up to its own length (a shorter response has fewer partitions)
        let parts = irs
            .iter()
            .map(|ir| {
                (0..p.min(ir.len().saturating_sub(from).div_ceil(block)))
                    .map(|i| {
                        let mut t = vec![0.0; n];
                        let a = (from + i * block).min(ir.len());
                        let b = (from + (i + 1) * block).min(to).min(ir.len());
                        t[..b - a].copy_from_slice(&ir[a..b]);
                        let mut s = vec![zero; bins];
                        fwd.process_with_scratch(&mut t, &mut s, &mut scratch_fwd).unwrap();
                        s.iter_mut().for_each(|c| *c /= n as f32);
                        s
                    })
                    .collect()
            })
            .collect();
        Stage {
            block,
            scratch_inv: inv.make_scratch_vec(),
            fwd,
            inv,
            parts,
            fdl: vec![vec![zero; bins]; p],
            silent: vec![true; p],
            silent_count: p,
            head: 0,
            time: vec![0.0; n],
            fill: 0,
            pre: vec![vec![zero; bins]; irs.len()],
            done: 1,
            spec: vec![zero; bins],
            out: vec![0.0; n],
            scratch_fwd,
        }
    }

    fn partitions(&self) -> usize {
        self.fdl.len()
    }

    /// Accumulate the products with partitions done..upto into `pre` (input spectra of the
    /// previous blocks: partition i pairs with the block i − 1 behind the newest stored).
    fn ahead(&mut self, upto: usize) {
        let p = self.partitions();
        if self.silent_count == p {
            self.done = self.done.max(upto.min(p)); // nothing but silence stored
            return;
        }
        for (pre, parts) in self.pre.iter_mut().zip(&self.parts) {
            for (i, h) in parts.iter().enumerate().take(upto.min(p)).skip(self.done) {
                let at = (self.head + i - 1) % p;
                if self.silent[at] {
                    continue;
                }
                crate::kernel::cmac(pre, &self.fdl[at], h);
            }
        }
        self.done = self.done.max(upto.min(p));
    }

    /// Feed one base block; when the stage's block completes, add its output (block
    /// samples) to the ring at `ring_pos` (wrapping).
    fn push(&mut self, input: &[f32], base: usize, ring: &mut [Vec<f32>], ring_pos: usize) {
        let b = self.block;
        self.time[b + self.fill..b + self.fill + input.len()].copy_from_slice(input);
        self.fill += input.len();
        // spread the products with older blocks over the base blocks of this period; behind (a
        // history just taken over) at least an even share of what is left before the block ends
        let ticks = b / base;
        let tick = self.fill / base;
        let p = self.partitions();
        let even = 1 + (p - 1) * tick / ticks;
        self.ahead(even.min(self.done + (p - self.done).div_ceil(ticks - tick + 1)));
        if self.fill < b {
            return;
        }
        self.fill = 0;
        self.ahead(p);
        // the transform's input: the previous block and this one; silent (exact zeros, a bridge zone no
        // string feeds): no transform, its products skipped
        let quiet = self.time.iter().all(|&x| x == 0.0);
        self.out.copy_from_slice(&self.time);
        self.time.copy_within(b.., 0);
        self.head = (self.head + p - 1) % p;
        self.silent_count -= self.silent[self.head] as usize;
        self.silent[self.head] = quiet;
        self.silent_count += quiet as usize;
        if quiet {
            self.spec.fill(Complex32::new(0.0, 0.0));
        } else {
            self.fwd.process_with_scratch(&mut self.out, &mut self.spec, &mut self.scratch_fwd).unwrap();
        }
        self.fdl[self.head].copy_from_slice(&self.spec);
        // every stored spectrum silent: the products are zero, nothing to add
        let all_quiet = self.silent_count == p;
        let len = ring[0].len();
        for (ch, pre) in self.pre.iter_mut().enumerate() {
            let Some(h0) = self.parts[ch].first() else { continue };
            if all_quiet {
                pre.fill(Complex32::new(0.0, 0.0));
                continue;
            }
            if !quiet {
                crate::kernel::cmac(pre, &self.spec, h0);
            }
            self.inv.process_with_scratch(pre, &mut self.out, &mut self.scratch_inv).unwrap();
            // into the ring at ring_pos, wrapping once at most (b ≤ half the ring)
            let first = b.min(len - ring_pos);
            for (r, &y) in ring[ch][ring_pos..ring_pos + first].iter_mut().zip(&self.out[b..b + first]) {
                *r += y;
            }
            for (r, &y) in ring[ch][..b - first].iter_mut().zip(&self.out[b + first..]) {
                *r += y;
            }
            pre.fill(Complex32::new(0.0, 0.0));
        }
        self.done = 1;
    }
}

pub struct Convolver {
    base: usize,
    stages: Vec<Stage>,
    /// output ring per channel; `pos` is the start of the next base block to emit
    ring: Vec<Vec<f32>>,
    pos: usize,
    /// response length (samples)
    len: usize,
}

impl Convolver {
    /// Stage blocks: base, then each of `larger` (powers of two, increasing), each used
    /// only if the response reaches its offset.
    pub fn new(irs: &[Vec<f32>], base: usize, larger: &[usize]) -> Self {
        let len = irs.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let mut planner = RealFftPlanner::<f32>::new();
        let mut blocks = vec![base];
        blocks.extend(larger.iter().copied().filter(|&b| b - base < len));
        let mut stages = Vec::new();
        for (i, &b) in blocks.iter().enumerate() {
            let from = b - base;
            let to = blocks.get(i + 1).map_or(len, |&nb| nb - base).min(len);
            stages.push(Stage::new(irs, b, if i == 0 { 0 } else { from }, to, &mut planner));
        }
        let ring_len = (2 * blocks.last().copied().unwrap_or(base)).next_power_of_two();
        debug_assert!(ring_len % base == 0, "the output ring holds whole base blocks");
        Convolver { base, stages, ring: vec![vec![0.0; ring_len]; irs.len()], pos: 0, len }
    }

    /// Samples the output rings after the last input (response length plus the latest stage).
    /// Silence: every stored input and pending output to zero (the stages' block timing kept,
    /// allocation-free).
    pub fn clear(&mut self) {
        let zero = Complex32::new(0.0, 0.0);
        for s in &mut self.stages {
            s.fdl.iter_mut().for_each(|v| v.fill(zero));
            s.silent.fill(true);
            s.silent_count = s.silent.len();
            s.pre.iter_mut().for_each(|v| v.fill(zero));
            s.time.fill(0.0);
            s.out.fill(0.0);
            s.spec.fill(zero);
        }
        self.ring.iter_mut().for_each(|r| r.fill(0.0));
    }

    pub fn tail(&self) -> usize {
        self.len + self.ring[0].len()
    }

    /// Whether `old` is partitioned alike (the base, the stages' blocks, the channels and their
    /// output rings): its input history can go over (`take_history`).
    pub fn same_layout(&self, old: &Convolver) -> bool {
        self.base == old.base
            && self.ring.len() == old.ring.len()
            && self.ring[0].len() == old.ring[0].len()
            && self.stages.len() == old.stages.len()
            && self.stages.iter().zip(&old.stages).all(|(a, b)| a.block == b.block)
    }

    /// Take over the input history of `old` (`same_layout`, fed the same input so far): the input
    /// spectra stored (as many as this response reaches back), the block being collected, the
    /// output already on its way (the old response's); the products ahead with this response
    /// follow over the rest of each stage's period. From here on its output is this response's to
    /// all the input. Allocation-free: copies only.
    pub fn take_history(&mut self, old: &Convolver) {
        debug_assert!(self.same_layout(old));
        let zero = Complex32::new(0.0, 0.0);
        for (s, o) in self.stages.iter_mut().zip(&old.stages) {
            let (pn, po) = (s.partitions(), o.partitions());
            for k in 0..pn {
                // the newest first, at each one's own head
                let at = (s.head + k) % pn;
                if k < po {
                    s.fdl[at].copy_from_slice(&o.fdl[(o.head + k) % po]);
                    s.silent[at] = o.silent[(o.head + k) % po];
                } else {
                    s.fdl[at].fill(zero);
                    s.silent[at] = true;
                }
            }
            s.silent_count = s.silent.iter().filter(|&&q| q).count();
            s.time.copy_from_slice(&o.time);
            s.fill = o.fill;
            // the products ahead with this response: caught up over the rest of the period (`push`)
            s.pre.iter_mut().for_each(|v| v.fill(zero));
            s.done = 1;
        }
        for (r, ro) in self.ring.iter_mut().zip(&old.ring) {
            r.copy_from_slice(ro);
        }
        self.pos = old.pos;
    }

    /// Push one base block of input; add the output block (the convolution of everything
    /// up to and including this input) to `out[channel]`.
    pub fn process(&mut self, input: &[f32], out: &mut [Vec<f32>]) {
        debug_assert_eq!(input.len(), self.base);
        for st in &mut self.stages {
            st.push(input, self.base, &mut self.ring, self.pos);
        }
        let len = self.ring[0].len();
        // pos steps by whole base blocks through a ring a power of two of them long: no wrap inside
        for (o, r) in out.iter_mut().zip(&mut self.ring) {
            let seg = &mut r[self.pos..self.pos + self.base];
            for (o, r) in o.iter_mut().zip(seg.iter_mut()) {
                *o += *r;
                *r = 0.0;
            }
        }
        self.pos = (self.pos + self.base) % len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, seed: u64) -> Vec<f32> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x % 2001) as f32 / 1000.0 - 1.0
            })
            .collect()
    }

    /// direct convolution's sample t
    fn direct(ir: &[f32], x: &[f32], t: usize) -> f32 {
        (0..ir.len().min(t + 1)).map(|k| ir[k] * x[t - k]).sum()
    }

    #[test]
    fn a_taken_history_sounds_as_if_it_had_always_played() {
        let (base, stages) = (128, [1024]);
        let a: Vec<f32> = noise(3000, 1).iter().enumerate().map(|(i, v)| v * (-(i as f32) / 700.0).exp()).collect();
        let b: Vec<f32> = noise(2900, 2).iter().enumerate().map(|(i, v)| v * (-(i as f32) / 500.0).exp()).collect();
        let x = noise(base * 60, 3);
        let mut ca = Convolver::new(std::slice::from_ref(&a), base, &stages);
        let mut cb = Convolver::new(std::slice::from_ref(&b), base, &stages);
        assert!(cb.same_layout(&ca));
        let handover = 37; // blocks, mid-period of the 1024 stage
        let mut out = vec![vec![0.0f32; base]];
        for k in 0..60 {
            if k == handover {
                cb.take_history(&ca);
            }
            out[0].fill(0.0);
            let c = if k < handover { &mut ca } else { &mut cb };
            c.process(&x[k * base..(k + 1) * base], &mut out);
            // past the output pending in the 1024 stage at the handover (the old response's): B's
            if k > handover + 1024 / base {
                for (j, y) in out[0].iter().enumerate() {
                    let t = k * base + j;
                    let want = direct(&b, &x, t);
                    assert!((y - want).abs() < 2e-3 * want.abs().max(1.0), "block {k}, sample {j}: {y} vs {want}");
                }
            }
        }
    }

    #[test]
    fn silence_skipped_sounds_the_same() {
        // bursts between long silences (a bridge zone struck now and then): exactly the direct sum
        let (base, stages) = (128, [1024]);
        let a: Vec<f32> = noise(5000, 4).iter().enumerate().map(|(i, v)| v * (-(i as f32) / 900.0).exp()).collect();
        let mut x = vec![0.0f32; base * 120];
        for start in [300, 9000, 9100, 14000] {
            for (j, v) in noise(700, start as u64).iter().enumerate() {
                x[start + j] = *v;
            }
        }
        let mut c = Convolver::new(std::slice::from_ref(&a), base, &stages);
        let mut out = vec![vec![0.0f32; base]];
        for k in 0..120 {
            out[0].fill(0.0);
            c.process(&x[k * base..(k + 1) * base], &mut out);
            for (j, y) in out[0].iter().enumerate() {
                let want = direct(&a, &x, k * base + j);
                assert!((y - want).abs() < 2e-3 * want.abs().max(1.0), "block {k}, sample {j}: {y} vs {want}");
            }
        }
    }
}

