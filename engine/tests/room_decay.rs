//! The diffuse room (`noise::room_ir`): its decay per band and its own level (`eq_db`), a cut
//! taking energy away.

use pianoengine::instrument::RoomSpec;
use pianoengine::noise::room_ir;

const SR: f64 = 48000.0;

fn spec(eq_db: Vec<[f64; 2]>) -> RoomSpec {
    RoomSpec {
        t60: vec![[100.0, 2.0], [10000.0, 2.0]],
        grid_hz: vec![20.0, 20000.0],
        h_db: vec![0.0, 0.0],
        length_s: 2.0,
        seeds: [11, 21],
        eq_db,
    }
}


/// Energy (dB) of the band [lo, hi) in the window [t, t + 0.05) s, both channels.
fn band_db(ir: &[Vec<f64>; 2], lo: f64, hi: f64, t: f64) -> f64 {
    let n = 2048;
    let i0 = (t * SR) as usize;
    let mut e = 0.0;
    for c in ir {
        for k in 0..(0.05 * SR) as usize / 256 {
            let s = i0 + k * 256;
            let frame: Vec<f64> = (0..n).map(|i| c.get(s + i).copied().unwrap_or(0.0) * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())).collect();
            for (j, f) in (0..n / 2).map(|j| (j, j as f64 * SR / n as f64)) {
                if f >= lo && f < hi {
                    let (re, im) = frame.iter().enumerate().fold((0.0, 0.0), |(re, im), (i, &v)| {
                        let a = -2.0 * std::f64::consts::PI * (i * j) as f64 / n as f64;
                        (re + v * a.cos(), im + v * a.sin())
                    });
                    e += re * re + im * im;
                }
            }
        }
    }
    10.0 * e.max(1e-300).log10()
}

#[test]
fn the_room_decays_at_its_t60() {
    let ir = room_ir(11, &spec(vec![]), 0.12, 1.0, SR);
    let rate = (band_db(&ir, 900.0, 1100.0, 0.2) - band_db(&ir, 900.0, 1100.0, 1.0)) / 0.8;
    assert!((rate - 30.0).abs() < 5.0, "{rate:.1} dB/s, T60 2 s is 30 dB/s");
}

#[test]
fn eq_db_tilts_the_room() {
    let flat = room_ir(11, &spec(vec![]), 0.12, 1.0, SR);
    let tilt = room_ir(11, &spec(vec![[500.0, 10.0], [4000.0, -10.0]]), 0.12, 1.0, SR);
    let ratio = |ir: &[Vec<f64>; 2]| band_db(ir, 3800.0, 4200.0, 0.1) - band_db(ir, 480.0, 520.0, 0.1);
    let d = ratio(&tilt) - ratio(&flat);
    assert!((d + 20.0).abs() < 3.0, "4 kHz re 500 Hz moved {d:.1} dB, not −20");
}



#[test]
fn a_cut_takes_energy_away() {
    // eq_db is on top of the room's unit energy: a uniform −10 dB is −10 dB, not renormalized away
    let flat = room_ir(11, &spec(vec![]), 0.12, 1.0, SR);
    let cut = room_ir(11, &spec(vec![[100.0, -10.0], [10000.0, -10.0]]), 0.12, 1.0, SR);
    let e = |ir: &[Vec<f64>; 2]| ir.iter().map(|c| c.iter().map(|v| v * v).sum::<f64>()).sum::<f64>();
    let d = 10.0 * (e(&cut) / e(&flat)).log10();
    assert!((d + 10.0).abs() < 0.2, "{d:+.2} dB");
}

/// Level (dB) of key 60 at `mic`, the room off (the board alone), over 0.5 s.
fn body_db(cut: bool, mic: pianoengine::mic::MicPosition) -> Option<f64> {
    use pianoengine::{Engine, Event, Instrument, Params, Tuning};
    let src = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_f"));
    let dir = if cut {
        src.to_path_buf()
    } else {
        // a copy whose export has no cuts
        let d = std::env::temp_dir().join(format!("pe-room-uncut-{}", std::process::id()));
        std::fs::create_dir_all(&d).ok()?;
        std::fs::copy(src.join("data.bin"), d.join("data.bin")).ok()?;
        std::fs::copy(src.join("level.json"), d.join("level.json")).ok()?; // the same speeds
        let mut j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(src.join("instrument.json")).ok()?).ok()?;
        j["radiation"]["room"].as_object_mut()?.remove("eq_db");
        std::fs::write(d.join("instrument.json"), j.to_string()).ok()?;
        d
    };
    let inst = Instrument::load(&dir).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning { mic, ..Tuning::default() }).unwrap();
    e.set_params(Params { room: 0.0, ..Params::default() });
    let speed = e.hammer_speed(60, 80.0);
    e.queue(0, Event::NoteOn { key: 60, speed });
    let n = (0.5 * SR) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    Some(10.0 * (l.iter().chain(&r).map(|v| (*v as f64).powi(2)).sum::<f64>()).log10())
}

#[test]
fn the_room_s_cuts_leave_the_board_at_the_other_positions() {
    // the microphone positions match their level to the recording's total: the room as it was
    // before its cuts (they change the room, not the board), so the board's Player against its
    // Recording stays
    use pianoengine::mic::MicPosition::{Player, Recording};
    let Some(with) = body_db(true, Player) else { return };
    let with = with - body_db(true, Recording).unwrap();
    let without = body_db(false, Player).unwrap() - body_db(false, Recording).unwrap();
    assert!((with - without).abs() < 0.3, "the board's Player re Recording: {:+.2} dB with the cuts", with - without);
}
