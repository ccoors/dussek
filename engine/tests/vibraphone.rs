//! The vibraphone (`proto/export_bars.py`): a bar's mode table in place of a string's
//! partials, its mallet (the pluck path's `bar`), the damper bar (lifted by a held key or the pedal).
//! Needs the exported instrument (skipped without).

use dussek::physics::{BarModes, PluckSpec, pluck_amplitude};
use dussek::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/vibraphone");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

#[test]
fn the_mallet_brightens_with_speed_and_hardness() {
    let p = PluckSpec { bar: true, order: 2.0, ..PluckSpec::string(0.5, 600.0, 1.0, 6.02, 2.0) };
    // no string shape: every mode alike at the cutoff's far side of its lowpass
    assert!((pluck_amplitude(1.0, 100.0, &p, 0.0, 1.0) - pluck_amplitude(2.0, 100.0, &p, 0.0, 1.0)).abs() < 1e-12);
    let share = |steps: f64, hardness: f64| pluck_amplitude(3.0, 2600.0, &p, steps, hardness) / pluck_amplitude(1.0, 260.0, &p, steps, hardness);
    assert!(share(-1.0, 1.0) < share(0.0, 1.0) && share(0.0, 1.0) < share(1.0, 1.0));
    assert!(share(0.0, 1.0) < share(0.0, 2.0));
    // amplitude ∝ speed far below the cutoff
    let lo = |steps: f64| pluck_amplitude(1.0, 1.0, &p, steps, 1.0);
    assert!((20.0 * (lo(1.0) / lo(0.0)).log10() - 6.02).abs() < 0.01);
}

#[test]
fn bars_compile_to_their_mode_table() {
    let Ok(mut inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == dussek::mic::Body::Vibraphone);
    let i = 62 - 21;
    let m: BarModes = inst.notes[i].spec.string.modes.clone().expect("a bar");
    let f0 = inst.notes[i].spec.string.f0;
    inst.compile(SR, Tuning::default());
    let n = &inst.notes[i];
    assert_eq!(n.phys.k, m.ratio.len());
    for j in 0..n.phys.k {
        assert!((n.phys.f_mean[j] - f0 * m.ratio[j]).abs() < 1e-9 * f0);
        assert!((n.phys.sigma_int[j] - m.sigma[j]).abs() < 1e-12);
        assert!((n.phys.bridge[j] - m.out[j]).abs() < 1e-12 * m.out[j]);
    }
    // the top key's upper modes lie beyond 0.45·sr at 44.1 kHz: left out
    inst.compile(44100.0, Tuning::default());
    let top = &inst.notes[87];
    assert!(top.phys.k < 3 && top.phys.f_mean.iter().all(|&f| f < 0.45 * 44100.0));
}

#[test]
fn a_held_key_rings_and_the_damper_bar_takes_it_at_the_release() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    // played from a keyboard: a held key lifts the damper bar off its bar (the real bar
    // follows the pedal alone, which made every note a short one without pedal); the key's
    // release lets it fall, unless the pedal holds it up
    let tail = |pedal: bool, hold: u64| -> f64 {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        if pedal {
            e.queue(0, Event::PedalDown);
        }
        e.queue(0, Event::NoteOn { key: 62, speed: 1.0 });
        e.queue(hold, Event::KeyUp { key: 62 });
        let x = render(&mut e, 120000);
        assert!(x.iter().all(|v| v.is_finite()));
        20.0 * (rms(&x[96000..120000]) / rms(&x[2400..12000])).log10()
    };
    let (held_up, released_up, released_down) = (tail(false, 110000), tail(false, 4800), tail(true, 4800));
    // held or pedalled: ~3.5 dB/s on the fundamental; released, pedal up: ~80 dB/s, far below
    assert!(held_up > -15.0, "held, pedal up: {held_up:.1} dB");
    assert!(released_down > -15.0, "released, pedal down: {released_down:.1} dB");
    assert!(released_up < -60.0, "released, pedal up: {released_up:.1} dB");
}

#[test]
fn a_strike_starts_without_a_click() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    // a struck bar starts from rest: the output's change in the first 4 ms (a first difference,
    // the click's high end) stays well below its later level (started from their displacement,
    // the modes stepped at the strike: the energy above 4 kHz −31 dB re the body at mf, now −45)
    let speed = inst.velocity_speed[1];
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, ..Params::default() });
    e.queue(0, Event::NoteOn { key: 62, speed });
    let x = render(&mut e, 4800);
    let on = x.iter().position(|v| v.abs() > 1e-6).expect("sound");
    let d: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
    let (a, b) = (on + (0.004 * SR) as usize, on + (0.04 * SR) as usize);
    let ratio = 20.0 * (rms(&d[on..a]) / rms(&x[a..b])).log10();
    assert!(ratio < -20.0, "first difference in the first 4 ms re the body: {ratio:.1} dB");
}

/// Level (dB) of x at f over windows of `win` samples every `hop` (a Hann-windowed DFT bin).
fn band_env(x: &[f32], f: f64, win: usize, hop: usize) -> Vec<f64> {
    (0..(x.len() - win) / hop)
        .map(|k| {
            let s = &x[k * hop..k * hop + win];
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &v) in s.iter().enumerate() {
                let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / win as f64).cos();
                let ph = 2.0 * std::f64::consts::PI * f * i as f64 / SR;
                re += w * v as f64 * ph.cos();
                im -= w * v as f64 * ph.sin();
            }
            20.0 * (re.hypot(im) + 1e-30).log10()
        })
        .collect()
}

fn ring(motor: bool, key: u8, n: usize) -> (Vec<f32>, Vec<f64>) {
    let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, motor, motor_rate: 5.0, motor_depth: 1.0, ..Params::default() });
    e.queue(0, Event::PedalDown);
    e.queue(0, Event::NoteOn { key, speed: 1.0 });
    let x = render(&mut e, n);
    let f = e.instrument().notes[key as usize - 21].f_mean.clone();
    (x, f)
}

#[test]
fn the_motor_pulses_the_fundamental_alone() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let n = 3 * 48000;
    let (x, f) = ring(true, 62, n);
    let (y, _) = ring(false, 62, n);
    assert!(x.iter().all(|v| v.is_finite()));
    // after the spin-up (1 s on): the fundamental's level swings, re its decay without the motor
    let (win, hop) = (2048, 240);
    let detrend = |a: &[f64], b: &[f64]| -> Vec<f64> { a.iter().zip(b).map(|(p, q)| p - q).skip(200).collect() };
    let f1 = detrend(&band_env(&x, f[0], win, hop), &band_env(&y, f[0], win, hop));
    let f2 = detrend(&band_env(&x, f[1], win, hop), &band_env(&y, f[1], win, hop));
    let span = |v: &[f64]| v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min);
    assert!(span(&f1) > 8.0, "fundamental swing {:.1} dB", span(&f1));
    // (from 1 to 2 s: later the fundamental's leakage into its bin, 40 dB over it by then, shows)
    assert!(span(&f2[..200]) < 0.3, "second mode swing {:.1} dB", span(&f2[..200]));
    // five pulses a second: the swing's minima 0.2 s apart (the 43 ms window smooths the dips)
    let lo: Vec<usize> = (1..f1.len() - 1).filter(|&i| f1[i] < f1[i - 1] && f1[i] <= f1[i + 1] && f1[i] < -4.0).collect();
    let gaps: Vec<f64> = lo.windows(2).map(|w| (w[1] - w[0]) as f64 * hop as f64 / SR).collect();
    assert!(gaps.len() >= 6 && gaps.iter().all(|g| (g - 0.2).abs() < 0.02), "{gaps:?}");
}

#[test]
fn a_motor_switched_off_changes_nothing() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let (y, _) = ring(false, 62, 48000);
    let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, motor_rate: 9.0, motor_depth: 0.3, ..Params::default() });
    e.queue(0, Event::PedalDown);
    e.queue(0, Event::NoteOn { key: 62, speed: 1.0 });
    assert_eq!(render(&mut e, 48000), y);
}

#[test]
fn a_bar_that_has_died_away_frees_its_voice() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    // released, pedal up: the damper bar stops it within a second
    e.queue(0, Event::NoteOn { key: 62, speed: 1.0 });
    e.queue(2400, Event::KeyUp { key: 62 });
    render(&mut e, 4800);
    assert_eq!(e.stats().voices, 1);
    render(&mut e, 3 * 48000);
    assert_eq!(e.stats().voices, 0, "{} modes still running", e.stats().modes);
}

#[test]
fn harmonics_follow_their_law() {
    use dussek::telemetry::{MAX_PARTIALS, Partial};
    use dussek::voice::Voice;
    let Ok(mut inst) = Instrument::load(DIR) else { return };
    inst.compile(SR, Tuning::default());
    let note = &inst.notes[62 - 21];
    let t = note.tube.as_ref().expect("a resonator");
    let hs = &t.harmonics;
    assert!(hs.h.first() == Some(&2), "{:?}", hs.h);
    let (f1, s1, sd) = (note.f_mean[0], note.sigma_int[0], note.damper_sigma[0]);
    // the voice's modes after a strike from rest: (frequency, decay, amplitude)
    let strike = |engage: f64, speed: f64| -> Vec<Partial> {
        let mut v = Voice::new(note, SR, inst.reference_sr, inst.contact_max_s, inst.contact_separation_s, 64);
        v.set_engage(engage);
        v.pluck(note, speed, 1.0);
        let mut out = [Partial { freq: 0.0, decay: 0.0, amp: 0.0, kind: 0 }; MAX_PARTIALS];
        let (n, _, _) = v.partials(&mut out);
        out[..n].to_vec()
    };
    let at = |ps: &[Partial], f: f64| ps.iter().find(|p| (p.freq as f64 / f - 1.0).abs() < 1e-3).copied();
    let (mf, ff) = (inst.velocity_speed[1], inst.velocity_speed[2]);
    for (engage, laws, loss) in [(0.0, &hs.free, s1), (1.0, &hs.damped, s1 + sd)] {
        let (top, soft) = (strike(engage, ff), strike(engage, mf));
        let (a_ff, a_mf) = (at(&top, f1).expect("f1").amp as f64, at(&soft, f1).expect("f1").amp as f64);
        // from rest at ff: the fundamental at the reference
        assert!((a_ff / t.a_ref - 1.0).abs() < 1e-3, "{a_ff} {}", t.a_ref);
        for (i, &h) in hs.h.iter().enumerate() {
            let law = laws[i];
            if law.level_db < -75.0 || h as f64 * f1 > 0.45 * SR {
                continue;
            }
            let p = at(&top, h as f64 * f1).unwrap_or_else(|| panic!("harmonic {h} (engage {engage})"));
            let lv = 20.0 * (p.amp as f64 / a_ff).log10();
            assert!((lv - law.level_db).abs() < 0.01, "h {h}: {lv:.2} dB, law {:.2}", law.level_db);
            assert!((p.decay as f64 / (law.decay * loss) - 1.0).abs() < 1e-3, "h {h}: decay {}", p.decay);
            // softer: its level re the fundamental falls by growth × the fundamental's fall
            let q = at(&soft, h as f64 * f1).expect("harmonic at mf");
            let drop = 20.0 * (q.amp as f64 / a_mf).log10() - lv;
            let want = law.growth * 20.0 * (a_mf / a_ff).log10();
            assert!((drop - want).abs() < 0.01, "h {h}: {drop:.2} dB, want {want:.2}");
        }
    }
}
