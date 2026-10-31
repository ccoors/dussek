//! The buff stop: pads on one register's strings add σ_buff(k) = a + g·sin²(kπp) to each
//! partial's loss, from the next pluck of a key on. Mostly on Harpsichord A (Unk) with a
//! buff set here (a = A, g = G, p = P on every key); the exported block in
//! `loads_the_exported_buff`. Skipped without the exports.

use std::f64::consts::PI;

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;
const A: f64 = 0.5;
const G: f64 = 4.0;
const P: f64 = 0.1;

fn load(name: &str) -> Option<Instrument> {
    Instrument::load(format!("{ROOT}{name}")).ok()
}

/// Harpsichord A with the test buff on every key, or with none
fn harpsichord(buff: bool) -> Option<Instrument> {
    let mut inst = load("harpsichord")?;
    for n in inst.notes.iter_mut() {
        n.spec.buff = buff.then_some((A, G, P));
    }
    inst.compile(SR, Tuning::default());
    Some(inst)
}

#[test]
fn compile_gives_the_pad_shape() {
    let (Some(on), Some(off)) = (harpsichord(true), harpsichord(false)) else { return };
    let n = &on.notes[39];
    assert_eq!(n.buff_sigma.len(), n.k);
    for (j, s) in n.buff_sigma.iter().enumerate() {
        let want = A + G * ((j + 1) as f64 * PI * P).sin().powi(2);
        assert!((s - want).abs() < 1e-12, "partial {}: {s} vs {want}", j + 1);
    }
    assert!(on.has_buff());
    assert!(off.notes.iter().all(|n| n.buff_sigma.is_empty()));
    assert!(!off.has_buff());
}

#[test]
fn loads_the_exported_buff() {
    if let Some(inst) = load("harpsichord") {
        assert!(inst.has_buff());
        assert!(inst.notes.iter().all(|n| n.buff_sigma.len() == n.k && n.buff_sigma.iter().all(|s| s.is_finite() && *s >= 0.0)));
    }
    if let Some(inst) = load("flemish") {
        assert!(inst.has_buff());
        assert!(inst.notes[..88].iter().all(|n| !n.buff_sigma.is_empty()), "the 8′ has the buff");
        assert!(inst.notes[88..].iter().all(|n| n.buff_sigma.is_empty()), "the 4′ has none");
    }
    for name in ["grand_a", "upright_a"] {
        if let Some(inst) = load(name) {
            assert!(!inst.has_buff(), "{name}");
        }
    }
}

#[test]
fn rejects_a_bad_buff_block() {
    let src = format!("{ROOT}harpsichord");
    let Ok(json) = std::fs::read_to_string(format!("{src}/instrument.json")) else { return };
    let dir = std::env::temp_dir().join(format!("pe_buff_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(format!("{src}/data.bin"), dir.join("data.bin")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    let notes: Vec<u8> = (21..109).collect();
    let mut try_load = |buff: serde_json::Value| {
        doc["buff"] = buff;
        std::fs::write(dir.join("instrument.json"), doc.to_string()).unwrap();
        Instrument::load(&dir)
    };
    let good = serde_json::json!({"register": 0, "notes": notes, "flat": vec![0.5; 88], "gain": vec![1.0; 88], "pos": vec![0.1; 88]});
    assert!(try_load(good.clone()).expect("a good block loads").has_buff());
    for (key, bad) in [
        ("pos", serde_json::json!(vec![0.7; 88])),
        ("pos", serde_json::json!(vec![0.0; 88])),
        ("gain", serde_json::json!(vec![-1.0; 88])),
        ("gain", serde_json::json!(vec![1.0; 87])),
        ("flat", serde_json::json!(vec![-0.5; 88])),
        ("flat", serde_json::json!(vec![0.5; 87])),
        ("register", serde_json::json!(1)),
        ("notes", serde_json::json!((22..110).collect::<Vec<u8>>())),
    ] {
        let mut b = good.clone();
        b[key] = bad.clone();
        assert!(try_load(b).is_err(), "{key} = {bad}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// quiet: no knock, release noise, sympathetic strings or room (decays compare)
fn quiet(buff: bool) -> Params {
    Params { buff, knock: 0.0, release: 0.0, sympathetic: 0.0, room: 0.0, ..Params::default() }
}

fn engine(inst: &Instrument, p: Params) -> Engine {
    let mut e = Engine::with_tuning(inst.clone(), SR, Tuning::default()).expect("engine");
    e.set_params(p);
    e
}

/// n samples (left) in 256-sample blocks; `switch`: new params from that sample (a multiple of 256)
fn run(e: &mut Engine, events: &[(u64, Event)], n: usize, switch: Option<(usize, Params)>) -> Vec<f32> {
    for &(t, ev) in events {
        e.queue(t, ev);
    }
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    let at = switch.map_or(n, |(t, _)| t);
    for (cl, cr) in l[..at].chunks_mut(256).zip(r[..at].chunks_mut(256)) {
        e.process(cl, cr);
    }
    if let Some((_, p)) = switch {
        e.set_params(p);
    }
    for (cl, cr) in l[at..].chunks_mut(256).zip(r[at..].chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

fn level(x: &[f32], f: f64) -> f64 {
    let n = x.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / n).cos();
        let ph = 2.0 * PI * f * i as f64 / SR;
        re += w * v as f64 * ph.cos();
        im -= w * v as f64 * ph.sin();
    }
    20.0 * (re.hypot(im) + 1e-30).log10()
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| (*v as f64).powi(2)).sum()
}

const C4: [(u64, Event); 1] = [(0, Event::NoteOn { key: 60, speed: 1.0 })];

#[test]
fn buff_off_is_bit_identical() {
    let (Some(on), Some(off)) = (harpsichord(true), harpsichord(false)) else { return };
    let ev = [
        (0, Event::NoteOn { key: 48, speed: 1.0 }),
        (0, Event::NoteOn { key: 60, speed: 1.0 }),
        (24000, Event::KeyUp { key: 60 }),
    ];
    let p = Params::default();
    assert_eq!(run(&mut engine(&on, p), &ev, 48000, None), run(&mut engine(&off, p), &ev, 48000, None));
}

#[test]
fn decay_follows_the_closed_form() {
    let Some(inst) = harpsichord(true) else { return };
    let note = inst.notes[39].clone(); // C4
    // level drop of partials 3…5 from the window around 0.3 s to the one around 0.9 s
    let drop = |buff: bool| -> Vec<f64> {
        let x = run(&mut engine(&inst, quiet(buff)), &C4, 48000, None);
        [3usize, 4, 5].iter().map(|&k| level(&x[9600..19200], note.f_mean[k - 1]) - level(&x[38400..48000], note.f_mean[k - 1])).collect()
    };
    let (off, on) = (drop(false), drop(true));
    for (i, k) in [3usize, 4, 5].iter().enumerate() {
        let want = note.buff_sigma[k - 1] * 0.6 * 20.0 / 10f64.ln();
        let got = on[i] - off[i];
        assert!((got - want).abs() < 1.0, "partial {k}: extra drop {got:.2} dB, closed form {want:.2} dB");
    }
}

#[test]
fn switch_waits_for_the_next_pluck() {
    let Some(inst) = harpsichord(true) else { return };
    let ev = [(0, Event::NoteOn { key: 60, speed: 1.0 }), (24064, Event::NoteOn { key: 60, speed: 1.0 })];
    let plain = run(&mut engine(&inst, quiet(false)), &ev, 48000, None);
    let switched = run(&mut engine(&inst, quiet(false)), &ev, 48000, Some((12032, quiet(true))));
    assert_eq!(plain[..24064], switched[..24064], "the ringing note is unchanged until the key plucks again");
    assert!(switched.iter().all(|v| v.is_finite()));
    // the second pluck puts the whole string under the pads
    assert!(energy(&switched[40000..48000]) < 0.5 * energy(&plain[40000..48000]));
    // no jump at the pluck beyond the pluck itself: the peak stays near the unswitched one
    let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak(&switched[24064..26464]) < 1.1 * peak(&plain[24064..26464]));
}

#[test]
fn toggling_returns_to_the_unbuffed_decay() {
    let Some(inst) = harpsichord(true) else { return };
    // 40 plucks alternating on/off, then a pluck with the buff off
    let mut e = engine(&inst, quiet(false));
    for i in 0..40u64 {
        e.queue(i * 512, Event::NoteOn { key: 60, speed: 1e-6 });
        let x = run(&mut e, &[], 512, None);
        assert!(x.iter().all(|v| v.is_finite()));
        e.set_params(quiet(i % 2 == 0));
    }
    e.set_params(quiet(false));
    let late = |e: &mut Engine| {
        let t = e.now(); // events are queued at absolute samples
        let x = run(e, &[(t, Event::NoteOn { key: 60, speed: 1.0 })], 48000, None);
        level(&x[38400..48000], inst.notes[39].f_mean[4])
    };
    let toggled = late(&mut e);
    let fresh = late(&mut engine(&inst, quiet(false)));
    assert!((toggled - fresh).abs() < 0.5, "toggled {toggled:.2} dB vs fresh {fresh:.2} dB");
}

#[test]
fn key_up_damps_with_buff_and_damper() {
    let Some(inst) = harpsichord(true) else { return };
    let ev = [(0, Event::NoteOn { key: 60, speed: 1.0 }), (24064, Event::KeyUp { key: 60 })];
    let off = run(&mut engine(&inst, quiet(false)), &ev, 36000, None);
    let on = run(&mut engine(&inst, quiet(true)), &ev, 36000, None);
    assert!(energy(&on[26000..30000]) < energy(&off[26000..30000]));
}

#[test]
fn only_the_named_register() {
    let Some(mut inst) = load("flemish") else { return };
    for n in inst.notes.iter_mut() {
        n.spec.buff = (n.register == 0).then_some((A, G, P));
    }
    inst.compile(SR, Tuning::default());
    assert!(inst.notes[88..].iter().all(|n| n.buff_sigma.is_empty()));
    let four = |buff| Params { stops: 2, ..quiet(buff) };
    assert_eq!(run(&mut engine(&inst, four(false)), &C4, 24000, None), run(&mut engine(&inst, four(true)), &C4, 24000, None));
    let both = |buff| Params { stops: 3, ..quiet(buff) };
    assert_ne!(run(&mut engine(&inst, both(false)), &C4, 24000, None), run(&mut engine(&inst, both(true)), &C4, 24000, None));
}

#[test]
fn no_block_ignores_the_param() {
    let Some(inst) = load("grand_a") else { return };
    let ev = [(0, Event::NoteOn { key: 60, speed: 2.0 }), (24000, Event::KeyUp { key: 60 })];
    let p = Params::default();
    assert_eq!(
        run(&mut engine(&inst, p), &ev, 36000, None),
        run(&mut engine(&inst, Params { buff: true, ..p }), &ev, 36000, None)
    );
}

#[test]
fn adopt_keeps_the_buff() {
    let Some(inst) = harpsichord(true) else { return };
    let n = 48000;
    let at = 24_037; // mid-block
    let reference = run(&mut engine(&inst, quiet(true)), &C4, n, None);
    let mut a = engine(&inst, quiet(true));
    a.queue(0, C4[0].1);
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    a.process(&mut l[..at], &mut r[..at]);
    let mut b = engine(&inst, quiet(true));
    assert!(b.adopt(&mut a));
    for (cl, cr) in l[at..].chunks_mut(256).zip(r[at..].chunks_mut(256)) {
        b.process(cl, cr);
    }
    let peak = reference.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let diff = reference.iter().zip(&l).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
    assert!(diff < peak * 1e-5, "difference {diff} re peak {peak}");
}

/// Harpsichord A with a buff of this flat part on every key (no pad)
fn harpsichord_flat(flat: f64) -> Option<Instrument> {
    let mut inst = load("harpsichord")?;
    for n in inst.notes.iter_mut() {
        n.spec.buff = Some((flat, 0.0, P));
    }
    inst.compile(SR, Tuning::default());
    Some(inst)
}

#[test]
fn switching_off_restores_the_string_exactly() {
    // an extreme loss: subtracting and adding it back must not cancel the string's own loss
    let Some(inst) = harpsichord_flat(1e16) else { return };
    let mut e = engine(&inst, quiet(true));
    run(&mut e, &[(0, Event::NoteOn { key: 60, speed: 1e-6 })], 512, None); // the string takes the pads
    e.set_params(quiet(false));
    let t = e.now();
    let toggled = run(&mut e, &[(t, Event::NoteOn { key: 60, speed: 1.0 })], 48000, None);
    let fresh = run(&mut engine(&inst, quiet(false)), &C4, 48000, None);
    assert!(toggled.iter().all(|v| v.is_finite()));
    let late = |x: &[f32]| energy(&x[38400..48000]);
    assert!((late(&toggled) / late(&fresh) - 1.0).abs() < 1e-3, "toggled {} vs fresh {}", late(&toggled), late(&fresh));
}

#[test]
fn switching_costs_little_on_the_audio_thread() {
    // a bass chord re-plucked right after the stop changed vs the same chord without a change
    let Some(inst) = harpsichord(true) else { return };
    let chord: Vec<u8> = (21..31).collect();
    let time = |toggle: bool| -> f64 {
        let mut best = f64::INFINITY;
        for _ in 0..7 {
            let mut e = engine(&inst, quiet(false));
            for &k in &chord {
                e.queue(0, Event::NoteOn { key: k, speed: 1.0 });
            }
            run(&mut e, &[], 4800, None);
            e.set_params(quiet(toggle));
            let t = e.now();
            for &k in &chord {
                e.queue(t, Event::NoteOn { key: k, speed: 1.0 });
            }
            let (mut l, mut r) = (vec![0.0; 64], vec![0.0; 64]);
            let t0 = std::time::Instant::now();
            e.process(&mut l, &mut r);
            best = best.min(t0.elapsed().as_secs_f64());
        }
        best
    };
    let (plain, toggled) = (time(false), time(true));
    assert!(toggled < 1.25 * plain, "re-pluck block {:.0} µs, after a switch {:.0} µs", plain * 1e6, toggled * 1e6);
}

// ---- the sympathetic strings under the buff ----

#[test]
fn compile_gives_the_sympathetic_partials_the_pad_loss() {
    let (Some(on), Some(off)) = (harpsichord(true), harpsichord(false)) else { return };
    let n = &on.notes[39];
    assert!(!n.sym.f.is_empty());
    assert_eq!(n.sym.buff_sigma.len(), n.sym.f.len());
    // the resonators are the note's partials below the sympathetic limit, in order
    for (i, s) in n.sym.buff_sigma.iter().enumerate() {
        assert!((s - n.buff_sigma[i]).abs() < 1e-12, "resonator {i}: {s} vs {}", n.buff_sigma[i]);
    }
    assert!(off.notes.iter().all(|n| n.sym.buff_sigma.is_empty()));
}

/// The bank of `inst` reduced to one resonator on `key` (440 Hz, loss 1 /s, pad loss 3 /s):
/// its decay rate (1/s) over 0.25 … 0.75 s after an impulse, the buff switched to `on`
/// before the impulse and to `then` after 0.25 s.
fn bank_decay(mut inst: Instrument, key: usize, on: bool, then: bool) -> f64 {
    use pianoengine::resonance::{Bank, KeyState};
    for (i, n) in inst.notes.iter_mut().enumerate() {
        let s = &mut n.phys.sym;
        let keep = i == key;
        s.f = if keep { vec![440.0] } else { vec![] };
        s.sigma = if keep { vec![1.0] } else { vec![] };
        s.kappa = if keep { vec![realfft::num_complex::Complex64::new(1.0, 0.0)] } else { vec![] };
        s.damper_sigma = if keep { vec![0.0] } else { vec![] };
        s.buff_sigma = if keep && !n.phys.buff_sigma.is_empty() { vec![3.0] } else { vec![] };
    }
    let mut bank = Bank::new(&inst, SR);
    bank.set_buff(on);
    let mut acc = vec![[0.0f32; pianoengine::kernel::LANES]; 1];
    bank.set_key(key, KeyState { driven: true, engage: 0.0 });
    bank.run(&[1.0], &mut acc);
    bank.set_key(key, KeyState { driven: false, engage: 0.0 });
    let n = (0.25 * SR) as usize;
    let env = |bank: &mut Bank| {
        let mut peak = 0.0f64;
        for _ in 0..n {
            let mut a = vec![[0.0f32; pianoengine::kernel::LANES]; 1];
            bank.run(&[0.0], &mut a);
            peak = peak.max(a[0].iter().map(|&x| x.abs() as f64).sum::<f64>());
        }
        peak
    };
    env(&mut bank);
    bank.set_buff(then);
    let a = env(&mut bank);
    let b = env(&mut bank);
    (a / b).ln() / 0.25
}

#[test]
fn buffed_resonators_decay_with_the_pads() {
    let Some(inst) = harpsichord(true) else { return };
    let off = bank_decay(inst.clone(), 39, false, false);
    let on = bank_decay(inst.clone(), 39, true, true);
    assert!((off - 1.0).abs() < 0.05, "off {off}");
    assert!((on - 4.0).abs() < 0.1, "on {on}");
    // the switch acts on the ringing resonators at once (the pads rest on every string)
    let switched = bank_decay(inst.clone(), 39, false, true);
    assert!((switched - 4.0).abs() < 0.1, "switched on {switched}");
    let back = bank_decay(inst, 39, true, false);
    assert!((back - 1.0).abs() < 0.05, "switched off {back}");
}

#[test]
fn unbuffed_resonators_ignore_the_switch() {
    let Some(inst) = harpsichord(false) else { return };
    let off = bank_decay(inst.clone(), 39, false, false);
    let on = bank_decay(inst, 39, true, true);
    assert_eq!(off, on);
}

#[test]
fn engine_switches_the_bank_live() {
    // C5 held silently (its strings free) rings along with a held C4; switching the buff on
    // while both sound leaves C4's voice as plucked but damps C5's resonators at once
    let Some(inst) = harpsichord(true) else { return };
    let ev = [(0, Event::KeyDown { key: 72 }), (0, Event::NoteOn { key: 60, speed: 1.0 })];
    let sym = |s: f32| Params { sympathetic: s, ..quiet(false) };
    let render = |p: Params, switch: bool| {
        let mut e = engine(&inst, p);
        run(&mut e, &ev, 48000, switch.then_some((4864, Params { buff: true, ..p })))
    };
    let base = render(sym(0.0), false);
    let bank = |x: &[f32]| energy(&x.iter().zip(&base).map(|(a, b)| a - b).collect::<Vec<_>>()[24000..]);
    let (kept, switched) = (bank(&render(sym(1.0), false)), bank(&render(sym(1.0), true)));
    assert!(kept > 0.0);
    // (driven mostly off resonance, where the loss matters little; the size: the bank tests)
    assert!(switched < 0.95 * kept, "buffed resonators must ring less: {switched} vs {kept}");
}
