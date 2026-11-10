//! Air added (`air_added`): the air over what the keys or Air give, up to the top over its travel; the plugin and midi2wav
//! tie the mod wheel (CC 1) to it, and the mod wheel's event itself is the clavichord's (the Bebung), not the brass'.
//! Needs the exported instruments (returns early without them).
use dussek::{Engine, Event, Instrument, Mix};

const SR: f64 = 44100.0;

fn load(name: &str) -> Option<Instrument> {
    let mut inst = Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/").to_string() + name).ok()?;
    inst.output_gain_db = 0.0;
    Some(inst)
}

/// A soft note held a second, dry at Recording, Air added at `added` (0 … 1), the events before it: the output.
fn render(inst: Instrument, added: f32, events: &[Event]) -> Vec<f32> {
    let mut e = Engine::new(inst, SR).unwrap();
    e.set_mix(Mix { room: false, ..Mix::ALL });
    let mut p = e.params();
    p.brass.air_added = added;
    e.set_params(p);
    for ev in events {
        e.queue(0, *ev);
    }
    let speed = e.hammer_speed(67, 40.0);
    e.queue(0, Event::NoteOn { key: 67, speed });
    let n = SR as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut done = 0;
    while done < n {
        let m = (n - done).min(512);
        e.process(&mut l[done..done + m], &mut r[done..done + m]);
        done += m;
    }
    l
}

fn db(x: &[f32]) -> f64 {
    let s = &x[x.len() / 2..];
    10.0 * (s.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / s.len() as f64 + 1e-24).log10()
}

#[test]
fn air_added_takes_a_soft_note_up() {
    let Some(trumpet) = load("trumpet") else { return };
    let (none, half, all) = (db(&render(trumpet.clone(), 0.0, &[])), db(&render(trumpet.clone(), 0.5, &[])), db(&render(trumpet, 1.0, &[])));
    assert!(half > none + 2.0, "G4 at 40: {none:.1} dB, Air added 50 % {half:.1} dB");
    assert!(all > half + 2.0, "G4 at 40: Air added 50 % {half:.1} dB, 100 % {all:.1} dB");
}

#[test]
fn the_mod_wheel_is_not_the_brass_air() {
    let Some(trumpet) = load("trumpet") else { return };
    let wheel = [Event::ModWheel { value: 1.0 }];
    assert_eq!(render(trumpet.clone(), 0.0, &wheel), render(trumpet, 0.0, &[]));
}
