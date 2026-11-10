//! A brass export read back (`brass::Tables`): point `PE_BRASS_EXPORT` at a scratch export or a committed one.
use dussek::Instrument;
use std::path::PathBuf;

fn export() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("PE_BRASS_EXPORT")?);
    p.join("instrument.json").exists().then_some(p)
}

#[test]
fn tables_read_what_the_export_wrote() {
    let Some(dir) = export() else { return };
    let inst = Instrument::load(&dir).unwrap();
    let t = inst.brass.as_ref().expect("a brass block");
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("instrument.json")).unwrap()).unwrap();
    let b = &doc["brass"];
    assert_eq!(t.mutes[0], "open");
    assert_eq!(t.table.len(), b["table"].as_array().unwrap().len());
    let key = t.table[t.table.len() / 2];
    let n = t.note(0, key);
    let want = &b["mutes"]["open"]["notes"][key.to_string()];
    assert_eq!(n.f, want["f"].as_f64().unwrap());
    assert_eq!(n.settings[1].0, want["settings"]["mf"][0].as_f64().unwrap());
    assert_eq!(n.h[0].len(), want["h"]["pp"]["n"].as_u64().unwrap() as usize);
    assert_eq!(n.r.len(), want["r"]["n"].as_u64().unwrap() as usize);
    assert_eq!(n.modes[2].len(), want["modes"]["ff"]["sp"]["re"]["n"].as_u64().unwrap() as usize);
    assert!(n.modes[1].sp_im.iter().all(|x| x.is_finite()) && n.h[1].iter().any(|x| *x != 0.0));
    assert_eq!(t.dec.len(), 96);
}

#[test]
fn a_mute_the_instrument_lacks_is_an_error() {
    let Some(dir) = export() else { return };
    let inst = Instrument::load(&dir).unwrap();
    let t = inst.brass.as_ref().unwrap();
    assert!(t.mute_index("no such mute").is_err());
    assert_eq!(t.mute_index("open").unwrap(), 0);
}

#[test]
fn a_brass_export_has_no_notes() {
    // the brass block is the sound: no silent keys for the strings' machinery to idle on, and it plays
    let dir = export().unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/trumpet")));
    let Ok(inst) = Instrument::load(&dir) else { return };
    assert!(inst.notes.is_empty(), "{} notes", inst.notes.len());
    assert!(!inst.is_hammer_piano() && inst.registers() == 0);
    assert!(matches!(inst.body(), dussek::mic::Body::Bell(_)));
    let mut e = dussek::Engine::new(inst, 44100.0).unwrap();
    let speed = e.hammer_speed(67, 0.7);
    e.queue(0, dussek::Event::NoteOn { key: 67, speed });
    let (mut l, mut r) = (vec![0.0f32; 22050], vec![0.0f32; 22050]);
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        e.process(a, b);
    }
    let rms = (l.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / l.len() as f64).sqrt();
    assert!(rms > 1e-4, "rms {rms}");
}

#[test]
fn the_bell_is_posed_as_its_export_says() {
    // the tuba's `[pose]`: up and to the right, the ears under and left of the bell; its radius from the bell's mouth
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/tuba"));
    let Ok(inst) = Instrument::load(&dir) else { return };
    let dussek::mic::Body::Bell(pose) = inst.body() else { panic!("not a bell: {:?}", inst.body()) };
    assert_eq!((pose.azimuth, pose.elevation, pose.ears), (60, 75, [-20, 0, -40]));
    assert_eq!(f64::from(pose.radius_mm) / 1000.0, (inst.brass.as_ref().unwrap().bell_radius * 1000.0).round() / 1000.0);
}
