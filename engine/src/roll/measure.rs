//! A candidate measured as the references are: each of the correction's 15 notes struck alone and
//! held, its spectrum in octave bands early and late, its level; brightness and decay per register;
//! the responses' ring; Player against Recording. `Realism` holds the references' range; `faults`
//! names what only a roll can get wrong (the bass out of phase, the microphones' colors apart).
use super::bands;
use super::files::Export;
use crate::instrument::Error;
use crate::mic::MicPosition;
use crate::{Engine, Event, Instrument, RadiationCache, Tuning};
use realfft::RealFftPlanner;

pub const NOTES: [u8; 15] = [21, 27, 33, 39, 45, 51, 57, 63, 69, 75, 81, 87, 93, 99, 105];
pub const STAT_NAMES: [&str; 7] = ["brightness bass", "brightness middle", "brightness treble", "decay bass", "decay middle", "decay treble", "ring at 60 ms"];
const SR: f64 = 48000.0;
/// octave bands from 62.5 Hz (the lower edges)
const OCTAVES: usize = 9;
/// the references' range widened by this share of its span on each side: the user approved a roll
/// 3 dB brighter in the treble than the brightest reference (an eighth of the span) and rejected
/// rolls a sixth and more beyond it (2026-10-17)
const MARGIN: f64 = 0.25;

fn band_lo(i: usize) -> f64 {
    62.5 * 2f64.powi(i as i32)
}

pub struct Notes {
    /// per note: power per octave band (dB) of the mono sum (L+R)/2, 0.05–0.5 s
    pub head: Vec<[f64; OCTAVES]>,
    /// the same, 1.5–2.9 s
    pub late: Vec<[f64; OCTAVES]>,
    /// per note: the level over 0–0.5 s (dB)
    pub level: Vec<f64>,
    /// per octave: |L+R|² against 2(|L|²+|R|²) over the notes' 0.05–0.5 s (dB: 0 in phase, −3
    /// unrelated, far below cancelling)
    pub mono_vs_power: [f64; OCTAVES],
}

fn spectrum(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(n);
    let mut buf: Vec<f64> = x.iter().enumerate().map(|(i, v)| v * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut buf, &mut out).expect("fft");
    out.iter().map(|c| c.norm_sqr()).collect()
}

fn octaves(p: &[f64], n: usize) -> [f64; OCTAVES] {
    std::array::from_fn(|b| {
        let (lo, hi) = (band_lo(b), 2.0 * band_lo(b));
        let s: f64 = p.iter().enumerate().filter(|(k, _)| {
            let f = *k as f64 * SR / n as f64;
            f >= lo && f < hi
        }).map(|(_, v)| v).sum();
        s
    })
}

fn db(x: f64) -> f64 {
    10.0 * (x + 1e-30).log10()
}

/// Each of `NOTES` alone on its own engine (built through one radiation cache), struck at the
/// hammer speed of MIDI `velocity`, released at 3.0 s, 3.5 s rendered; the notes in parallel.
pub fn render_notes(inst: &Instrument, velocity: f64, mic: MicPosition) -> Result<Notes, Error> {
    render(inst, velocity, mic, 3.5)
}

/// `render_notes` for the first 0.6 s only (`late` left empty): the levels and early spectra, six
/// times faster.
pub fn render_heads(inst: &Instrument, velocity: f64, mic: MicPosition) -> Result<Notes, Error> {
    render(inst, velocity, mic, 0.6)
}

fn render(inst: &Instrument, velocity: f64, mic: MicPosition, seconds: f64) -> Result<Notes, Error> {
    let tuning = Tuning { mic, ..Tuning::default() };
    // the first engine computes the radiation; the others are built in their notes' threads, each
    // from a copy of the filled cache (one after another, their convolvers took 0.6 s at Player)
    let mut cache = RadiationCache::default();
    let first = Engine::with_cache(inst.clone(), SR, tuning, &mut cache)?;
    let outs: Vec<(Vec<f64>, Vec<f64>)> = std::thread::scope(|s| {
        let mut first = Some(first);
        let handles: Vec<_> = NOTES
            .iter()
            .map(|&key| {
                let (mine, cache) = (first.take(), &cache);
                s.spawn(move || -> Result<_, Error> {
                    let mut e = match mine {
                        Some(e) => e,
                        None => Engine::with_cache(inst.clone(), SR, tuning, &mut cache.clone())?,
                    };
                    // as fitted, without a strike's own variation
                    e.set_params(crate::Params { variation: 0.0, ..crate::Params::default() });
                    let speed = e.hammer_speed(key, velocity);
                    e.queue(0, Event::NoteOn { key, speed });
                    e.queue((3.0 * SR) as u64, Event::KeyUp { key });
                    let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
                    let (mut ol, mut or) = (Vec::new(), Vec::new());
                    while ol.len() < (seconds * SR) as usize {
                        e.process(&mut l, &mut r);
                        ol.extend(l.iter().map(|&v| v as f64));
                        or.extend(r.iter().map(|&v| v as f64));
                    }
                    Ok((ol, or))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a note")).collect::<Result<_, _>>()
    })?;
    let (mut head, mut late, mut level) = (Vec::new(), Vec::new(), Vec::new());
    let (mut mono, mut power) = ([0.0; OCTAVES], [0.0; OCTAVES]);
    let seg = |v: &[f64], a: f64, b: f64| v[(a * SR) as usize..(b * SR) as usize].to_vec();
    for (l, r) in &outs {
        // the spectra from 0.05 s, past the hammer's attack (bright, and unlike from piano to piano:
        // the spike's measure, which passed the roll the user approved); the level from 0 s
        let (hl, hr) = (seg(l, 0.05, 0.5), seg(r, 0.05, 0.5));
        let n = hl.len();
        let (pl, pr) = (spectrum(&hl), spectrum(&hr));
        let sum: Vec<f64> = hl.iter().zip(&hr).map(|(a, b)| a + b).collect();
        let ps = spectrum(&sum);
        let both: Vec<f64> = pl.iter().zip(&pr).map(|(a, b)| a + b).collect();
        let (ob, os) = (octaves(&both, n), octaves(&ps, n));
        for b in 0..OCTAVES {
            mono[b] += os[b];
            power[b] += 2.0 * ob[b];
        }
        // the spectra of the mono sum ((L+R)/2: the spike's measure; a wide piano's treble, unlike
        // between the microphones, counts less than its matching bass, as in mono)
        head.push(os.map(|v| db(v / 4.0)));
        if seconds >= 2.9 {
            let (ll, lr) = (seg(l, 1.5, 2.9), seg(r, 1.5, 2.9));
            let m = ll.len();
            let lm: Vec<f64> = ll.iter().zip(&lr).map(|(a, b)| (a + b) / 2.0).collect();
            late.push(octaves(&spectrum(&lm), m).map(db));
        }
        let (fl, fr) = (seg(l, 0.0, 0.5), seg(r, 0.0, 0.5));
        level.push(db(fl.iter().chain(&fr).map(|v| v * v).sum::<f64>() / (2 * fl.len()) as f64));
    }
    Ok(Notes { head, late, level, mono_vs_power: std::array::from_fn(|b| db(mono[b]) - db(power[b])) })
}

/// The 7 statistics of `STAT_NAMES` at velocity 80, Recording.
pub fn stats(e: &Export) -> Result<Vec<f64>, Error> {
    Ok(measure(e)?.stats)
}

/// What one render at each position gives: the statistics, Player minus Recording per octave, each
/// note's level at Recording.
#[derive(Clone)]
pub struct Measured {
    pub stats: Vec<f64>,
    pub pr: [f64; OCTAVES],
    pub level: Vec<f64>,
    /// the Recording render's mono against power (`Notes::mono_vs_power`)
    pub mono_vs_power: [f64; OCTAVES],
}

pub fn measure(e: &Export) -> Result<Measured, Error> {
    let inst = e.instrument()?;
    measure_from(e, &render_notes(&inst, 80.0, MicPosition::Recording)?, &render_heads(&inst, 80.0, MicPosition::Player)?)
}

/// `measure` from renders already made: `rec` the whole notes at Recording (`render_notes`: the
/// decay needs their late spectra), `player` their heads at Player.
pub fn measure_from(e: &Export, rec: &Notes, player: &Notes) -> Result<Measured, Error> {
    if rec.late.len() != rec.head.len() {
        return Err(Error("roll: measuring needs the whole notes at Recording (the late spectra)".into()));
    }
    Ok(Measured { stats: stats_of(rec, ring(e)?), pr: difference(player, rec), level: rec.level.clone(), mono_vs_power: rec.mono_vs_power })
}

fn stats_of(notes: &Notes, ring: f64) -> Vec<f64> {
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let bright: Vec<f64> = notes
        .head
        .iter()
        .map(|h| {
            let m = mean(h);
            let c: Vec<f64> = h.iter().map(|v| v - m).collect();
            mean(&c[5..8]) - mean(&c[1..4])
        })
        .collect();
    let decay: Vec<f64> = notes.head.iter().zip(&notes.late).map(|(h, l)| mean(&h.iter().zip(l).map(|(a, b)| a - b).collect::<Vec<_>>())).collect();
    let reg = |v: &[f64]| [mean(&v[0..5]), mean(&v[5..10]), mean(&v[10..15])];
    let mut out = reg(&bright).to_vec();
    out.extend(reg(&decay));
    out.push(ring);
    out
}

/// Per octave: the notes' summed power in `a` minus in `b` (dB).
pub fn difference(a: &Notes, b: &Notes) -> [f64; OCTAVES] {
    let total = |n: &Notes| -> [f64; OCTAVES] { std::array::from_fn(|k| db(n.head.iter().map(|h| 10f64.powf(h[k] / 10.0)).sum())) };
    let (x, y) = (total(a), total(b));
    std::array::from_fn(|k| x[k] - y[k])
}

/// The responses' energy left 60 ms after their start (dB), their mean.
pub fn ring(e: &Export) -> Result<f64, Error> {
    let rs = e.responses()?;
    let sr = e.json.get("reference_sr").and_then(|v| v.as_f64()).unwrap_or(SR);
    let k = (0.06 * sr) as usize;
    Ok(rs.iter().map(|r| db(r.ir[k.min(r.ir.len())..].iter().map(|v| v * v).sum::<f64>()) - db(r.ir.iter().map(|v| v * v).sum::<f64>())).sum::<f64>() / rs.len() as f64)
}

/// Per octave (62.5 Hz … 16 kHz): the notes' power over 0.05–0.5 s at Player minus at
/// Recording (dB), velocity 80.
pub fn player_minus_recording(e: &Export) -> Result<[f64; OCTAVES], Error> {
    let inst = e.instrument()?;
    Ok(difference(&render_heads(&inst, 80.0, MicPosition::Player)?, &render_heads(&inst, 80.0, MicPosition::Recording)?))
}

/// The references' range: each statistic widened by `MARGIN` of its span on each side, Player minus
/// Recording per octave by 1 dB.
pub struct Realism {
    pub lo: Vec<f64>,
    pub hi: Vec<f64>,
    pub pr_lo: [f64; OCTAVES],
    pub pr_hi: [f64; OCTAVES],
}

impl Realism {
    pub fn from_references(refs: &[Export]) -> Result<Realism, Error> {
        let m: Vec<Measured> = refs.iter().map(measure).collect::<Result<_, _>>()?;
        Ok(Realism::from_measured(&m))
    }

    pub fn from_measured(m: &[Measured]) -> Realism {
        let s: Vec<&Vec<f64>> = m.iter().map(|x| &x.stats).collect();
        let p: Vec<[f64; OCTAVES]> = m.iter().map(|x| x.pr).collect();
        let n = s[0].len();
        let (mut lo, mut hi) = (Vec::new(), Vec::new());
        for j in 0..n {
            let (a, b) = s.iter().map(|v| v[j]).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
            lo.push(a - MARGIN * (b - a));
            hi.push(b + MARGIN * (b - a));
        }
        let pr_lo = std::array::from_fn(|b| p.iter().map(|v| v[b]).fold(f64::INFINITY, f64::min) - 1.0);
        let pr_hi = std::array::from_fn(|b| p.iter().map(|v| v[b]).fold(f64::NEG_INFINITY, f64::max) + 1.0);
        Realism { lo, hi, pr_lo, pr_hi }
    }

    /// How far outside the range: per statistic and octave the excess as a share of the range's
    /// span, summed (0 inside).
    pub fn excess(&self, m: &Measured) -> f64 {
        let out = |v: f64, lo: f64, hi: f64| (lo - v).max(v - hi).max(0.0) / (hi - lo).max(1e-9);
        let stats: f64 = m.stats.iter().enumerate().map(|(j, &v)| out(v, self.lo[j], self.hi[j])).sum();
        let pr: f64 = m.pr.iter().enumerate().map(|(b, &v)| out(v, self.pr_lo[b], self.pr_hi[b])).sum();
        stats + pr
    }

    /// One line per statistic outside the range: "<name> <value> (real <lo> … <hi>)".
    pub fn check(&self, e: &Export) -> Result<Vec<String>, Error> {
        Ok(self.check_measured(&measure(e)?))
    }

    pub fn check_measured(&self, m: &Measured) -> Vec<String> {
        let mut out = Vec::new();
        let (stats, pr) = (&m.stats, &m.pr);
        for (j, v) in stats.iter().enumerate() {
            if !(self.lo[j]..=self.hi[j]).contains(v) {
                out.push(format!("{} {v:.1} (real {:.1} … {:.1})", STAT_NAMES[j], self.lo[j], self.hi[j]));
            }
        }
        for (b, v) in pr.iter().enumerate() {
            if !(self.pr_lo[b]..=self.pr_hi[b]).contains(v) {
                out.push(format!("Player against Recording at {:.0} Hz {v:+.1} dB (real {:+.1} … {:+.1})", band_lo(b) * 2f64.sqrt(), self.pr_lo[b], self.pr_hi[b]));
            }
        }
        out
    }
}

/// Each key's fundamental (f0 √(1 + B)).
pub fn fundamentals(e: &Export) -> Vec<(u64, f64)> {
    let notes = e.json.get("notes").and_then(|n| n.as_array()).cloned().unwrap_or_default();
    notes
        .iter()
        .filter_map(|n| {
            let s = n.get("string")?;
            Some((n.get("note")?.as_u64()?, s.get("f0")?.as_f64()? * (1.0 + s.get("B")?.as_f64()?).sqrt()))
        })
        .collect()
}

/// A fundamental deeper than 15 dB in a notch of a vertical response (`bands::notch_depths`, as
/// `proto/tests/test_radiation_notches.py` checks the exports): the deepest, its key and channel.
fn deepest_notch(e: &Export, rs: &[super::files::Response], sr: f64) -> Option<(u64, f64, usize)> {
    let keys = fundamentals(e);
    let f: Vec<f64> = keys.iter().map(|k| k.1).collect();
    rs.iter()
        .filter(|r| r.pol == 0)
        .flat_map(|r| bands::notch_depths(&r.ir, sr, &f).into_iter().enumerate().map(move |(i, d)| (i, d, r.channel)))
        .filter(|&(_, d, _)| d < -15.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, d, c)| (keys[i].0, d, c))
}

/// What only a roll can get wrong: the bass out of phase between the microphones (the mono sum
/// under the channels' power by more than 1.5 dB at 63 or 125 Hz), the two microphones' colors
/// apart (a band from 60 Hz up more than 3 dB apart, per polarization) and a fundamental in a
/// notch of the radiation (a mixed fine structure can put one there).
pub fn faults(e: &Export) -> Result<Vec<String>, Error> {
    let notes = render_heads(&e.instrument()?, 80.0, MicPosition::Recording)?;
    faults_measured(e, &notes.mono_vs_power)
}

/// `faults` with the Recording render's mono against power already measured.
pub fn faults_measured(e: &Export, mono_vs_power: &[f64; OCTAVES]) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    for (b, &v) in mono_vs_power.iter().enumerate().take(2) {
        if v < -1.5 {
            out.push(format!("bass out of phase at {:.0} Hz: the mono sum {v:+.1} dB", band_lo(b) * 2f64.sqrt()));
        }
    }
    let sr = e.json.get("reference_sr").and_then(|v| v.as_f64()).unwrap_or(SR);
    let g = bands::grid();
    let rs = e.responses()?;
    if let Some((key, depth, channel)) = deepest_notch(e, &rs, sr) {
        out.push(format!("a fundamental in a notch: key {key} {depth:.1} dB under the response around it ({})", ["left", "right"][channel.min(1)]));
    }
    for pol in 0..2 {
        let pair: Vec<_> = rs.iter().filter(|r| r.pol == pol).collect();
        if pair.len() < 2 {
            continue;
        }
        let (l, r) = (bands::band_stats(&pair[0].ir, sr).0, bands::band_stats(&pair[1].ir, sr).0);
        let worst = (0..bands::BANDS).filter(|&k| g[k] >= 60.0).max_by(|&a, &b| (l[a] - r[a]).abs().total_cmp(&(l[b] - r[b]).abs()));
        if let Some(k) = worst.filter(|&k| (l[k] - r[k]).abs() > 3.0) {
            out.push(format!("microphones apart: {:+.1} dB at {:.0} Hz ({})", l[k] - r[k], g[k], ["vertical", "horizontal"][pol]));
        }
    }
    Ok(out)
}
