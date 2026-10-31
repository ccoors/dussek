//! The roll's basis (`roll::basis`): the references' bodies' main directions, oriented so the
//! first is warmth (+ warm) and the second air (+ airy); each reference is exactly its knobs.
//! Needs the exported instruments (skipped without them).

use dussek::roll::bands::grid;
use dussek::roll::basis::Basis;
use dussek::roll::body::{self, Body};
use dussek::roll::files::Export;
use std::path::Path;
use std::sync::OnceLock;

const REFS: [&str; 10] = ["grand_a", "grand_b", "grand_f", "grand_e", "grand_d", "grand_c", "grand_g", "upright_a", "upright_b", "upright_c"];

fn bodies() -> Option<&'static Vec<Body>> {
    static B: OnceLock<Option<Vec<Body>>> = OnceLock::new();
    B.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments");
        REFS.iter().map(|r| Export::read(&dir.join(r)).ok().map(|e| body::describe(&e).expect("describe"))).collect()
    })
    .as_ref()
}

#[test]
fn each_reference_is_its_knobs() {
    let Some(b) = bodies() else { return };
    let basis = Basis::new(b);
    for (name, body) in REFS.iter().zip(b) {
        let w = body.weighted();
        let back = basis.body(&basis.knobs(body)).weighted();
        let norm = w.iter().map(|v| v * v).sum::<f64>().sqrt();
        let err = w.iter().zip(&back).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt();
        assert!(err < 1e-9 * norm, "{name}: {err}");
    }
}

#[test]
fn ten_references_give_nine_directions() {
    let Some(b) = bodies() else { return };
    let basis = Basis::new(b);
    assert_eq!(basis.directions.len(), 9);
    assert_eq!(basis.sd.len(), 9);
    assert!(basis.sd.windows(2).all(|w| w[0] >= w[1]));
}

#[test]
fn n_references_give_at_most_n_minus_one_directions() {
    // the mean taken out leaves n − 1 at most: the zero direction's rounding is not one (it read
    // 1e-9 on nine of the refitted pianos, over an absolute threshold)
    let Some(b) = bodies() else { return };
    for leave in 0..b.len() {
        let some: Vec<Body> = b.iter().enumerate().filter(|&(i, _)| i != leave).map(|(_, x)| x.clone()).collect();
        let basis = Basis::new(&some);
        assert!(basis.directions.len() < some.len(), "without {}: {} directions", REFS[leave], basis.directions.len());
    }
}

#[test]
fn the_directions_are_oriented() {
    let Some(b) = bodies() else { return };
    let basis = Basis::new(b);
    let g = grid();
    // warmth: + raises the level at 63 Hz
    assert!(Body::unweighted(&basis.directions[0]).level[10] > 0.0);
    // air: + lowers the decay rate above 1 kHz
    let air = Body::unweighted(&basis.directions[1]);
    let hi: Vec<f64> = (0..61).filter(|&k| g[k] > 1000.0).map(|k| air.rate[k]).collect();
    assert!(hi.iter().sum::<f64>() < 0.0);
    // each further one: its largest weight positive
    for d in &basis.directions[2..] {
        let big = d.iter().fold(0.0f64, |m, &v| if v.abs() > m.abs() { v } else { m });
        assert!(big > 0.0);
    }
    let total: f64 = basis.sd.iter().map(|s| s * s).sum();
    let share: Vec<f64> = basis.sd.iter().map(|s| 100.0 * s * s / total).collect();
    // the references' spread per direction (Grand G's without its low hump since step 181: the first 60 → 54 %)
    for (got, want) in share.iter().zip([54.0, 16.0, 10.0, 7.0]) {
        assert!((got - want).abs() <= 5.0, "shares {:?}", &share[..4]);
    }
}

#[test]
fn grand_e_is_cold_and_upright_b_dry() {
    let Some(b) = bodies() else { return };
    let basis = Basis::new(b);
    let e = basis.knobs(&b[4]);
    let ub = basis.knobs(&b[8]);
    assert!(e[0] < -1.5, "Grand D's warmth {:.2}", e[0]);
    assert!(ub[1] < -1.0, "Upright B's air {:.2}", ub[1]);
}
