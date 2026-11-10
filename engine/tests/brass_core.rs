//! The Rust core against `rt.c`, on a synthetic note (`proto/brass/core_fixture.py`).
use dussek::brass::core::{Core, CoreParams, ModeBank, Ramp};
use dussek::brass::tables::Lips;
use serde_json::Value;

struct Fixture {
    doc: Value,
}

fn list(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

fn noise(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let i = i as f64;
            (0.015 * (i * 0.37).sin() * (i * 0.011).cos() + 0.01 * (i * i * 1e-5).sin()) as f32
        })
        .collect()
}

fn fixture() -> Fixture {
    Fixture { doc: serde_json::from_str(include_str!("fixtures/brass_core.json")).unwrap() }
}

fn run(f: &Fixture, scalar: bool) -> Vec<f64> {
    let d = &f.doc;
    let p = &d["params"];
    let params = CoreParams {
        width: p["width"].as_f64().unwrap(),
        y0: p["y0"].as_f64().unwrap(),
        soft: p["soft"].as_f64().unwrap(),
        rho: p["rho"].as_f64().unwrap(),
        nl: p["nl"].as_f64().unwrap(),
    };
    let mu = p["mu"].as_f64().unwrap();
    let mut bank = ModeBank::default();
    let (er, ei, rr, ri) = (list(&d["E"][0]), list(&d["E"][1]), list(&d["r"][0]), list(&d["r"][1]));
    bank.n = er.len();
    bank.er[..bank.n].copy_from_slice(&er);
    bank.ei[..bank.n].copy_from_slice(&ei);
    bank.rr[..bank.n].copy_from_slice(&rr);
    bank.ri[..bank.n].copy_from_slice(&ri);
    let lips = Lips { ratio: list(&d["lips"][0]), q: list(&d["lips"][1]), sign: list(&d["lips"][2]), comp: list(&d["lips"][3]) };
    let dec = list(&d["dec"]);
    let nz = noise(d["noise_n"].as_u64().unwrap() as usize);
    let filters: Vec<(Vec<f32>, Vec<f32>)> = d["filters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| (list(&x[0]).iter().map(|v| *v as f32).collect(), list(&x[1]).iter().map(|v| *v as f32).collect()))
        .collect();
    let mut core = Core::new();
    core.force_scalar = scalar;
    core.set_filters(&filters[0].0, std::sync::Arc::from(filters[0].1.clone()));
    let mut out = Vec::new();
    let mut block = [0.0; 64];
    for (b, c) in d["ctl"].as_array().unwrap().iter().enumerate() {
        if let Some(i) = d["change"].get(b.to_string()) {
            let i = i.as_u64().unwrap() as usize;
            core.set_filters(&filters[i].0, std::sync::Arc::from(filters[i].1.clone()));
        }
        let c = list(c);
        let ramp = Ramp { pm: (c[0], c[1]), lip: (c[2], c[3]), burst: (c[4], c[5]) };
        core.process(&ramp, c[6], mu, &bank, &lips, &params, &nz, &dec, &mut block);
        out.extend_from_slice(&block);
    }
    out
}

#[test]
fn the_core_is_rt_c() {
    let f = fixture();
    let got = run(&f, false);
    let want = list(&f.doc["out"]);
    assert_eq!(got.len(), want.len());
    let peak = want.iter().fold(0.0f64, |a, x| a.max(x.abs()));
    assert!(peak > 1.0, "the fixture's note sounds: peak {peak}");
    for (i, (a, b)) in got.iter().zip(&want).enumerate() {
        assert!((a - b).abs() <= 1e-6 * peak, "sample {i}: {a} against {b} (peak {peak})");
    }
}

#[test]
fn the_vector_path_is_the_scalar_path() {
    let f = fixture();
    let (a, b) = (run(&f, false), run(&f, true));
    let peak = b.iter().fold(0.0f64, |a, x| a.max(x.abs()));
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        assert!((x - y).abs() <= 1e-9 * peak, "sample {i}: {x} against {y}");
    }
}
