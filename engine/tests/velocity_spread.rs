//! The velocity curve against a file whose notes all sit near one velocity: a steep curve over
//! that range spreads their loudness (the editor's fix for a file that plays too evenly).
use pianoengine::velocity::VelocityCurve;
use pianoengine::{Engine, Event, Instrument};

const SR: f64 = 48000.0;

/// note 60's peak level (dB) struck at the hammer speed the curve gives `velocity`
fn peak_db(curve: &VelocityCurve, velocity: u8) -> Option<f64> {
    let inst = Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a")).ok()?;
    let mut e = Engine::new(inst, SR).ok()?;
    let speed = e.hammer_speed(60, curve.apply(velocity));
    e.queue(0, Event::NoteOn { key: 60, speed });
    let (mut l, mut r) = (vec![0.0; 24000], vec![0.0; 24000]);
    e.process(&mut l, &mut r);
    Some(20.0 * (l.iter().fold(0.0f32, |m, x| m.max(x.abs())) as f64).log10())
}

#[test]
fn a_steep_middle_spreads_a_files_loudness() {
    let steep = VelocityCurve::from_points(&[(1, 1), (55, 30), (75, 100), (127, 127)]).unwrap();
    let range = |c: &VelocityCurve| -> Option<f64> {
        let v: Vec<f64> = [60u8, 64, 68, 72].iter().map(|&x| peak_db(c, x)).collect::<Option<_>>()?;
        Some(v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min))
    };
    let (Some(flat), Some(spread)) = (range(&VelocityCurve::STRAIGHT), range(&steep)) else { return };
    eprintln!("velocities 60–72: straight {flat:.1} dB, steep {spread:.1} dB");
    assert!(spread >= 2.0 * flat, "straight {flat:.1} dB, steep {spread:.1} dB");
}
