//! A stiff hammer felt (hardness, wear) on a light top-key hammer made the explicit contact
//! step diverge to inf/NaN, which poisoned the convolvers for good (their FFT then failed
//! every block). The contact must stay finite at the extremes of both settings. Needs the
//! exported instruments (skipped without them).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

/// All keys struck at once at `speed`; true when every output sample stays finite.
fn finite(params: Params, speed: f64) -> Option<bool> {
    let inst = Instrument::load(DIR).ok()?;
    let mut e = Engine::with_tuning(inst, 48000.0, Tuning::default()).expect("engine");
    e.set_params(params);
    for key in 21..=108 {
        e.queue(0, Event::NoteOn { key, speed });
    }
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    for _ in 0..60 {
        e.process(&mut l, &mut r);
        if l.iter().chain(&r).any(|x| !x.is_finite()) {
            return Some(false);
        }
    }
    Some(true)
}

#[test]
fn stiff_felt_stays_finite() {
    let base = Params::default();
    for params in [Params { hardness: 20.0, ..base }, Params { hammer_wear: 1.0, ..base }, Params { hardness: 20.0, hammer_wear: 1.0, voicing_wear: 1.0, ..base }] {
        for speed in [3.0, 5.0, 12.0] {
            if let Some(ok) = finite(params, speed) {
                assert!(ok, "non-finite output at {params:?}, speed {speed}");
            }
        }
    }
}

/// with sub-steps +1 … +3 dB, without +5 … +15 dB (Salamander)
const LIMIT_DB: f32 = 6.0;

/// Peak output of keys struck one at a time at `speed` (None without the instrument).
fn peaks(params: Params, keys: &[u8], speed: f64) -> Option<Vec<f32>> {
    let inst = Instrument::load(DIR).ok()?;
    let mut out = Vec::new();
    for &key in keys {
        let mut e = Engine::with_tuning(inst.clone(), 48000.0, Tuning::default()).expect("engine");
        e.set_params(params);
        e.queue(0, Event::NoteOn { key, speed });
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let mut peak = 0.0f32;
        for _ in 0..40 {
            e.process(&mut l, &mut r);
            peak = l.iter().chain(&r).fold(peak, |m, x| m.max(x.abs()));
        }
        out.push(peak);
    }
    Some(out)
}

/// Below divergence the explicit contact step still grew without bound: at the slider's
/// hardest felt (+12 dB, four times the stiffness) the top keys peaked at up to 8× the
/// force of their neighbors; sub-steps keep the contact stable
#[test]
fn stiff_felt_stays_bounded() {
    let keys = [96, 100, 103, 106];
    let base = Params::default();
    let (Some(soft), Some(hard)) = (peaks(base, &keys, 7.0), peaks(Params { hardness: 4.0, ..base }, &keys, 7.0)) else {
        return;
    };
    for ((k, s), h) in keys.iter().zip(&soft).zip(&hard) {
        let db = 20.0 * (h / s).log10();
        assert!(db < LIMIT_DB, "key {k}: the hardest felt made it {db:.1} dB louder");
    }
}
