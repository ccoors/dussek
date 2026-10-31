//! The audio input (`Engine::process_input`, `Params::input`): a sound at the instrument drives
//! the free strings through the sympathetic bank. Needs the exports (skipped without).

use pianoengine::{Engine, Event, Instrument, Params};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;
const PIANO: &str = "grand_a";

/// uniform noise at −20 dBFS RMS
fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * 0.1 * 3f32.sqrt()
        })
        .collect()
}

/// A noise burst (0.1 … 0.6 s) into the input at `level`, the pedal down or not, no pedal noise
/// and no room; 2 s, the left channel, and the bank's running resonators at the burst's end.
fn burst(name: &str, level: f32, pedal: bool) -> Option<(Vec<f32>, usize)> {
    let inst = Instrument::load(format!("{ROOT}{name}")).ok()?;
    let mut e = Engine::new(inst, SR).unwrap();
    e.set_params(Params { input: level, pedal: 0.0, room: 0.0, ..Params::default() });
    if pedal {
        e.queue(0, Event::PedalDown);
    }
    let n = (2.0 * SR) as usize;
    let mut x = vec![0.0; (0.1 * SR) as usize];
    x.extend(noise((0.5 * SR) as usize, 1));
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    let mut running = 0;
    for (k, (cl, cr)) in l.chunks_mut(256).zip(r.chunks_mut(256)).enumerate() {
        let at = k * 256;
        e.process_input(x.get(at..(at + 256).min(x.len())).unwrap_or(&[]), cl, cr);
        if at < x.len() {
            running = e.stats().sympathetic;
        }
    }
    Some((l, running))
}

fn rms_db(x: &[f32]) -> f64 {
    10.0 * (x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / x.len() as f64).max(1e-30).log10()
}

/// the tail after the burst: 0.8 … 1.3 s after it ends, when the board's own ring has died away (its
/// low modes ring since design log step 184: the 63 Hz octave 90 dB/s, at 0.1 … 0.6 s after the
/// burst 2 dB over the strings' 1 kHz octave; the window was 0.1 … 0.6 s)
fn tail(x: &[f32]) -> &[f32] {
    &x[(1.4 * SR) as usize..(1.9 * SR) as usize]
}

/// energy (dB) below and above `hz`
fn split_db(x: &[f32], hz: f64) -> (f64, f64) {
    let n = x.len().next_power_of_two();
    let mut buf: Vec<f64> = x.iter().map(|&v| v as f64).collect();
    buf.resize(n, 0.0);
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut spec = fft.make_output_vec();
    fft.process(&mut buf, &mut spec).unwrap();
    let (mut lo, mut hi) = (1e-300, 1e-300);
    for (k, c) in spec.iter().enumerate() {
        if (k as f64 * SR / n as f64) < hz {
            lo += c.norm_sqr();
        } else {
            hi += c.norm_sqr();
        }
    }
    (10.0 * lo.log10(), 10.0 * hi.log10())
}

#[test]
fn a_noise_burst_with_the_pedal_down_rings_the_strings() {
    let (Some((on, running)), Some((off, _))) = (burst(PIANO, 1.0, true), burst(PIANO, 0.0, true)) else { return };
    assert!(running > 1000, "{running} resonators running");
    let (ring, quiet) = (rms_db(tail(&on)), rms_db(tail(&off)));
    // the strings go on ringing after the sound has stopped
    assert!(ring > -60.0, "ring {ring:.1} dBFS");
    assert!(quiet < -200.0, "without the input {quiet:.1} dBFS");
}

#[test]
fn with_the_pedal_up_only_the_undamped_strings_ring() {
    let Ok(inst) = Instrument::load(format!("{ROOT}{PIANO}")) else { return };
    let lowest = inst.notes[..88].iter().find(|n| !n.has_damper).expect("undamped treble").note;
    let f0 = 440.0 * 2f64.powf((lowest as f64 - 69.0) / 12.0);
    let (Some((up, running)), Some((down, all))) = (burst(PIANO, 1.0, false), burst(PIANO, 1.0, true)) else { return };
    assert!(running > 0 && running < all / 10, "{running} of {all} resonators");
    let (ring_up, ring_down) = (rms_db(tail(&up)), rms_db(tail(&down)));
    assert!(ring_up < ring_down - 10.0, "pedal up {ring_up:.1} dBFS, down {ring_down:.1} dBFS");
    // nothing below the lowest undamped key's fundamental (key {lowest}, {f0:.0} Hz)
    let (lo, hi) = split_db(tail(&up), 0.85 * f0);
    assert!(lo < hi - 30.0, "below {:.0} Hz {lo:.1} dB, above {hi:.1} dB", 0.85 * f0);
}

/// Level 0 (the default) is today's output, bit for bit, whatever comes in.
#[test]
fn level_zero_is_bit_identical() {
    let render = |input: bool| -> Option<Vec<f32>> {
        let inst = Instrument::load(format!("{ROOT}{PIANO}")).ok()?;
        let mut e = Engine::new(inst, SR).unwrap();
        e.set_params(Params::default());
        e.queue(0, Event::NoteOn { key: 48, speed: 3.0 });
        e.queue(4800, Event::PedalDown);
        e.queue(24000, Event::KeyUp { key: 48 });
        e.queue(36000, Event::NoteOn { key: 64, speed: 1.0 });
        let n = 60000;
        let x = noise(n, 3);
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        for ((cl, cr), cx) in l.chunks_mut(100).zip(r.chunks_mut(100)).zip(x.chunks(100)) {
            if input {
                e.process_input(cx, cl, cr);
            } else {
                e.process(cl, cr);
            }
        }
        l.extend(r);
        Some(l)
    };
    let (Some(a), Some(b)) = (render(false), render(true)) else { return };
    assert!(a == b);
}
