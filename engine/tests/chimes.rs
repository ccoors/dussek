//! The tubular bells (`proto/bars.py` "chimes"): a tube's modes re its fourth (twice the strike
//! note), the modes' slow beating (`BarModes::split`), a damper bar lifted by a held key or the
//! pedal. Needs the exported instrument (skipped without).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/chimes");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

/// The band's level (dB) at f every 10 ms: the signal heterodyned, an 80 ms Hann.
fn band_db(x: &[f32], f: f64) -> Vec<f64> {
    let n = (0.08 * SR) as usize;
    let w: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos()).collect();
    let om = 2.0 * std::f64::consts::PI * f / SR;
    (0..(x.len() - n) / 480)
        .map(|b| {
            let (mut re, mut im) = (0.0, 0.0);
            for i in 0..n {
                let k = b * 480 + i;
                re += x[k] as f64 * w[i] * (om * k as f64).cos();
                im -= x[k] as f64 * w[i] * (om * k as f64).sin();
            }
            10.0 * (re * re + im * im + 1e-30).log10()
        })
        .collect()
}

#[test]
fn a_tube_rings_at_its_fourth_mode_twice_the_strike_note() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == dussek::mic::Body::Chimes && !inst.motor);
    for key in [60u8, 66, 77] {
        let st = &inst.notes[key as usize - 21].spec.string;
        let pitch = 440.0 * 2f64.powf((key as f64 - 69.0) / 12.0);
        let cents = 1200.0 * (st.f0 / (2.0 * pitch)).log2();
        assert!(cents.abs() < 20.0, "key {key}: the reference mode {:.1} Hz, {cents:+.1} cents off 2 × the pitch", st.f0);
        let m = st.modes.as_ref().expect("a tube's modes");
        // the two below it (0.31 and 0.61 re it) and the six above
        assert!(m.ratio.iter().filter(|&&r| r < 1.0).count() == 2 && m.ratio.len() == 9);
        assert!(m.split.len() == 9 && m.split.iter().all(|&(hz, r)| hz.abs() < 5.0 && r > 0.0 && r < 0.5));
    }
}

#[test]
fn the_modes_beat_at_their_split() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let key = 64u8;
    let st = inst.notes[key as usize - 21].spec.string.clone();
    let m = st.modes.clone().unwrap();
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, ..Params::default() });
    e.queue(0, Event::NoteOn { key, speed: 1.0 });
    let x = render(&mut e, (8.5 * SR) as usize);
    // the reference mode's band over 0.3–8 s, its decay's trend taken out: it swings at its split
    let e_db = band_db(&x, st.f0);
    let (a, b) = (30, 800);
    let n = (b - a) as f64;
    let t: Vec<f64> = (a..b).map(|i| i as f64 * 0.01).collect();
    let y: Vec<f64> = e_db[a..b].to_vec();
    let (tm, ym) = (t.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let slope = t.iter().zip(&y).map(|(t, y)| (t - tm) * (y - ym)).sum::<f64>() / t.iter().map(|t| (t - tm).powi(2)).sum::<f64>();
    let res: Vec<f64> = t.iter().zip(&y).map(|(t, y)| y - ym - slope * (t - tm)).collect();
    let std = (res.iter().map(|r| r * r).sum::<f64>() / n).sqrt();
    let (hz, share) = m.split[0];
    let q = share / (1.0 - share);
    let want = 20.0 / std::f64::consts::LN_10 / 2f64.sqrt() * q;
    assert!(std > 0.5 * want && std < 2.0 * want, "swing {std:.2} dB, the split's {want:.2} dB ({hz:+.2} Hz)");
    // the swing's period: zero crossings of the residual
    let crossings = res.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f64;
    let rate = crossings / (n * 0.01);
    assert!((rate - hz.abs()).abs() < 0.35 * hz.abs() + 0.1, "beat rate {rate:.2} Hz, the split's {:.2}", hz.abs());
}

#[test]
fn a_held_key_rings_and_the_damper_bar_takes_it_at_the_release() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let level = |hold: u64, pedal: bool| -> f64 {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        if pedal {
            e.queue(0, Event::PedalDown);
        }
        e.queue(0, Event::NoteOn { key: 67, speed: 1.0 });
        e.queue(hold, Event::KeyUp { key: 67 });
        let x = render(&mut e, (3.0 * SR) as usize);
        let rms = |s: &[f32]| (s.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
        20.0 * (rms(&x[(2.5 * SR) as usize..]) / rms(&x[(0.1 * SR) as usize..(0.3 * SR) as usize])).log10()
    };
    let (held, released, pedalled) = (level((3.0 * SR) as u64, false), level((0.4 * SR) as u64, false), level((0.4 * SR) as u64, true));
    assert!(held > -25.0 && pedalled > -25.0, "held {held:.1} dB, pedalled {pedalled:.1} dB after 2.5 s");
    assert!(released < held - 30.0, "released {released:.1} dB, held {held:.1} dB");
}
