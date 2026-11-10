//! The brass attack's settings (`attack_rise_periods` with its knee `attack_rise_hz`, `attack_lean`, `attack_rise_from`): the pressure rising over periods of the note, so a
//! low note does not shock the bore into ringing louder than it then holds, from a share of it, so the lips move at once
//! rather than after a third of the rise; and no lean-in under the note's pressure, so a high note at a strong velocity
//! does not wait under its threshold. Needs the exported instruments (returns early without them).
use dussek::{Engine, Event, Instrument, Mix};

const SR: f64 = 44100.0;

fn load(name: &str) -> Option<Instrument> {
    let mut inst = Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/").to_string() + name).ok()?;
    inst.output_gain_db = 0.0;
    Some(inst)
}

/// One tongued note, dry at Recording, a second: the left output.
fn note(inst: Instrument, key: u8, vel: f64) -> Vec<f32> {
    let mut e = Engine::new(inst, SR).unwrap();
    e.set_mix(Mix { room: false, ..Mix::ALL });
    let speed = e.hammer_speed(key, vel);
    e.queue(0, Event::NoteOn { key, speed });
    let n = (1.0 * SR) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut done = 0;
    while done < n {
        let m = (n - done).min(512);
        e.process(&mut l[done..done + m], &mut r[done..done + m]);
        done += m;
    }
    l
}

/// The level over windows of `w` samples every `hop`, dB.
fn windows(x: &[f32], w: usize, hop: usize) -> Vec<f64> {
    (0..x.len() - w)
        .step_by(hop)
        .map(|i| 10.0 * (x[i..i + w].iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / w as f64 + 1e-24).log10())
        .collect()
}

/// One tongued note: the output's level over windows of two periods (at least 20 ms), every 5 ms, dB.
fn envelope(inst: Instrument, key: u8, vel: f64) -> Vec<f64> {
    let f = 440.0 * 2f64.powf((f64::from(key) - 69.0) / 12.0);
    windows(&note(inst, key, vel), ((2.0 / f).max(0.02) * SR) as usize, (0.005 * SR) as usize)
}

/// ms from the key until the output first comes within 30 dB of its sustain (0.5 to 0.9 s; 10 ms windows every 2 ms).
fn first_sound_ms(inst: Instrument, key: u8, vel: f64) -> f64 {
    let e = windows(&note(inst, key, vel), (0.01 * SR) as usize, (0.002 * SR) as usize);
    let sus = e[250..450].iter().sum::<f64>() / 200.0;
    2.0 * e.iter().position(|v| *v > sus - 30.0).unwrap() as f64
}

/// The attack's peak over the first 300 ms against the sustain (0.5 to 0.9 s), dB.
fn overshoot(env: &[f64]) -> f64 {
    let sus = env[100..180].iter().sum::<f64>() / 80.0;
    env[..60].iter().cloned().fold(f64::MIN, f64::max) - sus
}

/// The tuba's E1–A#1 at velocity 114: the attack over the sustain, mean, dB (the recordings: about 0).
fn low_overshoot(tuba: &Instrument) -> f64 {
    (28..=34).map(|k| overshoot(&envelope(tuba.clone(), k, 114.0))).sum::<f64>() / 7.0
}

/// ms from the key until the level stays within 6 dB of the sustain.
fn speaks_ms(env: &[f64]) -> f64 {
    let sus = env[100..180].iter().sum::<f64>() / 80.0;
    let i = (0..100).find(|&i| env[i..i + 10].iter().all(|v| *v > sus - 6.0)).unwrap_or(100);
    5.0 * i as f64
}

/// The export, its `brass` block given the settings (`None`: the three fields left out) and no `attack_rise_from`: a copy in
/// a temporary directory.
fn with(name: &str, settings: Option<(f64, f64, bool)>) -> Instrument {
    with_from(name, settings, None)
}

/// `with`, `attack_rise_from` given (`None`: left out).
fn with_from(name: &str, settings: Option<(f64, f64, bool)>, from: Option<f64>) -> Instrument {
    let src = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/")).join(name);
    let dir = std::env::temp_dir().join(format!("dussek-brass-attack-{name}-{settings:?}-{from:?}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(src.join("instrument.json")).unwrap()).unwrap();
    let brass = doc["brass"].as_object_mut().unwrap();
    for key in ["attack_rise_periods", "attack_rise_hz", "attack_lean", "attack_rise_from"] {
        brass.remove(key);
    }
    if let Some(f) = from {
        brass.insert("attack_rise_from".into(), f.into());
    }
    if let Some((periods, knee, lean)) = settings {
        brass.insert("attack_rise_periods".into(), periods.into());
        brass.insert("attack_rise_hz".into(), knee.into());
        brass.insert("attack_lean".into(), lean.into());
    }
    std::fs::write(dir.join("instrument.json"), serde_json::to_string(&doc).unwrap()).unwrap();
    std::fs::copy(src.join("data.bin"), dir.join("data.bin")).unwrap();
    let mut inst = Instrument::load(&dir).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    inst.output_gain_db = 0.0;
    inst
}

/// The settings off: the 8 ms rise and the lean-in.
const OFF: Option<(f64, f64, bool)> = Some((0.0, 0.0, true));

#[test]
fn a_low_note_rising_over_its_periods_does_not_overshoot() {
    let Some(_) = load("tuba") else { return };
    let off = low_overshoot(&with("tuba", OFF));
    let slow = low_overshoot(&with("tuba", Some((10.0, 0.0, true))));
    assert!(off > 6.0, "E1–A#1, the 8 ms rise: {off:.1} dB over the sustain");
    assert!(slow < 4.0, "E1–A#1, ten periods: {slow:.1} dB over the sustain (the recordings: about 0)");
}

/// Over the knee the rise blends back to 8 ms within an octave: a staccato in the trombone's middle speaks as before, the
/// low notes keep their slow rise (ten periods everywhere took a G3 at 62 from 30 to 65 ms).
#[test]
fn over_the_knee_a_note_rises_as_before() {
    let Some(_) = load("tenor_trombone") else { return };
    let off = speaks_ms(&envelope(with("tenor_trombone", OFF), 55, 62.0));
    let everywhere = speaks_ms(&envelope(with("tenor_trombone", Some((10.0, 0.0, true))), 55, 62.0));
    let knee = speaks_ms(&envelope(with("tenor_trombone", Some((10.0, 100.0, true))), 55, 62.0));
    assert!(everywhere >= off + 15.0, "G3 at 62: {off} ms at 8 ms, {everywhere} ms with ten periods");
    assert!(knee <= off + 5.0, "G3 at 62: {off} ms at 8 ms, {knee} ms with the knee at 100 Hz");
    let Some(_) = load("tuba") else { return };
    let low = low_overshoot(&with("tuba", Some((10.0, 100.0, true))));
    assert!(low < 4.0, "the tuba's E1–A#1 under the knee: {low:.1} dB over the sustain");
}

/// From a fifth of the pressure the tuba's low notes sound at once (from none, the air reached a quarter of the rise's top
/// only after a third of it: 80 ms at E1), and still do not overshoot.
#[test]
fn a_rise_from_a_share_speaks_sooner() {
    let Some(_) = load("tuba") else { return };
    let slow = Some((10.0, 100.0, false));
    let first = |from| (28..=34).map(|k| first_sound_ms(with_from("tuba", slow, from), k, 114.0)).sum::<f64>() / 7.0;
    let (none, share) = (first(None), first(Some(0.2)));
    assert!(share + 25.0 <= none, "E1–A#1 at 114, ms to the first sound: {none:.0} from none, {share:.0} from a fifth");
    let over = low_overshoot(&with_from("tuba", slow, Some(0.2)));
    assert!(over < 4.0, "E1–A#1 at 114 from a fifth: {over:.1} dB over the sustain");
}

#[test]
fn a_high_note_without_the_lean_speaks_sooner() {
    let Some(_) = load("trumpet") else { return };
    let lean = speaks_ms(&envelope(with("trumpet", OFF), 72, 81.0));
    let none = speaks_ms(&envelope(with("trumpet", Some((0.0, 0.0, false))), 72, 81.0));
    assert!(none + 10.0 <= lean, "C5 at 81: {lean} ms with the lean, {none} ms without");
}

/// An export without the fields plays as one with them written off.
#[test]
fn the_settings_left_out_are_off() {
    let Some(_) = load("trumpet") else { return };
    let t = with("trumpet", None);
    let b = t.brass.as_ref().unwrap();
    assert_eq!((b.attack_rise_periods, b.attack_rise_hz, b.attack_lean, b.attack_rise_from), (0.0, 0.0, true, 0.0));
    assert_eq!(envelope(t, 72, 81.0), envelope(with("trumpet", OFF), 72, 81.0));
    let tuba = || with("tuba", Some((10.0, 100.0, false)));
    assert_eq!(note(tuba(), 30, 114.0), note(with_from("tuba", Some((10.0, 100.0, false)), Some(0.0)), 30, 114.0));
}

/// The projects' values as exported: the low brass rise over their periods, under the knee, from a fifth of the pressure,
/// the tuba's low note no longer overshoots and sounds at once; the trumpet keeps its 8 ms; all start a note on a release
/// from rest.
#[test]
fn the_exports_carry_the_projects_values() {
    let Some(tuba) = load("tuba") else { return };
    let b = tuba.brass.as_ref().unwrap();
    assert_eq!((b.attack_rise_periods, b.attack_rise_hz, b.attack_lean, b.attack_rise_from, b.attack_from_rest), (10.0, 100.0, false, 0.2, true));
    let over = low_overshoot(&tuba);
    assert!(over < 4.0, "the tuba's E1–A#1 as exported: {over:.1} dB over the sustain");
    let first = (28..=34).map(|k| first_sound_ms(tuba.clone(), k, 114.0)).sum::<f64>() / 7.0;
    assert!(first < 20.0, "the tuba's E1–A#1 as exported: the first sound after {first:.0} ms");
    let Some(trumpet) = load("trumpet") else { return };
    let b = trumpet.brass.as_ref().unwrap();
    assert_eq!((b.attack_rise_periods, b.attack_lean, b.attack_rise_from, b.attack_from_rest), (0.0, false, 0.0, true));
    for name in ["horn", "tenor_trombone", "bass_trombone"] {
        let b = load(name).unwrap().brass.unwrap();
        assert_eq!((b.attack_rise_from, b.attack_from_rest), (0.2, true), "{name}");
    }
}
