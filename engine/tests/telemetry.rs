//! The telemetry frame: what the engine reports about its sounding voices. Needs the
//! exported instruments (skipped without them), like the other engine tests.

use pianoengine::telemetry::{Frame, KEY_DAMPER, KEY_HAMMER, KEY_WORDS, MAX_PARTIALS, RING_FRAMES};
use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

fn engine(name: &str, stops: u32) -> Option<Engine> {
    let inst = Instrument::load(format!("{ROOT}{name}")).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).expect("engine");
    e.set_params(Params { stops, ..Params::default() });
    Some(e)
}

fn run(e: &mut Engine, frames: usize) {
    let (mut l, mut r) = (vec![0.0f32; frames], vec![0.0f32; frames]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
}

/// `midi` is the MIDI note of the event; the frame's `key` is the 0-based piano key (MIDI - 21)
fn voice_of(f: &Frame, midi: u8, reg: u8) -> Option<usize> {
    (0..f.voices as usize).find(|&i| f.voice[i].key == midi - 21 && f.voice[i].reg == reg)
}

#[test]
fn idle_engine_has_no_voices() {
    let Some(mut e) = engine("grand_a", 3) else { return };
    run(&mut e, 4800);
    let mut f = Frame::boxed();
    e.telemetry_frame(&mut f);
    assert_eq!(f.voices, 0);
    assert_eq!(f.keys, [0u32; KEY_WORDS]);
    assert_eq!(f.pedal, 0.0);
}

#[test]
fn a_struck_note_reports_its_partials() {
    let Some(mut e) = engine("grand_a", 3) else { return };
    let key = 69u8; // A4
    e.queue(0, Event::NoteOn { key, speed: 3.0 });
    run(&mut e, 4800);
    let mut f = Frame::boxed();
    e.telemetry_frame(&mut f);
    let i = voice_of(&f, key, 0).expect("voice for A4");
    let v = &f.voice[i];
    assert!(v.count as usize <= MAX_PARTIALS && v.count > 8, "count {}", v.count);
    assert!(v.speed > 2.9 && v.speed < 3.1);
    assert!(v.level > 0.0);
    let ps = &v.partials[..v.count as usize];
    assert!(ps.windows(2).all(|w| w[0].amp >= w[1].amp), "strongest first");
    assert!(ps.iter().all(|p| p.freq > 0.0 && p.decay > 0.0 && p.amp > 0.0 && p.amp.is_finite()));
    // the fundamental is among them, within a few cents of 440 Hz (stiffness sharpens it a little)
    assert!(ps.iter().any(|p| (p.freq / 440.0 - 1.0).abs() < 0.01), "no partial near 440 Hz: {:?}", ps.iter().map(|p| p.freq).collect::<Vec<_>>());
    // key words: A4 = key 48, held and ringing
    assert_eq!(f.keys[48 / 12] >> (2 * (48 % 12)) & 0b11, 0b11);
}

#[test]
fn a_struck_key_shows_in_the_visualization() {
    let Some(mut e) = engine("grand_a", 3) else { return };
    let (key, at) = (60u8, 2400u64); // C4, struck after 50 ms
    let k = key as usize - 21;
    let mut f = Frame::boxed();
    run(&mut e, 4800);
    e.telemetry_frame(&mut f);
    assert_eq!(f.key[k].struck_at, 0);
    assert!(f.key.iter().all(|k| k.amp.iter().all(|&a| a == 0.0)), "nothing sounds yet");
    assert!(f.key[k].engage > 0.99, "the damper rests");
    assert!(f.key[k].flags & KEY_DAMPER != 0 && f.key[k].flags & KEY_HAMMER != 0);
    assert!(f.key[k].length > 0.3 && f.key[k].length < 1.0, "C4's speaking length {}", f.key[k].length);
    assert!(f.key[k].point > 0.05 && f.key[k].point < 0.2);
    e.queue(4800 + at, Event::NoteOn { key, speed: 3.0 });
    run(&mut e, 9600);
    e.telemetry_frame(&mut f);
    assert_eq!(f.key[k].struck_at, 4800 + at);
    assert!((f.key[k].speed - 3.0).abs() < 1e-6);
    assert!(f.key[k].engage < 0.01, "the damper lifted");
    // the fundamental moves most, a millimeter or so at a forte
    let a = &f.key[k].amp;
    assert!(a[0] > 1e-4 && a[0] < 1e-2, "fundamental {}", a[0]);
    assert!(a.iter().all(|x| x.is_finite() && *x >= 0.0));
    let moved = f.key.iter().filter(|k| k.amp.iter().any(|&x| x > 0.0)).count();
    assert_eq!(moved, 1, "only the struck key's strings move (the ring is apart)");
    // released: the damper rests again and the strings stop, the strike stays recorded
    let before = a[0];
    e.queue(14400 + 4800, Event::KeyUp { key });
    run(&mut e, 48000);
    e.telemetry_frame(&mut f);
    assert!(f.key[k].engage > 0.99);
    assert!(f.key[k].amp[0] < 0.01 * before, "{} after {}", f.key[k].amp[0], before);
    assert_eq!(f.key[k].struck_at, 4800 + at);
}

#[test]
fn a_bar_shows_in_the_visualization() {
    let Some(mut e) = engine("marimba", 3) else { return };
    let k = 60 - 21;
    e.queue(0, Event::NoteOn { key: 60, speed: 2.0 });
    run(&mut e, 2400);
    let mut f = Frame::boxed();
    e.telemetry_frame(&mut f);
    assert!(f.key[k].flags & (KEY_DAMPER | KEY_HAMMER) == 0);
    let a = f.key[k].amp.iter().fold(0.0f32, |m, &x| m.max(x));
    assert!(a > 1e-5 && a < 3e-2, "a bar on the strings' scale: {a}");
}

#[test]
fn the_voice_decays() {
    let Some(mut e) = engine("grand_a", 3) else { return };
    e.queue(0, Event::NoteOn { key: 69, speed: 3.0 }); // held all the time: no key-up, no pedal, so no culling by release
    run(&mut e, 4800);
    let mut f = Frame::boxed();
    e.telemetry_frame(&mut f);
    let i = voice_of(&f, 69, 0).unwrap();
    let (early, p0) = (f.voice[i].level, f.voice[i].partials[0]);
    let dt = 1.0f32;
    run(&mut e, SR as usize);
    e.telemetry_frame(&mut f);
    let i = voice_of(&f, 69, 0).expect("a held note is still there");
    let late = f.voice[i].level;
    assert_eq!(f.keys[48 / 12] >> (2 * (48 % 12)) & 0b01, 0b01, "still held");
    assert!(late < early * 0.8, "natural decay of a held note: {late} vs {early}");
    // the strongest partial's amplitude after dt follows its own decay rate: amp · e^(−σ·dt)
    let p1 = f.voice[i].partials[..f.voice[i].count as usize]
        .iter()
        .find(|p| (p.freq - p0.freq).abs() < 0.05 && p.kind == p0.kind)
        .expect("the strongest partial is still listed");
    let expected = p0.amp * (-p0.decay * dt).exp();
    assert!((p1.amp / expected - 1.0).abs() < 0.1, "amp {} after {dt} s, expected {expected} from decay {}", p1.amp, p0.decay);
    assert!(p0.decay > 0.1, "a non-trivial rate: {}", p0.decay);
    // released: not held any more, and it dies (culled) after a while
    e.queue(0, Event::KeyUp { key: 69 }); // an event in the past applies at the next block
    run(&mut e, 4800);
    e.telemetry_frame(&mut f);
    assert_eq!(f.keys[48 / 12] >> (2 * (48 % 12)) & 0b01, 0);
}

#[test]
fn harpsichord_voices_map_to_keys() {
    let Some(mut e) = engine("flemish", 3) else { return };
    e.queue(0, Event::NoteOn { key: 60, speed: 3.0 });
    run(&mut e, 4800);
    let mut f = Frame::boxed();
    e.telemetry_frame(&mut f);
    // both registers (8′ and 4′) sound under the same key
    assert!(voice_of(&f, 60, 0).is_some(), "8′");
    assert!(voice_of(&f, 60, 1).is_some(), "4′");
    assert!(f.voice[..f.voices as usize].iter().all(|v| v.key == 60 - 21));
}

#[test]
fn only_the_loudest_voices_are_listed() {
    let Some(mut e) = engine("grand_a", 3) else { return };
    // 30 notes, speeds strictly increasing with the key: the 6 lowest soft (0.6 … 0.85 m/s), the
    // others 2.0 … 4.3 m/s (one even ramp of 1.0 … 3.9 m/s left a 1.7 % gap at the cut once Grand A
    // was voiced even, step 139: its keys' levels lie close, the order at the cut a matter of the key)
    let speed = |key: u8| if key < 36 { 0.6 + (key - 30) as f64 * 0.05 } else { 2.0 + (key - 36) as f64 * 0.1 };
    for key in 30u8..60 {
        e.queue(0, Event::NoteOn { key, speed: speed(key) });
    }
    run(&mut e, 4800);
    let mut f = Frame::boxed();
    e.telemetry_frame(&mut f);
    assert_eq!(f.voices as usize, 24);
    let lv: Vec<f32> = f.voice[..24].iter().map(|v| v.level).collect();
    assert!(lv.windows(2).all(|w| w[0] >= w[1]), "sorted by level");
    // The assumption "a harder strike is louder" is not strictly true across keys (the level also
    // depends on the key), so it is checked from the data: the same notes struck one at a time
    // give every key's level; the 24 listed must be exactly the 24 loudest of those, and (the
    // data shows) those are the 24 hardest-struck, so the 6 dropped keys are the 6 softest.
    let mut alone: Vec<(f32, u8)> = (30u8..60)
        .map(|key| {
            let mut one = engine("grand_a", 3).unwrap();
            one.queue(0, Event::NoteOn { key, speed: speed(key) });
            run(&mut one, 4800);
            let mut g = Frame::boxed();
            one.telemetry_frame(&mut g);
            assert_eq!(g.voices, 1);
            (g.voice[0].level, key)
        })
        .collect();
    alone.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut loudest: Vec<u8> = alone[..24].iter().map(|&(_, k)| k).collect();
    let mut listed: Vec<u8> = f.voice[..24].iter().map(|v| v.key + 21).collect();
    loudest.sort_unstable();
    listed.sort_unstable();
    assert_eq!(listed, loudest, "the listed voices are the 24 loudest");
    assert_eq!(listed, (36u8..60).collect::<Vec<_>>(), "which here are the 24 hardest-struck: the 6 softest (keys 30..35) are dropped");
    // (Grand A refit with its level steps, step 104: 4 % at the cut, was over 5 %)
    assert!(alone[23].0 > 1.03 * alone[24].0, "a clear gap at the cut, not a tie: {} vs {}", alone[23].0, alone[24].0);
}

/// C4 on the grand_a with worn strings (false beats at their maximum, seed 7): its listed
/// partials include both polarizations and false-beat copies (probed: 18 horizontal, 24 copies)
#[test]
fn horizontal_partials_and_false_beat_copies_are_flagged() {
    use pianoengine::telemetry::{KIND_COPY, KIND_HORIZ};
    let strike = |false_beats: f64| {
        let inst = Instrument::load(format!("{ROOT}grand_a")).ok()?;
        let mut t = Tuning::default();
        t.wear.false_beats = false_beats;
        t.wear.seed = 7;
        let mut e = Engine::with_tuning(inst, SR, t).expect("engine");
        e.set_params(Params { stops: 3, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 60, speed: 3.0 });
        run(&mut e, 4800);
        let mut f = Frame::boxed();
        e.telemetry_frame(&mut f);
        Some(f)
    };
    let Some(f) = strike(1.0) else { return };
    let v = &f.voice[voice_of(&f, 60, 0).expect("voice for C4")];
    let ps = &v.partials[..v.count as usize];
    assert!(ps.iter().any(|p| p.kind & KIND_HORIZ != 0), "no horizontal partial listed");
    let copies: Vec<_> = ps.iter().filter(|p| p.kind & KIND_COPY != 0).collect();
    assert!(!copies.is_empty(), "no false-beat copy listed");
    assert!(ps.iter().any(|p| p.kind == 0), "vertical base partials are plain");
    for c in &copies {
        // a copy sits a few Hz off its own partial: close to a base partial of the same polarization, never equal
        let nearest = ps
            .iter()
            .filter(|b| b.kind & KIND_COPY == 0 && b.kind & KIND_HORIZ == c.kind & KIND_HORIZ)
            .map(|b| (b.freq - c.freq).abs())
            .fold(f32::MAX, f32::min);
        assert!(nearest > 0.0 && nearest < 0.03 * c.freq, "copy {} Hz: nearest base {} Hz away", c.freq, nearest);
    }
    // without false beats there are no copies, but still horizontal partials
    let Some(g) = strike(0.0) else { return };
    let v = &g.voice[voice_of(&g, 60, 0).unwrap()];
    let ps = &v.partials[..v.count as usize];
    assert!(ps.iter().all(|p| p.kind & KIND_COPY == 0));
    assert!(ps.iter().any(|p| p.kind & KIND_HORIZ != 0));
}

use pianoengine::telemetry::{Region, region_size};

fn phrase(e: &mut Engine) {
    for (t, ev) in [
        (0, Event::NoteOn { key: 48, speed: 4.0 }),
        (0, Event::NoteOn { key: 64, speed: 2.0 }),
        (9600, Event::PedalDown),
        (24000, Event::KeyUp { key: 48 }),
        (30000, Event::NoteOn { key: 72, speed: 3.0 }),
        (48000, Event::PedalUp),
    ] {
        e.queue(t, ev);
    }
}

/// Blocks of irregular sizes, as hosts deliver them.
fn render(e: &mut Engine, total: usize) -> Vec<f32> {
    let sizes = [64usize, 100, 256, 37, 512, 1];
    let (mut l, mut r) = (vec![0.0f32; total], vec![0.0f32; total]);
    let (mut at, mut i) = (0, 0);
    while at < total {
        let n = sizes[i % sizes.len()].min(total - at);
        e.process(&mut l[at..at + n], &mut r[at..at + n]);
        at += n;
        i += 1;
    }
    l.extend(r);
    l
}

#[test]
fn telemetry_does_not_change_the_sound() {
    let (Some(mut a), Some(mut b)) = (engine("grand_a", 3), engine("grand_a", 3)) else { return };
    let mut mem = vec![0u64; region_size() / 8 + 1];
    let region = unsafe { Region::init(mem.as_mut_ptr().cast(), mem.len() * 8) }.unwrap();
    region.set_enabled(true);
    b.attach_telemetry(Some(region), SR);
    phrase(&mut a);
    phrase(&mut b);
    let (ra, rb) = (render(&mut a, 72000), render(&mut b, 72000));
    assert!(ra.iter().zip(&rb).all(|(x, y)| x.to_bits() == y.to_bits()), "telemetry changed the output");
    // and it did deliver something
    let mut f = Frame::boxed();
    assert!(region.read_latest(&mut f));
    assert!(f.rate == SR as f32 && f.counter > 0);
}

#[test]
fn disabled_telemetry_writes_nothing() {
    let Some(mut e) = engine("grand_a", 3) else { return };
    let mut mem = vec![0u64; region_size() / 8 + 1];
    let region = unsafe { Region::init(mem.as_mut_ptr().cast(), mem.len() * 8) }.unwrap();
    e.attach_telemetry(Some(region), SR); // enabled flag not set
    phrase(&mut e);
    render(&mut e, 24000);
    let mut f = Frame::boxed();
    assert!(!region.read_latest(&mut f));
    let (mut cursor, mut out) = (0u64, vec![0.0f32; 512]);
    assert_eq!(region.read_audio(&mut cursor, &mut out), 0);
    // switching it on starts it
    region.set_enabled(true);
    render(&mut e, 24000);
    assert!(region.read_latest(&mut f));
    assert!(region.read_audio(&mut cursor, &mut out) > 0);
}

#[test]
fn frames_come_at_about_30_hz_and_the_audio_is_the_output() {
    let Some(mut e) = engine("grand_a", 3) else { return };
    let mut mem = vec![0u64; region_size() / 8 + 1];
    let region = unsafe { Region::init(mem.as_mut_ptr().cast(), mem.len() * 8) }.unwrap();
    region.set_enabled(true);
    e.attach_telemetry(Some(region), SR);
    e.queue(0, Event::NoteOn { key: 60, speed: 3.0 });
    // one second, reading as a UI would (every ~10 ms of audio): count the distinct frames
    let (mut seen, mut last, mut f) = (0, 0u64, Frame::boxed());
    let (mut l, mut r) = (vec![0.0f32; 480], vec![0.0f32; 480]);
    let (mut cursor, mut tap) = (0u64, Vec::<f32>::new());
    let mut buf = vec![0.0f32; 2 * 4096];
    let mut played = Vec::<f32>::new();
    for _ in 0..100 {
        e.process(&mut l, &mut r);
        played.extend(l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]));
        if region.read_latest(&mut f) && f.counter != last {
            seen += 1;
            last = f.counter;
        }
        let n = region.read_audio(&mut cursor, &mut buf);
        tap.extend_from_slice(&buf[..2 * n]);
    }
    assert!((25..=35).contains(&seen), "{seen} frames in one second");
    // the tap is written where samples enter the host-side FIFO: the host receives tap[k] at k + latency()
    let lat = e.latency();
    let n = played.len() / 2 - lat - 1000;
    let lag = |off: usize| tap[2 * off..2 * off + 2 * n].iter().zip(&played[2 * (off + lat)..]).all(|(a, b)| a == b);
    assert!(tap.len() / 2 >= played.len() / 2 - lat - 500);
    assert!(lag(0), "tap is not the (delayed) output");
    // not vacuous: the compared stretch really carries sound (a struck note, not silence)
    let peak = tap[..2 * n].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak > 1e-4, "the tap is silent: peak {peak}");
    assert!(played[2 * lat..2 * (lat + n)].iter().any(|&x| x != 0.0));
}

#[test]
fn a_full_ring_costs_no_frame_builds_and_counts_the_drops() {
    let Some(mut e) = engine("grand_a", 3) else { return };
    let mut mem = vec![0u64; region_size() / 8 + 1];
    let region = unsafe { Region::init(mem.as_mut_ptr().cast(), mem.len() * 8) }.unwrap();
    region.set_enabled(true);
    e.attach_telemetry(Some(region), SR);
    e.queue(0, Event::NoteOn { key: 60, speed: 3.0 });
    // nobody reads: one second offers ~30 frames; the ring takes its capacity and no more is built
    run(&mut e, SR as usize);
    assert!(!region.has_frame_space());
    assert_eq!(e.telemetry_frames_built(), RING_FRAMES as u64, "frames were built although the ring was full");
    let dropped = region.dropped();
    assert!((20..=30).contains(&dropped), "every skipped frame counts as dropped: {dropped}");
    run(&mut e, SR as usize);
    assert_eq!(e.telemetry_frames_built(), RING_FRAMES as u64);
    assert!(region.dropped() > dropped + 20, "keeps counting while the ring is full");
    // a reader that wakes up gets the newest kept frame, and frames flow again
    let mut f = Frame::boxed();
    assert!(region.read_latest(&mut f));
    let first = f.counter;
    assert!(region.has_frame_space());
    // and a healthy reader (every ~10 ms of audio) sees no further drops
    let dropped = region.dropped();
    let (mut l, mut r) = (vec![0.0f32; 480], vec![0.0f32; 480]);
    for _ in 0..100 {
        e.process(&mut l, &mut r);
        region.read_latest(&mut f);
    }
    assert!(f.counter > first, "frames flow again");
    assert_eq!(region.dropped(), dropped, "no drops while the reader keeps up");
}
