//! The clavichord (`proto/scale.py` TANGENT): the tangent's strike (the pluck path's `tangent`) and
//! the Bebung: a held key's pitch rises with the pressure (the mod wheel, channel or key pressure,
//! whichever is highest), by `Params::bebung` cents at full. Needs the exported instrument (skipped
//! without).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/clavichord");
const HARPSICHORD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/harpsichord");
const SR: f64 = 48000.0;

fn engine(dir: &str) -> Option<Engine> {
    let inst = Instrument::load(dir).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, knock: 0.0, ..Params::default() });
    Some(e)
}

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

/// The frequency of the strongest component within ±80 cents of `f` (Hann-windowed DFT on a
/// 0.1-cent grid), in cents re `f`.
fn peak_cents(x: &[f32], f: f64) -> f64 {
    let n = x.len();
    let w: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos()).collect();
    let mag = |c: f64| {
        let om = 2.0 * std::f64::consts::PI * f * 2f64.powf(c / 1200.0) / SR;
        let (mut re, mut im) = (0.0, 0.0);
        for i in 0..n {
            let v = x[i] as f64 * w[i];
            re += v * (om * i as f64).cos();
            im -= v * (om * i as f64).sin();
        }
        re * re + im * im
    };
    let coarse = (-80..=80).map(|c| c as f64).max_by(|a, b| mag(*a).total_cmp(&mag(*b))).unwrap();
    (-10..=10).map(|d| coarse + d as f64 * 0.1).max_by(|a, b| mag(*a).total_cmp(&mag(*b))).unwrap()
}

fn f0(dir: &str, key: u8) -> f64 {
    Instrument::load(dir).unwrap().notes[key as usize - 21].spec.string.f0
}

#[test]
fn the_tangent_strikes_without_a_comb() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let i = 60 - 21;
    match &inst.notes[i].spec.excitation {
        dussek::physics::Excitation::Pluck(p) => assert!(p.tangent),
        _ => panic!("the clavichord's key is not a tangent"),
    }
    assert!(inst.tangent && inst.notes[i].spec.string.detune_cents.len() == 2, "double strung, with tangents");
}

#[test]
fn the_mod_wheel_raises_a_held_note_by_the_bebung() {
    let Some(_) = engine(DIR) else { return };
    let f = f0(DIR, 69);
    let pitch = |wheel: Option<f32>, bebung: f32| -> f64 {
        let mut e = engine(DIR).unwrap();
        e.set_params(Params { room: 0.0, knock: 0.0, bebung, ..Params::default() });
        if let Some(v) = wheel {
            e.queue(0, Event::ModWheel { value: v });
        }
        e.queue(0, Event::NoteOn { key: 69, speed: 1.0 });
        let x = render(&mut e, 24000);
        assert!(x.iter().all(|v| v.is_finite()));
        peak_cents(&x[2400..14400], f)
    };
    let base = pitch(None, 30.0);
    let full = pitch(Some(1.0), 30.0) - base;
    let half = pitch(Some(0.5), 30.0) - base;
    assert!((full - 30.0).abs() < 2.0, "full wheel: {full:+.1} cents");
    assert!((half - 15.0).abs() < 2.0, "half wheel: {half:+.1} cents");
    // no depth: no Bebung
    let none = pitch(Some(1.0), 0.0) - base;
    assert!(none.abs() < 0.5, "depth 0: {none:+.1} cents");
}

#[test]
fn the_pitch_follows_the_pressure_while_the_note_sounds() {
    let Some(mut e) = engine(DIR) else { return };
    let f = f0(DIR, 69);
    e.queue(0, Event::NoteOn { key: 69, speed: 1.0 });
    e.queue(12000, Event::ChannelPressure { value: 1.0 });
    let x = render(&mut e, 36000);
    let before = peak_cents(&x[2400..12000], f);
    let after = peak_cents(&x[16800..33600], f);
    let rise = after - before;
    let depth = Params::default().bebung as f64;
    // the board's reactive pull adds about 2 cents at the raised pitch (31.9 and 32.0 cents over
    // two refits; the peak reads to a tenth of a cent)
    assert!((rise - depth).abs() < 2.5, "pressure on a sounding note: {rise:+.1} cents (depth {depth})");
}

#[test]
fn key_pressure_acts_on_its_key_and_channel_pressure_on_all() {
    let Some(_) = engine(DIR) else { return };
    let (fa, fe) = (f0(DIR, 69), f0(DIR, 76));
    let rises = |ev: Event| -> (f64, f64) {
        let run = |ev: Option<Event>| {
            let mut e = engine(DIR).unwrap();
            if let Some(ev) = ev {
                e.queue(0, ev);
            }
            e.queue(0, Event::NoteOn { key: 69, speed: 1.0 });
            e.queue(0, Event::NoteOn { key: 76, speed: 1.0 });
            let x = render(&mut e, 16000);
            (peak_cents(&x[2400..14400], fa), peak_cents(&x[2400..14400], fe))
        };
        let (b, p) = (run(None), run(Some(ev)));
        (p.0 - b.0, p.1 - b.1)
    };
    let depth = Params::default().bebung as f64;
    let (a, e) = rises(Event::KeyPressure { key: 69, value: 1.0 });
    assert!((a - depth).abs() < 2.0 && e.abs() < 0.5, "key pressure on A4: A4 {a:+.1}, E5 {e:+.1} cents");
    let (a, e) = rises(Event::ChannelPressure { value: 1.0 });
    assert!((a - depth).abs() < 2.0 && (e - depth).abs() < 2.0, "channel pressure: A4 {a:+.1}, E5 {e:+.1} cents");
}

#[test]
fn other_instruments_ignore_the_pressure() {
    let Some(_) = engine(HARPSICHORD) else { return };
    let run = |wheel: bool| {
        let mut e = engine(HARPSICHORD).unwrap();
        if wheel {
            e.queue(0, Event::ModWheel { value: 1.0 });
            e.queue(0, Event::ChannelPressure { value: 1.0 });
        }
        e.queue(0, Event::NoteOn { key: 69, speed: 1.0 });
        render(&mut e, 12000)
    };
    assert_eq!(run(false), run(true));
}
