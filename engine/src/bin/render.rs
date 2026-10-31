//! Offline render of an event list written by `proto/demo.py`
//! (`projects/<name>/cache/<part>/demo_events.json`), for comparison with the prototype.
//!
//! Usage: render <instrument dir> <events.json> <out.wav> [sample rate] [--morph DIR=W …]
//!
//! `--morph DIR=W` (repeatable) mixes in the instrument in DIR at weight W, the instrument given
//! first getting the rest (`Instrument::morph`, as midi2wav's).
//!
//! Test options (environment): RENDER_PREROLL, RENDER_BLOCK (below); RENDER_TUNING, a
//! tuning spec `key=value,…` (a4, width, unisons, tuning, strings, board, falsebeats,
//! seed, mic, spacing, distance, direct); RENDER_SWITCH, `t:spec;t:spec…`: at t seconds
//! a new engine for that spec (on top of RENDER_TUNING) takes over (`Engine::adopt`);
//! RENDER_STOPS, the stops bitmask (bit r = register r: 8′, 4′, 8′ II; default 1, the 8′); RENDER_PARAMS,
//! live parameters as midi2wav's --param (e.g. buff=1,lute=1,room=0).

use std::time::Instant;

use pianoengine::{Engine, Event, Instrument, Tuning};
use serde::Deserialize;

#[derive(Deserialize)]
struct Ev {
    t: f64,
    note: u8,
    speed: Option<f64>,
    hold: f64,
}

#[derive(Deserialize)]
struct Events {
    sr: f64,
    t_end: f64,
    pedal: Vec<[f64; 2]>,
    /// optional continuous pedal: (time s, depth 0…1) points, sent as given (like CC64)
    #[serde(default)]
    pedal_curve: Vec<[f64; 2]>,
    events: Vec<Ev>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // the --morph items taken out first, wherever they are
    let (mut args, mut morph) = (Vec::new(), Vec::new());
    let mut it = std::env::args();
    while let Some(a) = it.next() {
        if a == "--morph" {
            morph.push(it.next().ok_or("--morph needs DIR=W")?);
        } else {
            args.push(a);
        }
    }
    if !(4..=5).contains(&args.len()) {
        return Err("usage: render <instrument dir> <events.json> <out.wav> [sample rate] [--morph DIR=W …]".into());
    }
    let sides = pianoengine::spec::morph_sides(&args[1], &morph)?;
    let t0 = Instant::now();
    let inst = Instrument::morph(&sides)?;
    let load = t0.elapsed();
    let evs: Events = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let sr = match args.get(4) {
        Some(s) => s.parse()?,
        None => evs.sr,
    };
    let base = pianoengine::spec::tuning(inst.tuning, &std::env::var("RENDER_TUNING").unwrap_or_default())?;
    let mut engine = Engine::with_tuning(inst, sr, base)?;
    if let Some(stops) = std::env::var("RENDER_STOPS").ok().and_then(|v| v.parse::<u32>().ok()) {
        engine.set_params(pianoengine::Params { stops, ..engine.params() });
    }
    if let Ok(spec) = std::env::var("RENDER_PARAMS") {
        engine.set_params(pianoengine::spec::params(engine.params(), &spec)?);
    }
    let mut switches: Vec<(usize, Tuning)> = Vec::new();
    for sw in std::env::var("RENDER_SWITCH").unwrap_or_default().split(';').filter(|s| !s.is_empty()) {
        let (t, spec) = sw.split_once(':').ok_or(format!("bad switch {sw}"))?;
        switches.push(((t.parse::<f64>()? * sr) as usize, pianoengine::spec::tuning(base, spec)?));
    }

    // sample positions as the prototype computes them; at equal positions: pedal and
    // key-ups before key-downs
    // test options: RENDER_PREROLL frames of silence before t = 0, RENDER_BLOCK frames per
    // process call (as a host would call it)
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok());
    let preroll = env("RENDER_PREROLL").unwrap_or(0);
    let block = env("RENDER_BLOCK");
    let at = |t: f64| (t * sr) as u64 + preroll as u64;
    let mut list: Vec<(u64, u8, Event)> = Vec::new();
    for e in &evs.events {
        let down = match e.speed {
            Some(speed) => Event::NoteOn { key: e.note, speed },
            None => Event::KeyDown { key: e.note },
        };
        list.push((at(e.t), 2, down));
        list.push((at(e.t + e.hold), 1, Event::KeyUp { key: e.note }));
    }
    for p in &evs.pedal {
        list.push((at(p[0]), 0, Event::PedalDown));
        list.push((at(p[1]), 0, Event::PedalUp));
    }
    for p in &evs.pedal_curve {
        list.push((at(p[0]), 0, Event::Pedal { depth: p[1] as f32 }));
    }
    list.sort_by_key(|&(s, o, _)| (s, o));
    for (s, _, ev) in list {
        engine.queue(s, ev);
    }

    let n = (evs.t_end * sr) as usize;
    let lat = engine.latency();
    let mut l = vec![0.0f32; n + lat];
    let mut r = vec![0.0f32; n + lat];
    let t1 = Instant::now();
    if preroll > 0 {
        let (mut sl, mut sr_) = (vec![0.0f32; preroll], vec![0.0f32; preroll]);
        engine.process(&mut sl, &mut sr_);
    }
    let mut build = std::time::Duration::ZERO;
    let mut pos = 0;
    for (at, t) in switches.iter().map(|&(a, t)| (Some(a), Some(t))).chain([(None, None)]) {
        let to = at.unwrap_or(l.len()).min(l.len());
        let (cl, cr) = (&mut l[pos..to], &mut r[pos..to]);
        match block {
            Some(b) => {
                for (cl, cr) in cl.chunks_mut(b).zip(cr.chunks_mut(b)) {
                    engine.process(cl, cr);
                }
            }
            None => engine.process(cl, cr),
        }
        pos = to;
        if let Some(t) = t {
            let tb = Instant::now();
            let mut next = Engine::with_tuning(Instrument::morph(&sides)?, sr, t)?;
            build += tb.elapsed();
            if !next.adopt(&mut engine) {
                return Err("handover refused".into());
            }
            engine = next;
        }
    }
    let render = t1.elapsed() - build;

    let spec = hound::WavSpec { channels: 2, sample_rate: sr as u32, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(&args[3], spec)?;
    for i in lat..n + lat {
        w.write_sample(l[i])?;
        w.write_sample(r[i])?;
    }
    w.finalize()?;
    println!(
        "rendered {:.1} s in {:.2} s ({:.1}× real time), instrument loaded in {:.2} s",
        n as f64 / sr,
        render.as_secs_f64(),
        n as f64 / sr / render.as_secs_f64(),
        load.as_secs_f64()
    );
    Ok(())
}
