//! The roll's export files (`roll::files`): read, edited array by array and written back, an export
//! loads and plays as the original. Needs the exported instruments (skipped without them).

use dussek::roll::files::Export;
use dussek::{Engine, Event, Instrument, Tuning};
use std::path::{Path, PathBuf};

const A: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pe-roll-files-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// key 60 at 2 m/s, 1 s, both channels appended
fn render(inst: Instrument) -> Vec<f32> {
    let mut e = Engine::with_tuning(inst, 48000.0, Tuning::default()).expect("engine");
    e.queue(0, Event::NoteOn { key: 60, speed: 2.0 });
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    let mut out = Vec::new();
    for _ in 0..188 {
        e.process(&mut l, &mut r);
        out.extend_from_slice(&l);
        out.extend_from_slice(&r);
    }
    out
}

#[test]
fn a_written_export_renders_as_the_original() {
    if !Path::new(A).exists() {
        return;
    }
    let d = tmp("render");
    Export::read(Path::new(A)).unwrap().write(&d).unwrap();
    assert_eq!(render(Instrument::load(A).unwrap()), render(Instrument::load(&d).unwrap()));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_longer_array_is_appended_aligned() {
    if !Path::new(A).exists() {
        return;
    }
    let mut e = Export::read(Path::new(A)).unwrap();
    let old_len = e.data.len();
    let mut f = e.f64s("/board/f").unwrap();
    f.push(f[f.len() - 1] * 1.01);
    e.set_f64s("/board/f", &f).unwrap();
    let o = e.json.pointer("/board/f/o").unwrap().as_u64().unwrap() as usize;
    assert_eq!(o % 8, 0);
    assert!(o >= old_len);
    assert_eq!(e.f64s("/board/f").unwrap(), f);
    // the board's other arrays follow (the engine needs as many of each)
    for k in ["sigma", "phi"] {
        let mut v = e.f64s(&format!("/board/{k}")).unwrap();
        v.push(v[v.len() - 1]);
        e.set_f64s(&format!("/board/{k}"), &v).unwrap();
    }
    for k in ["r", "r_h"] {
        let v = e.f64s(&format!("/board/{k}")).unwrap();
        let n = v.len() / 2;
        let w: Vec<f64> = [&v[..n], &[v[n - 1]], &v[n..], &[v[2 * n - 1]]].concat();
        e.set_f64s(&format!("/board/{k}"), &w).unwrap();
    }
    let d = tmp("append");
    e.write(&d).unwrap();
    Instrument::load(&d).expect("the written export loads");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn responses_round_trip() {
    if !Path::new(A).exists() {
        return;
    }
    let orig = Export::read(Path::new(A)).unwrap();
    let mut e = orig.clone();
    let rs = e.responses().unwrap();
    let len = orig.json.pointer("/radiation/v/body/len").unwrap().as_u64().unwrap() as usize;
    assert_eq!(rs.len(), 4);
    assert!(rs.iter().all(|r| r.ir.len() == len));
    for r in &rs {
        e.set_response(r.pol, r.channel, &r.ir).unwrap();
    }
    assert_eq!(e.data, orig.data);
}

#[test]
fn an_export_builds_an_instrument() {
    if !Path::new(A).exists() {
        return;
    }
    let e = Export::read(Path::new(A)).unwrap();
    assert_eq!(render(e.instrument().unwrap()), render(Instrument::load(A).unwrap()));
}

#[test]
fn concurrent_writes_never_mix_their_files() {
    if !Path::new(A).exists() {
        return;
    }
    let a = Export::read(Path::new(A)).unwrap();
    let mut b = a.clone();
    let c = b.f64s("/correction/C_db").unwrap();
    b.set_f64s("/correction/C_db", &[c, vec![1.0; 4096]].concat()).unwrap();
    let d = tmp("concurrent");
    a.write(&d).unwrap();
    std::thread::scope(|s| {
        for e in [&a, &b] {
            let d = &d;
            s.spawn(move || {
                for _ in 0..30 {
                    e.write(d).unwrap();
                }
            });
        }
    });
    let got = std::fs::read(d.join("data.bin")).unwrap();
    assert!(got == a.data || got == b.data, "data.bin mixed from two writes ({} bytes)", got.len());
    let left = std::fs::read_dir(&d).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains(".tmp")).count();
    assert_eq!(left, 0);
    let _ = std::fs::remove_dir_all(&d);
}
