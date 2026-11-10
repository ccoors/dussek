//! The brass family in the engine: `PE_BRASS_EXPORT` a scratch export (or a committed one), else these return early.
use dussek::brass::family;
use dussek::brass::section::Ev;
use dussek::brass::streams::CTRL;
use dussek::mic::MicPosition;
use dussek::{Engine, Event, Instrument, Mix, Tuning};
use std::path::PathBuf;

const SR: f64 = 44100.0;

fn export() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PE_BRASS_EXPORT") {
        return Some(PathBuf::from(p));
    }
    let p = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/tenor_trombone"));
    p.join("instrument.json").exists().then_some(p)
}

/// The export at the fit's own level: these hold the engine to the players' own sound (the export's gain: `output_gain.rs`).
fn load() -> Option<Instrument> {
    let mut inst = Instrument::load(export()?).ok()?;
    inst.output_gain_db = 0.0;
    Some(inst)
}

fn dry(e: &mut Engine) {
    e.set_mix(Mix { room: false, ..Mix::ALL });
}

fn render(e: &mut Engine, n: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    let mut done = 0;
    while done < n {
        let m = (n - done).min(512);
        e.process(&mut l[done..done + m], &mut r[done..done + m]);
        done += m;
    }
    (l, r)
}

fn on(e: &mut Engine, at: u64, key: u8, v: f64) {
    let speed = e.hammer_speed(key, v);
    e.queue(at, Event::NoteOn { key, speed });
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

/// The strongest frequency near `f`, by an FFT-free scan of a Hann-windowed correlation.
fn peak_near(x: &[f32], sr: f64, f: f64) -> f64 {
    let n = x.len();
    let power = |hz: f64| {
        let (mut a, mut b) = (0.0, 0.0);
        for (i, v) in x.iter().enumerate() {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
            let t = 2.0 * std::f64::consts::PI * hz * i as f64 / sr;
            a += w * f64::from(*v) * t.cos();
            b += w * f64::from(*v) * t.sin();
        }
        a * a + b * b
    };
    let (mut best, mut at) = (0.0, f);
    for c in -100..=100 {
        let hz = f * 2f64.powf(c as f64 / 1200.0);
        let p = power(hz);
        if p > best {
            best = p;
            at = hz;
        }
    }
    at
}

#[test]
fn the_engine_plays_what_the_players_play() {
    let Some(inst) = load() else { return };
    let tables = inst.brass.clone().unwrap();
    let key = tables.near(60);
    let mut e = Engine::new(inst, SR).unwrap();
    dry(&mut e);
    on(&mut e, 0, key, 90.0);
    e.queue((SR * 0.4) as u64, Event::KeyUp { key });
    let n = (SR * 0.8) as usize;
    let lag = e.latency();
    let (l, r) = render(&mut e, n + lag);
    // the players' own: the same notes, the same random numbers
    let mut s = family::section(&tables, 0);
    s.post(Ev::On(key, 90.0));
    let mut mix = [[0.0; CTRL]; 2];
    let (mut a, mut b) = (Vec::new(), Vec::new());
    let mut posted = false;
    for step in 0.. {
        if a.len() >= n {
            break;
        }
        // the engine's first block has the note; its key-up arrives with the block that holds it
        if !posted && (step + 1) * CTRL > (SR * 0.4) as usize {
            s.post(Ev::Off(key));
            posted = true;
        }
        s.step(&mut mix);
        for k in 0..CTRL {
            a.push(mix[0][k].tanh() as f32);
            b.push(mix[1][k].tanh() as f32);
        }
    }
    let mut e2 = 0.0;
    let mut p = 0.0;
    for i in 0..n {
        let (x, y) = (f64::from(l[i + lag]), f64::from(a[i]));
        e2 += (x - y).powi(2) + (f64::from(r[i + lag]) - f64::from(b[i])).powi(2);
        p += y * y;
    }
    assert!(p > 0.0);
    let db = 10.0 * (e2 / p).log10();
    assert!(db < -90.0, "{db} dB from the players' own");
}

#[test]
fn a_chord_at_48k_keeps_its_pitch() {
    let Some(inst) = load() else { return };
    let t = inst.brass.clone().unwrap();
    let key = t.near(60);
    let f = t.note(0, key).f;
    let sr = 48000.0;
    let mut e = Engine::new(inst, sr).unwrap();
    dry(&mut e);
    let mut p = e.params();
    p.brass.virtual_player = false;
    e.set_params(p);
    on(&mut e, 0, key, 90.0);
    let (l, _) = render(&mut e, (sr * 1.2) as usize);
    let at = peak_near(&l[(sr * 0.5) as usize..], sr, f);
    let cents = 1200.0 * (at / f).log2();
    assert!(cents.abs() < 25.0, "{cents} cents at 48 kHz");
    // the same note at 44.1 kHz
    let inst = load().unwrap();
    let mut e = Engine::new(inst, SR).unwrap();
    dry(&mut e);
    let mut p = e.params();
    p.brass.virtual_player = false;
    e.set_params(p);
    on(&mut e, 0, key, 90.0);
    let (l, _) = render(&mut e, (SR * 1.2) as usize);
    let there = peak_near(&l[(SR * 0.5) as usize..], SR, f);
    assert!((1200.0 * (at / there).log2()).abs() < 2.0, "{at} Hz at 48 kHz, {there} Hz at 44.1 kHz");
}

#[test]
fn a_key_outside_the_range_is_silent() {
    let Some(inst) = load() else { return };
    let t = inst.brass.clone().unwrap();
    let key = t.range[1] + 1;
    let mut e = Engine::new(inst, SR).unwrap();
    dry(&mut e);
    on(&mut e, 0, key, 100.0);
    let (l, _) = render(&mut e, 8192);
    assert_eq!(rms(&l), 0.0);
    assert!(e.key_words() != [0; dussek::telemetry::KEY_WORDS], "the pressed key still shows");
}

#[test]
fn the_pedals_do_nothing() {
    let Some(inst) = load() else { return };
    let key = inst.brass.as_ref().unwrap().near(60);
    let run = |pedals: bool| {
        let mut e = Engine::new(load().unwrap(), SR).unwrap();
        dry(&mut e);
        if pedals {
            e.queue(0, Event::PedalDown);
            e.queue(0, Event::SoftPedal { depth: 1.0 });
            e.queue(0, Event::SostenutoDown);
        }
        on(&mut e, 0, key, 90.0);
        e.queue(8000, Event::KeyUp { key });
        render(&mut e, 44100).0
    };
    assert_eq!(run(false), run(true));
}

#[test]
fn a_mute_waits_for_the_next_tongued_note() {
    let Some(inst) = load() else { return };
    let t = inst.brass.clone().unwrap();
    if t.mutes.len() < 2 {
        return;
    }
    let key = t.near(60);
    let mut e = Engine::new(inst, SR).unwrap();
    dry(&mut e);
    let mut base = e.params();
    on(&mut e, 0, key, 90.0);
    render(&mut e, 22050);
    base.brass.mute = 1;
    e.set_params(base);
    let t0 = e.now();
    // a slur on the changed mute stays open (the old key up at once: left down it is harmony, a new tongued player)
    on(&mut e, t0, key + 1, 90.0);
    e.queue(t0 + 100, Event::KeyUp { key });
    let (with_slur, _) = render(&mut e, 22050);
    let mut f = Engine::new(load().unwrap(), SR).unwrap();
    dry(&mut f);
    on(&mut f, 0, key, 90.0);
    render(&mut f, 22050);
    let t1 = f.now();
    on(&mut f, t1, key + 1, 90.0);
    f.queue(t1 + 100, Event::KeyUp { key });
    let (open, _) = render(&mut f, 22050);
    assert_eq!(with_slur, open, "the sounding player keeps its mute through a slur");
    // lifted and tongued: the mute is on
    e.queue(e.now(), Event::KeyUp { key: key + 1 });
    render(&mut e, 44100);
    f.queue(f.now(), Event::KeyUp { key: key + 1 });
    render(&mut f, 44100);
    let (t2, t3) = (e.now(), f.now());
    on(&mut e, t2, key, 90.0);
    on(&mut f, t3, key, 90.0);
    let (muted, _) = render(&mut e, 22050);
    let (plain, _) = render(&mut f, 22050);
    assert_ne!(muted, plain);
}

#[test]
fn host_blocks_of_any_size_give_the_same_sound() {
    let Some(inst) = load() else { return };
    let key = inst.brass.as_ref().unwrap().near(60);
    for sr in [SR, 48000.0] {
        let run = |block: usize| {
            let mut e = Engine::new(load().unwrap(), sr).unwrap();
            dry(&mut e);
            on(&mut e, 100, key, 90.0);
            e.queue(10000, Event::KeyUp { key });
            let n = 30000;
            let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
            let mut done = 0;
            while done < n {
                let m = (n - done).min(block);
                e.process(&mut l[done..done + m], &mut r[done..done + m]);
                done += m;
            }
            l
        };
        let a = run(4096);
        assert_eq!(a, run(1), "a block of 1 at {sr}");
        assert_eq!(a, run(37), "a block of 37 at {sr}");
    }
}

#[test]
fn silence_ends_it() {
    let Some(inst) = load() else { return };
    let key = inst.brass.as_ref().unwrap().near(60);
    let mut e = Engine::new(inst, SR).unwrap();
    on(&mut e, 0, key, 100.0);
    render(&mut e, 8192);
    e.silence();
    render(&mut e, (SR * 2.5) as usize);
    let (l, _) = render(&mut e, 8192);
    assert_eq!(rms(&l), 0.0, "nothing comes back");
}

#[test]
fn a_non_finite_voice_is_reset_quietly() {
    let Some(inst) = load() else { return };
    let t = inst.brass.clone().unwrap();
    let mut s = family::section(&t, 0);
    let key = t.near(60);
    s.post(Ev::On(key, 90.0));
    let mut mix = [[0.0; CTRL]; 2];
    for _ in 0..50 {
        s.step(&mut mix);
    }
    s.voices[0].poison();
    for _ in 0..200 {
        s.step(&mut mix);
        assert!(mix[0].iter().all(|x| x.is_finite()) && mix[1].iter().all(|x| x.is_finite()));
    }
    assert_eq!(s.resets, 1);
}

#[test]
fn the_tuning_moves_the_note() {
    let Some(inst) = load() else { return };
    let t = inst.brass.clone().unwrap();
    let key = t.near(60);
    let f = t.note(0, key).f;
    let tuning = Tuning { pitch_ratio: 2f64.powf(50.0 / 1200.0), mic: MicPosition::Recording, ..Tuning::default() };
    let mut e = Engine::with_tuning(inst, SR, tuning).unwrap();
    dry(&mut e);
    let mut p = e.params();
    p.brass.virtual_player = false;
    e.set_params(p);
    on(&mut e, 0, key, 90.0);
    let (l, _) = render(&mut e, 44100);
    let at = peak_near(&l[22050..], SR, f * 2f64.powf(50.0 / 1200.0));
    assert!((1200.0 * (at / f).log2() - 50.0).abs() < 30.0, "{at} Hz from {f} Hz");
}

#[test]
fn a_mute_past_the_instrument_s_is_open() {
    // the window says Open for it: a value kept from an instrument with more mutes
    let Some(inst) = load() else { return };
    let key = inst.brass.as_ref().unwrap().near(60);
    let run = |mute: u32| {
        let mut e = Engine::new(load().unwrap(), SR).unwrap();
        dry(&mut e);
        let mut p = e.params();
        p.brass.mute = mute;
        e.set_params(p);
        on(&mut e, 0, key, 90.0);
        render(&mut e, 22050).0
    };
    assert_eq!(run(99), run(0));
}

#[test]
fn a_mute_is_chosen_by_its_kind() {
    // the value names a kind (0 open, 1 straight, 2 cup, 3 wah, 4 wah with its stem, 5 bucket, 6 plunger): the trumpet's
    // wah stays a wah on the next instrument, or open where it has none, never the mute in the wah's place
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/bass_trombone"));
    let Ok(inst) = Instrument::load(path) else { return };
    assert_eq!(inst.brass.as_ref().unwrap().mutes, ["open", "straight", "cup", "bucket", "plunger"]);
    let key = inst.brass.as_ref().unwrap().near(48);
    let run = |mute: u32| {
        let mut e = Engine::new(inst.clone(), SR).unwrap();
        dry(&mut e);
        let mut p = e.params();
        p.brass.mute = mute;
        e.set_params(p);
        on(&mut e, 0, key, 90.0);
        render(&mut e, 22050).0
    };
    let open = run(0);
    assert_eq!(run(3), open, "no wah: open");
    assert_ne!(run(5), open, "the bucket");
}

/// Soft lines falling onto their last keys, played in turn: the players vary from note to note, so each line comes round
/// a few times. Where each last key sits from 50 to 270 ms after its slur, cents, or none without the export.
fn soft_landings(lines: &[&[u8]]) -> Option<Vec<(u8, f64)>> {
    let p = std::env::var_os("PE_HORN_EXPORT")
        .map_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/horn")), PathBuf::from);
    let inst = Instrument::load(p).ok()?;
    let mut e = Engine::new(inst, SR).unwrap();
    dry(&mut e);
    let (mut t, mut at) = (0.3, Vec::new());
    for _ in 0..4 {
        for line in lines {
            for (i, key) in line.iter().enumerate() {
                on(&mut e, (SR * t) as u64, *key, 60.0);
                e.queue((SR * (t + 0.28)) as u64, Event::KeyUp { key: *key });
                if i + 1 == line.len() {
                    at.push((*key, t));
                }
                t += 0.25;
            }
            t += 0.8;
        }
    }
    let (l, _) = render(&mut e, (SR * t) as usize);
    Some(
        at.iter()
            .map(|(key, t)| {
                let f = 440.0 * 2f64.powf((f64::from(*key) - 69.0) / 12.0);
                let hz = peak_near(&l[(SR * (t + 0.05)) as usize..(SR * (t + 0.27)) as usize], SR, f);
                (*key, 1200.0 * (hz / f).log2())
            })
            .collect(),
    )
}

#[test]
fn a_soft_slur_on_the_horn_lands_on_its_note() {
    // a stronger mode close under the note: G4 stayed 90 c flat, D-flat 5 140 c, for as long as they sounded
    let lines: [&[u8]; 4] = [&[69, 67, 66], &[70, 68, 67], &[75, 74, 73], &[76, 75, 74]];
    let Some(got) = soft_landings(&lines) else { return };
    let off: Vec<_> = got.iter().filter(|(_, c)| c.abs() > 40.0).collect();
    assert!(off.is_empty(), "{off:?}");
}

#[test]
fn the_latency_reported_is_the_latency_at_every_rate() {
    // the brass plays at the fits' 44.1 kHz and is resampled to the host's: the resampler's delay is part of what the
    // host must make up, so a note starts at the same time after the reported latency at every rate
    let Some(inst) = load() else { return };
    let key = inst.brass.as_ref().unwrap().near(60);
    // a note waits for the end of its block and behind what the brass made ahead, which cycles over a few blocks at a
    // rate not 44.1 kHz: the mean over starts spread across many blocks
    let start = |sr: f64| {
        let starts = (0..16).map(|j| {
            let mut e = Engine::new(inst.clone(), sr).unwrap();
            dry(&mut e);
            // the same note every time: the player's variation moves a start by milliseconds
            let mut p = e.params();
            p.variation = 0.0;
            p.brass.virtual_player = false;
            e.set_params(p);
            let at = (0.05 * sr) as u64 + 47 * j;
            on(&mut e, at, key, 100.0);
            let (l, _) = render(&mut e, (0.4 * sr) as usize);
            let peak = l.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            let first = l.iter().position(|x| x.abs() > 0.05 * peak).unwrap();
            (first as f64 - at as f64 - e.latency() as f64) / sr
        });
        starts.sum::<f64>() / 16.0
    };
    let base = start(44100.0);
    for sr in [48000.0, 88200.0, 96000.0] {
        let t = start(sr);
        assert!((t - base).abs() < 0.2e-3, "{sr} Hz: {:.2} ms after the reported latency, 44.1 kHz {:.2} ms", t * 1e3, base * 1e3);
    }
}

