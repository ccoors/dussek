//! The strip's glissando: white keys back to back, the old key's off and the next key's on at the same instant, a tongued
//! note on the player still in its release. Its core then carries the last note's state into the attack, which peaks louder
//! than the same note played apart; `attack_from_rest` starts a tongued note on a release from rest, the release faded out
//! over 4.4 ms first (cleared at once, it clicked). Needs the exported instruments (returns early without them).
use dussek::{Engine, Event, Instrument, Mix};

const SR: f64 = 44100.0;
const STEP: f64 = 0.12;

/// The export, its `brass` block's `attack_from_rest` set (`None`: left out): a copy in a temporary directory.
fn with(name: &str, from_rest: Option<bool>) -> Option<Instrument> {
    let src = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/")).join(name);
    let json = std::fs::read_to_string(src.join("instrument.json")).ok()?;
    let dir = std::env::temp_dir().join(format!("dussek-brass-drag-{name}-{from_rest:?}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    let brass = doc["brass"].as_object_mut().unwrap();
    brass.remove("attack_from_rest");
    if let Some(b) = from_rest {
        brass.insert("attack_from_rest".into(), b.into());
    }
    std::fs::write(dir.join("instrument.json"), serde_json::to_string(&doc).unwrap()).unwrap();
    std::fs::copy(src.join("data.bin"), dir.join("data.bin")).unwrap();
    let mut inst = Instrument::load(&dir).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    inst.output_gain_db = 0.0;
    Some(inst)
}

/// The events (sample, on, key) at velocity 100, dry at Recording, `n` samples: the left output.
fn play(inst: Instrument, mut events: Vec<(usize, bool, u8)>, n: usize) -> Vec<f32> {
    let mut e = Engine::new(inst, SR).unwrap();
    e.set_mix(Mix { room: false, ..Mix::ALL });
    events.sort_by_key(|&(t, on, _)| (t, on));
    for (t, on, key) in events {
        if on {
            let speed = e.hammer_speed(key, 100.0);
            e.queue(t as u64, Event::NoteOn { key, speed });
        } else {
            e.queue(t as u64, Event::KeyUp { key });
        }
    }
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut done = 0;
    while done < n {
        let m = (n - done).min(512);
        e.process(&mut l[done..done + m], &mut r[done..done + m]);
        done += m;
    }
    l
}

/// The keys each held STEP and the next `gap` later (on whole samples: the off and the next on in the same frame, the off
/// first, as the strip sends them): the output and each key's sample.
fn glide(inst: Instrument, keys: &[u8], gap: f64) -> (Vec<f32>, Vec<usize>) {
    let (step, gap) = ((STEP * SR) as usize, (gap * SR) as usize);
    let at: Vec<usize> = (0..keys.len()).map(|i| (0.5 * SR) as usize + i * (step + gap)).collect();
    let n = at[keys.len() - 1] + step + gap + SR as usize;
    let events = keys.iter().zip(&at).flat_map(|(&k, &a)| [(a, true, k), (a + step, false, k)]).collect();
    (play(inst, events, n), at)
}

/// Each note's peak level over its own 110 ms (windows of two periods, at least 20 ms), dB.
fn peaks(inst: Instrument, keys: &[u8], gap: f64) -> Vec<f64> {
    let (l, at) = glide(inst, keys, gap);
    let at = |i: usize| at[i];
    keys.iter()
        .enumerate()
        .map(|(i, &k)| {
            let f = 440.0 * 2f64.powf((f64::from(k) - 69.0) / 12.0);
            let w = ((2.0 / f).max(0.02) * SR) as usize;
            let (a, b) = (at(i), at(i) + ((STEP - 0.01) * SR) as usize);
            (a..b - w)
                .step_by((0.0025 * SR) as usize)
                .map(|j| {
                    let p = l[j..j + w].iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / w as f64;
                    10.0 * (p + 1e-24).log10()
                })
                .fold(f64::MIN, f64::max)
        })
        .collect()
}

/// The white keys from `lo` to `hi` and back down.
fn glissando(lo: u8, hi: u8) -> Vec<u8> {
    let up: Vec<u8> = (lo..=hi).filter(|k| [0, 2, 4, 5, 7, 9, 11].contains(&(k % 12))).collect();
    up.iter().chain(up.iter().rev().skip(1)).copied().collect()
}

/// The loudest dragged note over the same note apart, dB.
fn worst(inst: Instrument, keys: &[u8]) -> f64 {
    let dragged = peaks(inst.clone(), keys, 0.0);
    let apart = peaks(inst, keys, 0.3);
    dragged.iter().zip(&apart).map(|(d, a)| d - a).fold(f64::MIN, f64::max)
}

#[test]
fn a_dragged_note_starts_from_rest() {
    let keys = glissando(48, 67);
    let Some(carried) = with("horn", Some(false)) else { return };
    let from_rest = with("horn", Some(true)).unwrap();
    let (c, r) = (worst(carried, &keys), worst(from_rest, &keys));
    assert!(c > 5.0, "the horn's glissando, the state carried: the loudest note {c:.1} dB over apart");
    assert!(r < 3.5, "the horn's glissando from rest: the loudest note {r:.1} dB over apart");
}

#[test]
fn left_out_it_carries_the_state() {
    let keys = glissando(48, 60);
    let Some(out) = with("horn", None) else { return };
    assert!(!out.brass.as_ref().unwrap().attack_from_rest);
    assert_eq!(peaks(out, &keys, 0.0), peaks(with("horn", Some(false)).unwrap(), &keys, 0.0));
}


/// The output's second difference (the top of the spectrum, where a click is), its peak over `a..b`.
fn edge(x: &[f32], a: usize, b: usize) -> f64 {
    (a.max(2)..b.min(x.len())).map(|i| (f64::from(x[i]) - 2.0 * f64::from(x[i - 1]) + f64::from(x[i - 2])).abs()).fold(0.0, f64::max)
}

/// Each join's edge (1 ms before the key to 8 ms after: the fade and the start) over the louder of the last note's (5 to
/// 30 ms before) and the new one's (30 to 80 ms after), dB: over 0, a spike over both notes.
fn joins(x: &[f32], at: &[usize]) -> Vec<f64> {
    let ms = |t: f64| (t * SR / 1000.0) as usize;
    at[1..]
        .iter()
        .map(|&a| {
            let join = edge(x, a - ms(1.0), a + ms(8.0));
            let near = edge(x, a - ms(30.0), a - ms(5.0)).max(edge(x, a + ms(30.0), a + ms(80.0)));
            20.0 * (join / near.max(1e-12)).log10()
        })
        .collect()
}

#[test]
fn a_note_from_rest_fades_the_last_one_out() {
    let keys = glissando(48, 67);
    let Some(from_rest) = with("horn", Some(true)) else { return };
    let (x, at) = glide(from_rest, &keys, 0.0);
    let worst = joins(&x, &at).into_iter().fold(f64::MIN, f64::max);
    assert!(worst < 3.0, "the horn's glissando from rest: the worst join {worst:.1} dB over both notes' edges");
}

/// A tap on the strip: the next key's off 1 ms after its on, inside the fade. The note still starts, and plays as the same
/// tap from silence.
#[test]
fn a_key_up_during_the_fade_still_sounds() {
    let Some(inst) = with("horn", Some(true)) else { return };
    let (a, b) = ((0.5 * SR) as usize, (0.8 * SR) as usize);
    let tap = vec![(b, true, 57), (b + (0.001 * SR) as usize, false, 57)];
    let mut on_release = vec![(a, true, 55), (b, false, 55)];
    on_release.extend(tap.clone());
    let n = (1.6 * SR) as usize;
    let level = |x: &[f32], from: f64, to: f64| {
        let s = &x[b + (from * SR) as usize..b + (to * SR) as usize];
        10.0 * (s.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / s.len() as f64 + 1e-24).log10()
    };
    let (alone, faded) = (play(inst.clone(), tap, n), play(inst, on_release, n));
    for (p, q) in [(0.02, 0.04), (0.04, 0.08), (0.08, 0.12), (0.12, 0.3)] {
        let (a, f) = (level(&alone, p, q), level(&faded, p, q));
        assert!((f - a).abs() < 4.0, "{p}–{q} s after the tap: {f:.1} dB on the release, {a:.1} dB from silence");
    }
}

/// The note waiting for the fade let go (Solo switched on inside it: every player released): the fade ends in silence, the
/// last note's release does not blow the cleared bore again.
#[test]
fn a_note_let_go_during_the_fade_ends_with_it() {
    let Some(inst) = with("horn", Some(true)) else { return };
    let (a, b) = ((0.5 * SR) as usize, (0.8 * SR) as usize);
    let mut e = Engine::new(inst, SR).unwrap();
    e.set_mix(Mix { room: false, ..Mix::ALL });
    let speed = e.hammer_speed(55, 100.0);
    e.queue(a as u64, Event::NoteOn { key: 55, speed });
    e.queue(b as u64, Event::KeyUp { key: 55 });
    e.queue(b as u64, Event::NoteOn { key: 57, speed });
    let n = (1.4 * SR) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let cut = b + (0.001 * SR) as usize;
    e.process(&mut l[..cut], &mut r[..cut]);
    let mut p = e.params();
    p.brass.solo = true;
    e.set_params(p);
    let mut done = cut;
    while done < n {
        let m = (n - done).min(512);
        e.process(&mut l[done..done + m], &mut r[done..done + m]);
        done += m;
    }
    let level = |from: f64, to: f64| {
        let s = &l[(b as f64 + from * SR) as usize..(b as f64 + to * SR) as usize];
        10.0 * (s.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / s.len() as f64 + 1e-24).log10()
    };
    let (held, after) = (level(-0.1, -0.02), level(0.04, 0.3));
    assert!(after < held - 60.0, "40–300 ms after the key: {after:.1} dB, the note before {held:.1} dB");
}
