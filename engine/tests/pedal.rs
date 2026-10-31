//! The strings set ringing as the pedal lifts their dampers (`Voice::kick`, `proto/pedal.py`).
//! Needs the exports (skipped without).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

/// A pedal press at 0.1 s (left channel): the thumps alone (`thump`) or the strings' ring alone.
fn press(name: &str, thump: bool, kick: Option<(f64, f64)>) -> Option<Vec<f32>> {
    // `kick`: another (center, spread) for the kick
    let mut inst = Instrument::load(format!("{ROOT}{name}")).ok()?;
    let p = inst.pedal.as_mut()?;
    if thump {
        p.kick_amp = 0.0;
    } else {
        p.down.clear();
        p.up.clear();
    }
    if let Some((center, spread)) = kick {
        p.kick_center = center;
        p.kick_spread = spread;
    }
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, ..Params::default() });
    e.queue(4800, Event::PedalDown);
    let n = 48000;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    Some(l)
}

/// A pedal press at 0.1 s (left channel), the instrument as exported, at `params` (no room).
fn press_at(name: &str, params: Params) -> Option<Vec<f32>> {
    let inst = Instrument::load(format!("{ROOT}{name}")).ok()?;
    inst.pedal.as_ref()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, ..params });
    e.queue(4800, Event::PedalDown);
    let n = 48000;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    Some(l)
}

#[test]
fn the_thumps_and_the_ring_have_their_own_gains() {
    // the Pedal noise control sets the thumps, the Pedal ring the strings set ringing as the dampers
    // lift: either at 0 leaves the other as it is alone
    for name in ["grand_a", "upright_d"] {
        let Some(thump) = press(name, true, None) else { continue };
        let ring = press(name, false, None).unwrap();
        assert_eq!(press_at(name, Params { pedal_ring: 0.0, ..Params::default() }).unwrap(), thump, "{name}: the thumps");
        assert_eq!(press_at(name, Params { pedal: 0.0, ..Params::default() }).unwrap(), ring, "{name}: the ring");
        let half = press_at(name, Params { pedal: 0.0, pedal_ring: 0.5, ..Params::default() }).unwrap();
        let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak(&half) / peak(&ring) - 0.5).abs() < 0.01, "{name}: the ring at half its gain");
    }
}

/// RBJ highpass biquad at `fc`.
fn highpass(x: &[f32], fc: f64) -> Vec<f64> {
    let w = 2.0 * std::f64::consts::PI * fc / SR;
    let (c, a) = (w.cos(), w.sin() / std::f64::consts::SQRT_2);
    let (b0, b1, b2, a0, a1, a2) = ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + a, -2.0 * c, 1.0 - a);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    x.iter()
        .map(|&v| {
            let v = v as f64;
            let y = (b0 * v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2) / a0;
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y
        })
        .collect()
}

/// The peak above 2 kHz in the 80 ms after the press.
fn onset_hf(x: &[f32]) -> f64 {
    highpass(x, 2000.0)[4800..4800 + 3840].iter().fold(0.0f64, |m, v| m.max(v.abs()))
}

#[test]
fn the_dampers_leave_the_strings_without_a_click() {
    // the strings lie still under the dampers and leave them moving: from rest, the unison's
    // strings against each other. Kicked from a displacement the bridge force jumped at the lift:
    // a click on every press once the refit (step 50) put the kick on the low bass's single strings
    // (Grand B: 18 dB over the thump above 2 kHz, Grand D 33). Now from rest, the kick's shape
    // Salamander's (proto/pedal.py SHAPE_FROM): 33-55 dB under it; the kick widened by the dampers'
    // refit (step 119) put Grand D's 85 dB under an mf note, 24 dB over before, still inaudible
    for name in ["grand_a", "grand_b", "grand_f", "grand_e", "grand_d", "grand_c", "grand_g", "upright_a", "upright_b", "upright_c"] {
        let Some(thump) = press(name, true, None) else { continue };
        let ring = press(name, false, None).unwrap();
        // re the thump's whole peak: its part above 2 kHz is the microphones' perspective (13 dB lower
        // since step 121), not a level the ring's click could be judged by
        let peak = thump[4800..4800 + 3840].iter().fold(0.0f64, |m, v| m.max(v.abs() as f64));
        let c = 20.0 * (onset_hf(&ring) / peak).log10();
        assert!(c < -20.0, "{name}: the ring's onset above 2 kHz {c:.1} dB re the thump's peak");
    }
}

#[test]
fn the_sympathetic_strings_take_no_click_from_the_dampers() {
    // Grand D, a legato line with the pedal between half-down and up (as in half-pedalled Bach).
    // The bank switched each key's drive in one sample as its dampers landed or lifted: the
    // resonators' forced response stopped or started at once and each rang on at its own
    // frequency, a broadband burst (the excerpt: 35–50 dB over the bank's level before it, to
    // 16 kHz; here the largest 20 ms at 6–16 kHz 33 dB over the median). Its drive fades instead
    // (`resonance::DRIVE_RAMP_S`): 10
    let name = format!("{ROOT}grand_d");
    if Instrument::load(&name).is_err() {
        return;
    }
    let run = |sympathetic: f32| {
        let mut e = Engine::with_tuning(Instrument::load(&name).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, sympathetic, ..Params::default() });
        let at = |t: f64| (t * SR) as u64;
        let line = [62u8, 70, 72, 70, 69, 70, 74, 72, 70, 69, 67, 69, 70, 72, 74, 75];
        for (i, &key) in line.iter().enumerate() {
            let t = 0.3 + 0.25 * i as f64;
            let speed = e.hammer_speed(key, 70.0);
            e.queue(at(t), Event::NoteOn { key, speed });
            e.queue(at(t + 0.3), Event::KeyUp { key });
        }
        // the pedal: down to 80/127 over 0.1 s, held, back up over 0.1 s, every 0.55 s
        let mut tp = 0.2;
        while tp < 4.3 {
            for v in (0..80).step_by(8) {
                e.queue(at(tp + v as f64 / 800.0), Event::Pedal { depth: v as f32 / 127.0 });
            }
            for v in (0..=80).rev().step_by(8) {
                e.queue(at(tp + 0.4 + (80 - v) as f64 / 800.0), Event::Pedal { depth: v as f32 / 127.0 });
            }
            tp += 0.55;
        }
        let n = (4.8 * SR) as usize;
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        l
    };
    // the sympathetic strings' share (all else alike), 6–16 kHz in 20 ms windows
    let bank: Vec<f32> = run(1.0).iter().zip(run(0.0)).map(|(a, b)| a - b).collect();
    // three highpasses: 36 dB per octave (one let the bank's 1–5 kHz through, the median theirs)
    let h1: Vec<f32> = highpass(&bank, 6000.0).iter().map(|&v| v as f32).collect();
    let h2: Vec<f32> = highpass(&h1, 6000.0).iter().map(|&v| v as f32).collect();
    let h = highpass(&h2, 6000.0);
    let w = (0.02 * SR) as usize;
    let mut e: Vec<f64> = h.chunks_exact(w).map(|c| c.iter().map(|v| v * v).sum::<f64>() + 1e-40).collect();
    let top = e.iter().cloned().fold(0.0, f64::max);
    e.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let burst = 10.0 * (top / e[e.len() / 2]).log10();
    assert!(burst < 20.0, "the sympathetic strings' largest 20 ms at 6–16 kHz: {burst:+.1} dB re the median");
}
