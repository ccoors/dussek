//! The kalimba (`proto/bars.py` "kalimba"): a tine's cantilever modes at equal temperament, the
//! instrument's other tines ringing along (extra modes of each key, `halo`), no dampers. Needs the
//! exported instrument (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/kalimba");
const SR: f64 = 48000.0;
/// the recorded tines (VCSL's Tanzanian kalimba, at their keys)
const TINES: [u8; 25] = [43, 44, 47, 49, 51, 53, 55, 58, 61, 63, 65, 67, 70, 73, 74, 75, 77, 79, 82, 83, 85, 88, 90, 92, 97];

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

fn pitch(key: f64) -> f64 {
    440.0 * 2f64.powf((key - 69.0) / 12.0)
}

#[test]
fn a_tine_rings_at_equal_temperament_with_the_other_tines_along() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == pianoengine::mic::Body::Kalimba && !inst.motor);
    for key in [48u8, 61, 74, 90] {
        let n = &inst.notes[key as usize - 21];
        let st = &n.spec.string;
        assert!((1200.0 * (st.f0 / pitch(key as f64)).log2()).abs() < 0.01, "key {key}: {:.2} Hz", st.f0);
        assert!(!n.spec.has_damper, "key {key}: a kalimba has no dampers");
        let m = st.modes.as_ref().expect("a tine's modes");
        // the second cantilever mode near 6.3–6.8
        assert!(m.ratio[1] > 6.0 && m.ratio[1] < 7.5, "key {key}: {:?}", &m.ratio[..3]);
        // the ringing tines (from `halo_from`, then their onset partners): at the other tines'
        // pitches, at most the instrument's
        let h = &m.ratio[m.halo_from..];
        assert!(h.len() % 2 == 0 && h[h.len() / 2..] == h[..h.len() / 2], "key {key}: the ringing tines' partners");
        let halo: Vec<f64> = h[..h.len() / 2].iter().map(|r| r * st.f0).collect();
        assert!(halo.len() >= 15 && halo.len() <= TINES.len(), "key {key}: {} ringing tines", halo.len());
        for f in &halo {
            assert!(TINES.iter().any(|&t| (1200.0 * (f / pitch(t as f64)).log2()).abs() < 0.5), "key {key}: {f:.1} Hz is no tine");
        }
    }
}

#[test]
fn a_released_tine_rings_on() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let level = |hold: u64| -> f64 {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 55, speed: 1.0 });
        e.queue(hold, Event::KeyUp { key: 55 });
        let x = render(&mut e, (1.5 * SR) as usize);
        (x[(1.0 * SR) as usize..].iter().map(|&v| (v as f64).powi(2)).sum::<f64>()).sqrt()
    };
    let (held, released) = (level((1.5 * SR) as u64), level((0.1 * SR) as u64));
    assert!((20.0 * (released / held).log10()).abs() < 0.5, "held {held:.3e}, released {released:.3e}");
}

/// The level (dB) at f over x[a..b] (Hann).
fn level(x: &[f32], a: usize, b: usize, f: f64) -> f64 {
    let x = &x[a..b];
    let n = x.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n).cos();
        let ph = 2.0 * std::f64::consts::PI * f * i as f64 / SR;
        re += w * v as f64 * ph.cos();
        im -= w * v as f64 * ph.sin();
    }
    10.0 * (re * re + im * im + 1e-30).log10()
}

#[test]
fn the_ringing_tines_follow_the_sympathetic_control() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let m = inst.notes[85 - 21].spec.string.modes.as_ref().unwrap();
    assert!(m.halo_from > 0 && m.halo_from < m.ratio.len(), "the halo from {} of {}", m.halo_from, m.ratio.len());
    let run = |sympathetic: f32| {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, sympathetic, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 85, speed: 1.0 });
        render(&mut e, SR as usize)
    };
    let (on, half, off) = (run(1.0), run(0.5), run(0.0));
    // C♯6's fundamental alike; G3's tine (196 Hz), ringing along, at half: −6 dB, off: gone
    let (a, b) = ((0.05 * SR) as usize, (0.55 * SR) as usize);
    let f1 = (level(&on, a, b, pitch(85.0)) - level(&off, a, b, pitch(85.0))).abs();
    assert!(f1 < 0.5, "the fundamental moved {f1:.2} dB");
    let g = |x: &[f32]| level(x, a, b, pitch(55.0));
    assert!((g(&on) - g(&half) - 6.0).abs() < 1.0, "half: {:.1} dB", g(&on) - g(&half));
    assert!(g(&on) - g(&off) > 30.0, "off: {:.1} dB", g(&on) - g(&off));
}

#[test]
fn the_tines_rattle_between_the_lines() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let m = inst.membrane.expect("the rattle");
    assert!(m.power > 1.0 && !m.sympathetic, "{m:?}");
    // each tine its own: C♯4's hardly (−81 dB re its fundamental), E6's strongly (−53)
    let g = |k: usize| inst.notes[k - 21].membrane_gain;
    assert!(g(61) < 0.2 && g(88) > 3.0, "C♯4 {:.2}, E6 {:.2}", g(61), g(88));
    let run = |rattle: bool| {
        let mut inst = Instrument::load(DIR).unwrap();
        if !rattle {
            inst.membrane = None;
        }
        let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, knock: 0.0, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 63, speed: 1.0 });
        render(&mut e, SR as usize)
    };
    let (with, without) = (run(true), run(false));
    // between E♭4's lines (311 Hz: its 3rd and 4th harmonics' places, the halo's tines aside), the
    // noise over 0.15–0.3 s: the recording's lies ~30 dB over the engine's lines' skirts alone
    let (a, b) = ((0.15 * SR) as usize, (0.3 * SR) as usize);
    let band = |x: &[f32]| (1050..1200).step_by(10).map(|f| 10f64.powf(level(x, a, b, f as f64) / 10.0)).sum::<f64>();
    let gain = 10.0 * (band(&with) / band(&without)).log10();
    assert!(gain > 10.0, "the rattle adds {gain:.1} dB between the lines");
}

#[test]
fn a_pluck_rises_from_zero_at_every_ringing_tines_setting() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    // each mode from zero over 4 ms (export_bars `onset_s`; the tine stepped to 0.64 of its peak in
    // the first 0.2 ms, the recordings 0.02); the tine's own partners are not the ringing tines'
    // (`halo_from`): at any "Ringing tines" it starts from zero
    for sympathetic in [1.0f32, 0.5, 0.0] {
        for key in [43u8, 67, 90] {
            let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
            e.set_params(Params { room: 0.0, knock: 0.0, sympathetic, ..Params::default() });
            e.queue(0, Event::NoteOn { key, speed: 1.0 });
            let x = render(&mut e, (0.06 * SR) as usize);
            let peak = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
            let on = x.iter().position(|v| v.abs() > 1e-6 * peak).unwrap();
            let first = x[on..on + (0.0002 * SR) as usize].iter().fold(0.0f32, |a, v| a.max(v.abs()));
            assert!(first < 0.1 * peak, "ringing tines {sympathetic}, key {key}: {:.2} of the peak in the first 0.2 ms", first / peak);
        }
    }
}
