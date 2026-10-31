//! The bridge's mobility along the keyboard (`physics::Mobility`): a factor per key on the
//! vertical polarization's coupling below 500 Hz; the sympathetic strings follow, the horizontal
//! polarization does not (design 2026-10-20-bridge-mobility).
use pianoengine::physics::Mobility;
use pianoengine::{Instrument, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

fn grand_a(mobility: Option<Mobility>) -> Option<Instrument> {
    let mut inst = Instrument::load(format!("{ROOT}grand_a")).ok()?;
    inst.globals.board.mobility = mobility;
    inst.compile(SR, Tuning::default());
    Some(inst)
}

#[test]
fn mobility_scales_the_vertical_coupling() {
    let m = Mobility { anchors: vec![24.0, 36.0, 48.0], factors: vec![1.0, 4.0, 2.0], fc_hz: 500.0 };
    let (Some(off), Some(on)) = (grand_a(None), grand_a(Some(m.clone()))) else { return };
    for key in [15, 27, 39, 60] {
        let (a, b) = (&off.notes[key], &on.notes[key]);
        let note = a.spec.note as f64;
        assert!(!a.phys.sym.f.is_empty());
        for (j, &f) in a.phys.sym.f.iter().enumerate() {
            let want = m.factor(note, f);
            let got = b.phys.sym.kappa[j].re / a.phys.sym.kappa[j].re;
            assert!((got - want).abs() < 1e-9 * want, "key {key}, {f} Hz: {got} vs {want}");
        }
        // the horizontal polarization's modes as they were
        assert_eq!(a.phys.horiz.lam, b.phys.horiz.lam, "key {key}");
    }
}
