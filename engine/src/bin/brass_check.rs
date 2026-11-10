//! A checker, no tool for players: the brass players against what `proto/brass/port_check.py` wrote.
//! `brass_check <folder>`: replays the phrase with Python's random numbers, exit status non-zero over -80 dB.
//! `--own`: with the engine's own, writes rust.bin. `--hold`: `rt_stability`'s grid, one player, writes hold.bin.
use dussek::Instrument;
use dussek::brass::player::{DT, Live};
use dussek::brass::section::{Ev, Section};
use dussek::brass::shape::{DrawSource, SeededDraws, ShapeDraws};
use dussek::brass::streams::{CTRL, Streams};
use dussek::brass::tables::Tables;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

struct Recorded(Vec<f64>);

impl DrawSource for Recorded {
    fn draw(&mut self, i: u32) -> ShapeDraws {
        let r = &self.0[i as usize * 6..i as usize * 6 + 6];
        ShapeDraws { level: r[0], on: r[1], depth: r[2], rate: r[3], off: r[4], scoop: r[5] }
    }
}

fn f64s(p: &Path) -> Vec<f64> {
    std::fs::read(p).unwrap().as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect()
}

fn f32s(p: &Path) -> Vec<f32> {
    std::fs::read(p).unwrap().as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()
}

fn write(p: &Path, x: &[f64]) {
    let mut b = Vec::with_capacity(x.len() * 8);
    x.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
    std::fs::write(p, b).unwrap();
}

fn tables_at(meta: &Value) -> Arc<Tables> {
    let inst = Instrument::load(meta["export"].as_str().unwrap()).unwrap();
    inst.brass.clone().expect("a brass export")
}

fn cc(s: &mut Section, c: i64, v: f64) {
    let e = match c {
        1 => Ev::Wheel(v),
        2 | 11 => Ev::Air(v),
        70 => Ev::Vibrato(v / 127.0),
        21 => Ev::Growl(v / 127.0),
        23 => Ev::Flutter(v / 127.0),
        65 => Ev::Glide(v >= 64.0),
        5 => Ev::GlideTime(0.04 * 30f64.powf(v / 127.0)),
        4 => Ev::Plunger(v / 127.0),
        120 | 123 => Ev::AllOff,
        _ => return,
    };
    s.post(e);
}

fn post(s: &mut Section, e: &[Value]) {
    match e[1].as_str().unwrap() {
        "on" => s.post(Ev::On(e[2].as_u64().unwrap() as u8, e[3].as_f64().unwrap())),
        "off" => s.post(Ev::Off(e[2].as_u64().unwrap() as u8)),
        "pb" => s.post(Ev::Bend(e[2].as_f64().unwrap())),
        "at" => s.post(Ev::Aftertouch(e[2].as_f64().unwrap())),
        "cc" => cc(s, e[2].as_i64().unwrap(), e[3].as_f64().unwrap()),
        "mode" => s.post(Ev::Solo(e[2].as_str().unwrap() == "solo")),
        k => panic!("event {k}"),
    }
}

fn play(mut s: Section, meta: &Value) -> Vec<f64> {
    let steps = meta["steps"].as_u64().unwrap() as usize;
    let ev = meta["events"].as_array().unwrap();
    let mut j = 0;
    let mut out = Vec::with_capacity(steps * CTRL * 2);
    let mut mix = [[0.0; CTRL]; 2];
    for k in 0..steps {
        while j < ev.len() && ev[j][0].as_u64().unwrap() as usize <= k {
            post(&mut s, ev[j].as_array().unwrap());
            j += 1;
        }
        s.step(&mut mix);
        for i in 0..CTRL {
            out.push(mix[0][i]);
            out.push(mix[1][i]);
        }
    }
    eprintln!("resets {}", s.resets);
    out
}

fn replay(dir: &Path, meta: &Value) -> i32 {
    let t = tables_at(meta);
    let mute = t.mute_index(meta["mute"].as_str().unwrap()).unwrap();
    let mut voices = Vec::new();
    for i in 0..meta["voices"].as_u64().unwrap() as usize {
        let st = Streams {
            noise: f32s(&dir.join(format!("noise_{i}.bin"))).into(),
            wp: f64s(&dir.join(format!("wp_{i}.bin"))).into(),
            wl: f64s(&dir.join(format!("wl_{i}.bin"))).into(),
            band: f64s(&dir.join(format!("band_{i}.bin"))).into(),
            jit: f64s(&dir.join(format!("jit_{i}.bin"))).into(),
        };
        let d = Recorded(f64s(&dir.join(format!("draws_{i}.bin"))));
        voices.push(Live::new(t.clone(), st, Box::new(d), mute, true, 0));
    }
    let got = play(Section::new(t, voices), meta);
    let want = f64s(&dir.join("out.bin"));
    assert_eq!(got.len(), want.len());
    write(&dir.join("rust.bin"), &got);
    let e: f64 = got.iter().zip(&want).map(|(a, b)| (a - b) * (a - b)).sum();
    let p: f64 = want.iter().map(|b| b * b).sum();
    let db = 10.0 * (e / p.max(1e-30)).log10();
    let mut worst = (0usize, 0.0f64);
    for (i, (a, b)) in got.iter().zip(&want).enumerate() {
        if (a - b).abs() > worst.1 {
            worst = (i, (a - b).abs());
        }
    }
    println!("residual {db:.1} dB over the render; worst sample at {:.3} s: {:.2e}", worst.0 as f64 / 2.0 * DT / CTRL as f64, worst.1);
    i32::from(db > -80.0)
}

fn own(dir: &Path, meta: &Value) -> i32 {
    let t = tables_at(meta);
    let mute = t.mute_index(meta["mute"].as_str().unwrap()).unwrap();
    let st = Streams::generate(&t, 1);
    let voices = (0..meta["voices"].as_u64().unwrap() as usize)
        .map(|i| Live::new(t.clone(), st.clone(), Box::new(SeededDraws { seed: 1 + 100 * i as u64 }), mute, true, i * 6007))
        .collect();
    write(&dir.join("rust.bin"), &play(Section::new(t, voices), meta));
    0
}

/// `rt_stability.one`: the air set, a held note, one player.
fn hold(dir: &Path) -> i32 {
    let meta: Value = serde_json::from_slice(&std::fs::read(dir.join("hold.json")).unwrap()).unwrap();
    let t = tables_at(&meta);
    let mute = t.mute_index(meta["mute"].as_str().unwrap()).unwrap();
    let st = Streams::generate(&t, 1);
    let cases: Vec<(u8, f64, f64)> = meta["cases"].as_array().unwrap().iter().map(|c| (c[0].as_u64().unwrap() as u8, c[1].as_f64().unwrap(), c[2].as_f64().unwrap())).collect();
    let steps = (1.6 / DT) as usize;
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunks: Vec<Vec<(usize, (u8, f64, f64))>> = (0..threads).map(|n| cases.iter().copied().enumerate().filter(|(i, _)| i % threads == n).collect()).collect();
    let mut out = vec![0.0; cases.len() * steps * CTRL];
    let parts: Vec<Vec<(usize, Vec<f64>)>> = std::thread::scope(|sc| {
        let hs: Vec<_> = chunks
            .into_iter()
            .map(|ch| {
                let (t, st) = (t.clone(), st.clone());
                sc.spawn(move || {
                    let mut rows = Vec::new();
                    for (i, (key, air, vel)) in ch {
                        let mut v = Live::new(t.clone(), st.clone(), Box::new(SeededDraws { seed: 1 }), mute, false, 0);
                        v.pedal = Some(air);
                        let mut y = vec![0.0; steps * CTRL];
                        let (on, off) = ((0.05 / DT).ceil() as usize, (1.45 / DT).ceil() as usize);
                        let mut b = [0.0; CTRL];
                        for k in 0..steps {
                            if k == on {
                                v.note_on(key, vel);
                            }
                            if k == off {
                                v.note_off(key);
                            }
                            v.step(&mut b);
                            y[k * CTRL..(k + 1) * CTRL].copy_from_slice(&b);
                        }
                        rows.push((i, y));
                    }
                    rows
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for rows in parts {
        for (i, y) in rows {
            out[i * steps * CTRL..(i + 1) * steps * CTRL].copy_from_slice(&y);
        }
    }
    write(&dir.join("hold.bin"), &out);
    0
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = Path::new(&a[1]);
    if a.get(2).is_some_and(|x| x == "--hold") {
        std::process::exit(hold(dir));
    }
    let meta: Value = serde_json::from_slice(&std::fs::read(dir.join("meta.json")).unwrap()).unwrap();
    let code = if a.get(2).is_some_and(|x| x == "--own") { own(dir, &meta) } else { replay(dir, &meta) };
    std::process::exit(code);
}
