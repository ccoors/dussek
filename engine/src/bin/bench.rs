//! Real-time load benchmark: renders synthetic worst-case passages in host-sized buffers
//! and reports the load (render time / audio time), mean and per-buffer worst case.
//!
//! Usage: bench <instrument dir> [buffer frames, default 64]
//! BENCH_STOPS: the stops bitmask (instruments with registers; default 1, the 8′).
//! BENCH_TELEMETRY=1: instead of the scenarios, a pedalled dense chord with the diagnostics
//! on and off: median/max of `telemetry_frame` and the p99 and max block time.
//! BENCH_ONLY=TEXT: only the scenarios whose name holds it; BENCH_SECONDS=S: each cut to S seconds (a
//! profiler's run).
//! BENCH_DRAG=1: the microphones dragged as the plugin plays it (the C API: a worker building
//! engines through a cache at the plugin's build threads, an audio thread paced in real time
//! taking each over), against the same passage held still: the block times re the budget.

use std::time::Instant;

use dussek::{Engine, Event, Instrument};

/// small deterministic generator (xorshift), so every run plays the same notes
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn key(&mut self, lo: u8, hi: u8) -> u8 {
        lo + (self.next() % (hi - lo + 1) as u64) as u8
    }
}

struct Scenario {
    name: &'static str,
    seconds: f64,
    /// (time s, event)
    events: Vec<(f64, Event)>,
}

fn scenarios(speed: [f64; 3]) -> Vec<Scenario> {
    let [_, mf, ff] = speed;
    let mut out = Vec::new();

    // pedal down throughout, four random ff keys every 125 ms over the whole keyboard:
    // after a few seconds every string rings and the sympathetic bank is fully driven
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let mut ev = vec![(0.0, Event::PedalDown)];
    let mut t = 0.05;
    while t < 15.0 {
        for _ in 0..4 {
            let key = rng.key(21, 108);
            ev.push((t, Event::NoteOn { key, speed: ff }));
            ev.push((t + 0.1, Event::KeyUp { key }));
        }
        t += 0.125;
    }
    out.push(Scenario { name: "pedal, 32 ff notes/s, whole keyboard", seconds: 20.0, events: ev });

    // one mf key held: a brass instrument's one player, against its section below
    out.push(Scenario { name: "one mf key held", seconds: 6.0, events: vec![(0.05, Event::NoteOn { key: 60, speed: mf })] });

    // eight-note mf chords every 0.5 s without pedal, held 0.45 s
    let mut ev = Vec::new();
    let mut t = 0.05;
    while t < 15.0 {
        let root = rng.key(28, 72);
        for i in 0..8u8 {
            let key = root + i * 4 % 36;
            ev.push((t, Event::NoteOn { key, speed: mf }));
            ev.push((t + 0.45, Event::KeyUp { key }));
        }
        t += 0.5;
    }
    out.push(Scenario { name: "8-note mf chords, no pedal", seconds: 16.0, events: ev });

    // a pedalled ff glissando over all 88 keys, 20 ms apart, then ringing
    let mut ev = vec![(0.0, Event::PedalDown)];
    for (i, key) in (21..=108u8).enumerate() {
        let t = 0.05 + 0.02 * i as f64;
        ev.push((t, Event::NoteOn { key, speed: ff }));
        ev.push((t + 0.05, Event::KeyUp { key }));
    }
    out.push(Scenario { name: "pedalled ff glissando, 88 keys", seconds: 8.0, events: ev });

    // half-pedalling: the pedal swept up and down through the lift range as a dense CC
    // stream (one value per ms), while 4 mf notes per 0.25 s sound
    let mut ev = Vec::new();
    for ms in 0..15000 {
        let t = ms as f64 / 1000.0;
        let depth = 0.5 + 0.5 * (2.0 * std::f64::consts::PI * t / 1.5).sin();
        ev.push((t, Event::Pedal { depth: depth as f32 }));
    }
    let mut t = 0.05;
    while t < 15.0 {
        for _ in 0..4 {
            let key = rng.key(28, 96);
            ev.push((t, Event::NoteOn { key, speed: mf }));
            ev.push((t + 0.2, Event::KeyUp { key }));
        }
        t += 0.25;
    }
    out.push(Scenario { name: "half-pedal sweeps (1 kHz CC), mf notes", seconds: 16.0, events: ev });
    out
}

/// Time of the buffers covering one ff strike (hammer contact, mode start), per key,
/// minus the same buffers without a strike.
fn strike_cost(inst: Instrument, sr: f64, ff: f64) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new(inst, sr)?;
    let frames = 64;
    let (mut l, mut r) = (vec![0.0f32; frames], vec![0.0f32; frames]);
    let mut now = 0u64;
    let mut run = |engine: &mut Engine, now: &mut u64, buffers: usize| -> f64 {
        let t0 = Instant::now();
        for _ in 0..buffers {
            engine.process(&mut l, &mut r);
        }
        *now += (buffers * frames) as u64;
        t0.elapsed().as_secs_f64()
    };
    let mut worst = (0.0, 0);
    let mut line = String::new();
    for key in 21..=108u8 {
        let mut best = f64::INFINITY;
        for _ in 0..5 {
            engine.queue(now, Event::NoteOn { key, speed: ff });
            let t = run(&mut engine, &mut now, 3);
            engine.queue(now, Event::KeyUp { key });
            // let it die out (undamped keys ring long; the cull ends them)
            run(&mut engine, &mut now, 48000 * 8 / frames);
            best = best.min(t);
        }
        let us = best * 1e6;
        if us > worst.0 {
            worst = (us, key);
        }
        if key % 12 == 0 || key == 21 || key == 108 {
            line += &format!("  {key}: {us:.0} µs");
        }
    }
    println!("ff strike, 3 buffers of 64 incl. contact (best of 5):{line}; worst {:.0} µs at key {}", worst.0, worst.1);
    Ok(())
}

struct Run {
    /// seconds per buffer
    times: Vec<f64>,
    total: f64,
    voices: usize,
    modes: usize,
    sym: usize,
}

fn run(inst: Instrument, sr: f64, sc: &Scenario, frames: usize, mic: dussek::mic::MicPosition) -> Result<Run, Box<dyn std::error::Error>> {
    let tuning = dussek::Tuning { mic, ..Default::default() };
    let mut engine = Engine::with_tuning(inst, sr, tuning)?;
    if let Some(stops) = std::env::var("BENCH_STOPS").ok().and_then(|v| v.parse::<u32>().ok()) {
        engine.set_params(dussek::Params { stops, ..engine.params() });
    }
    let mut evs = sc.events.clone();
    evs.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (t, e) in evs {
        engine.queue((t * sr) as u64, e);
    }
    let n = (sc.seconds * sr) as usize;
    let (mut l, mut r) = (vec![0.0f32; frames], vec![0.0f32; frames]);
    let mut out = Run { times: Vec::with_capacity(n / frames), total: 0.0, voices: 0, modes: 0, sym: 0 };
    let start = Instant::now();
    for _ in 0..n / frames {
        let t0 = Instant::now();
        engine.process(&mut l, &mut r);
        out.times.push(t0.elapsed().as_secs_f64());
        let st = engine.stats();
        out.voices = out.voices.max(st.voices);
        out.modes = out.modes.max(st.modes);
        out.sym = out.sym.max(st.sympathetic);
    }
    out.total = start.elapsed().as_secs_f64();
    Ok(out)
}

/// Dense pedalled chord (all 88 keys, ff), settled for 1 s; then the cost of one telemetry
/// frame, and the 64-frame block times of the next 3 s with the telemetry off and on,
/// alternating (a reader drains the region every ~10 ms, as the UI does).
fn telemetry_bench(inst: Instrument, sr: f64, ff: f64) -> Result<(), Box<dyn std::error::Error>> {
    use dussek::telemetry::{Frame, Region, region_size};
    let mut e = Engine::new(inst, sr)?;
    e.queue(0, Event::PedalDown);
    for key in 21..=108u8 {
        e.queue(0, Event::NoteOn { key, speed: ff });
    }
    let (mut l, mut r) = (vec![0.0f32; 64], vec![0.0f32; 64]);
    for _ in 0..sr as usize / 64 {
        e.process(&mut l, &mut r);
    }
    let mut f = Frame::boxed();
    let mut t = Vec::new();
    for _ in 0..300 {
        let t0 = Instant::now();
        e.telemetry_frame(&mut f);
        t.push(t0.elapsed().as_secs_f64() * 1e6);
    }
    t.sort_by(f64::total_cmp);
    println!("telemetry_frame: median {:.0} µs, max {:.0} µs ({} live modes, {} voices listed)", t[t.len() / 2], t[t.len() - 1], e.stats().modes, f.voices);
    let mut mem = vec![0u64; region_size() / 8 + 1];
    // SAFETY: 8-aligned memory of at least region_size() bytes that outlives the engine's attachment
    let region = unsafe { Region::init(mem.as_mut_ptr().cast(), mem.len() * 8) }.ok_or("region")?;
    // alternate every 50 blocks, so that both see the same decaying chord
    e.attach_telemetry(Some(region), sr);
    let mut b = [Vec::new(), Vec::new()];
    for i in 0..(4.0 * sr) as usize / 64 {
        let on = i / 50 % 2;
        region.set_enabled(on == 1);
        let t0 = Instant::now();
        e.process(&mut l, &mut r);
        b[on].push(t0.elapsed().as_secs_f64() * 1e6);
        if i % 8 == 0 {
            region.read_latest(&mut f);
        }
    }
    for (on, b) in b.iter_mut().enumerate() {
        b.sort_by(f64::total_cmp);
        println!("telemetry {}: block of 64: median {:.0} µs, p99 {:.0} µs, max {:.0} µs (budget {:.0} µs)", ["off", "on "][on], b[b.len() / 2], b[b.len() * 99 / 100], b[b.len() - 1], 64e6 / sr);
    }
    Ok(())
}

/// Environment: BENCH_STRIKE=1 measures the strike cost per key; BENCH_REPEAT=n runs every
/// scenario n times and keeps each buffer's fastest time (removes system noise);
/// BENCH_TOP=1 lists the slowest buffers with the events near them.
/// One run of the drag benchmark: `drag` the free pair moves 1 cm per build, else it stands; the
/// block times (s) of a pedalled passage over `secs`, the builds handed over.
fn drag_run(dir: &str, frames: usize, secs: f64, drag: bool) -> (Vec<f64>, usize, Vec<f64>, Vec<bool>) {
    use dussek::ffi::*;
    use std::ffi::CString;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;
    let sr = 48000.0;
    let tuning = |x: f64| {
        // SAFETY: plain numbers; every field set below or meant as 0
        let mut t: PeTuning = unsafe { std::mem::zeroed() };
        t.a4_hz = 440.0;
        t.unison_width = 1.0;
        t.mic_position = 5;
        t.mic_spacing = 0.5;
        t.bolt_pos = 0.31;
        t.lid = 1.0;
        t.sustain = 1.0;
        t.mic_setup = PeMicSetup::default();
        t.mic_setup.pos = [[x - 0.25, -1.25, 0.45], [x + 0.25, -1.25, 0.45]];
        t
    };
    let cdir = CString::new(dir).unwrap();
    let weights = [1.0f32];
    let cache = Mutex::new(SendPtr(pe_cache_new()));
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get()) / 2;
    pe_set_build_threads(cores.saturating_sub(1).max(1) as u32);
    let build = |x: f64| {
        let t = tuning(x);
        let dirs = [cdir.as_ptr()];
        let c = cache.lock().unwrap();
        // SAFETY: valid arrays and tuning, the cache owned here
        SendPtr(unsafe { pe_create_morph(dirs.as_ptr(), weights.as_ptr(), 1, sr, &t, std::ptr::null(), c.0, std::ptr::null_mut(), 0) })
    };
    let first = build(0.0);
    let offered: Mutex<Option<SendPtr>> = Mutex::new(None);
    let retired: Mutex<Vec<SendPtr>> = Mutex::new(Vec::new());
    let stop = AtomicBool::new(false);
    let handed = AtomicUsize::new(0);
    let mut times = Vec::new();
    let mut adopts = Vec::new();
    let mut with_handover = Vec::new();
    std::thread::scope(|s| {
        if drag {
            s.spawn(|| {
                let mut x = 0.0;
                while !stop.load(Ordering::Relaxed) {
                    x += 0.01;
                    let e = build(x);
                    if let Some(old) = offered.lock().unwrap().replace(e) {
                        // SAFETY: never handed over
                        unsafe { pe_destroy(old.0) };
                    }
                    for r in retired.lock().unwrap().drain(..) {
                        // SAFETY: handed back by the audio thread
                        unsafe { pe_destroy(r.0) };
                    }
                    std::thread::sleep(std::time::Duration::from_millis(15)); // a mouse's pace
                }
            });
        }
        let mut e = first;
        let (mut l, mut r) = (vec![0.0f32; frames], vec![0.0f32; frames]);
        let block = frames as f64 / sr;
        let blocks = (secs / block) as usize;
        let t0 = Instant::now();
        let mut rng = Rng(0x2545f4914f6cdd1d);
        // SAFETY: e is a live engine owned by this thread
        unsafe { pe_pedal(e.0, 0, true) };
        for b in 0..blocks {
            // pace: wait for this block's deadline as a sound card would
            let due = b as f64 * block;
            while t0.elapsed().as_secs_f64() < due {
                std::hint::spin_loop();
            }
            let start = Instant::now();
            let mut handed_here = false;
            if std::env::var_os("BENCH_DRAG_KEEP").is_none()
                && let Ok(mut o) = offered.try_lock()
                && let Some(next) = o.take()
            {
                // SAFETY: both live; the old one goes back to the worker
                let a = Instant::now();
                unsafe { pe_adopt(next.0, e.0) };
                adopts.push(a.elapsed().as_secs_f64());
                handed_here = true;
                retired.lock().unwrap().push(e);
                e = next;
                handed.fetch_add(1, Ordering::Relaxed);
            }
            if b % 24 == 0 {
                for _ in 0..4 {
                    let key = rng.key(21, 108);
                    // SAFETY: live engine
                    unsafe { pe_note_on(e.0, 0, key, 100) };
                }
            }
            // SAFETY: live engine, buffers of `frames`
            unsafe { pe_process(e.0, std::ptr::null(), l.as_mut_ptr(), r.as_mut_ptr(), frames as u32) };
            times.push(start.elapsed().as_secs_f64());
            with_handover.push(handed_here);
        }
        stop.store(true, Ordering::Relaxed);
        retired.lock().unwrap().push(e);
    });
    for r in retired.into_inner().unwrap().into_iter().chain(offered.into_inner().unwrap()) {
        // SAFETY: owned, not used afterwards
        unsafe { pe_destroy(r.0) };
    }
    // SAFETY: from pe_cache_new, no engine building
    unsafe { pe_cache_free(cache.into_inner().unwrap().0 as *mut _) };
    (times, handed.into_inner(), adopts, with_handover)
}

/// a raw pointer passed between the benchmark's threads (each engine used by one at a time)
#[derive(Clone, Copy)]
struct SendPtr<T = dussek::ffi::PeEngine>(*mut T);
// SAFETY: the benchmark hands each pointer from one thread to the other, never shared
unsafe impl<T> Send for SendPtr<T> {}

fn drag_bench(dir: &str, frames: usize) {
    let budget = frames as f64 / 48000.0;
    for drag in [false, true] {
        let (mut t, handed, mut adopts, with) = drag_run(dir, frames, 12.0, drag);
        let over = t.iter().filter(|&&x| x > budget).count();
        let over_at = t.iter().zip(&with).filter(|&(&x, &w)| w && x > budget).count();
        t.sort_by(f64::total_cmp);
        let q = |p: f64| t[((t.len() - 1) as f64 * p) as usize] / budget * 100.0;
        println!(
            "{}: {} blocks, {handed} handovers; block time re budget: median {:.0} %, p99 {:.0} %, p99.9 {:.0} %, max {:.0} %; over budget {over}",
            if drag { "dragging" } else { "still   " },
            t.len(),
            q(0.5),
            q(0.99),
            q(0.999),
            q(1.0)
        );
        if !adopts.is_empty() {
            adopts.sort_by(f64::total_cmp);
            println!(
                "  the handover itself: median {:.0} %, max {:.0} % of the budget; over budget in a handover's block {over_at}",
                adopts[adopts.len() / 2] / budget * 100.0,
                adopts[adopts.len() - 1] / budget * 100.0
            );
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err("usage: bench <instrument dir> [buffer frames]".into());
    }
    let frames: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(64);
    let repeat: usize = std::env::var("BENCH_REPEAT").ok().map(|s| s.parse()).transpose()?.unwrap_or(1);
    let sr = 48000.0;
    let speed = Instrument::load(&args[1])?.velocity_speed;
    // BENCH_MIC=0/1/2: microphone position (zones from 1 on)
    let mic = dussek::mic::MicPosition::from_index(std::env::var("BENCH_MIC").ok().and_then(|s| s.parse().ok()).unwrap_or(0));
    if std::env::var_os("BENCH_DRAG").is_some() {
        drag_bench(&args[1], frames);
        return Ok(());
    }
    if std::env::var_os("BENCH_TELEMETRY").is_some() {
        return telemetry_bench(Instrument::load(&args[1])?, sr, speed[2]);
    }
    if std::env::var_os("BENCH_STRIKE").is_some() {
        strike_cost(Instrument::load(&args[1])?, sr, speed[2])?;
    }
    let buf_s = frames as f64 / sr;
    let only = std::env::var("BENCH_ONLY").ok();
    let cut: Option<f64> = std::env::var("BENCH_SECONDS").ok().and_then(|v| v.parse().ok());
    for mut sc in scenarios(speed).into_iter().filter(|s| only.as_ref().is_none_or(|o| s.name.contains(o.as_str()))) {
        if let Some(c) = cut {
            sc.seconds = sc.seconds.min(c);
        }
        let mut best = run(Instrument::load(&args[1])?, sr, &sc, frames, mic)?;
        for _ in 1..repeat {
            let r = run(Instrument::load(&args[1])?, sr, &sc, frames, mic)?;
            best.times.iter_mut().zip(&r.times).for_each(|(a, b)| *a = a.min(*b));
            best.total = best.total.min(r.total);
        }
        let times = &best.times;
        if std::env::var_os("BENCH_TOP").is_some() {
            let mut idx: Vec<usize> = (0..times.len()).collect();
            idx.sort_by(|&a, &b| times[b].total_cmp(&times[a]));
            for &i in idx.iter().take(8) {
                let t = (i * frames) as f64 / sr;
                let evs: Vec<String> = sc
                    .events
                    .iter()
                    .filter(|(te, _)| *te >= t - 0.01 && *te < t + buf_s)
                    .map(|(te, e)| format!("{te:.4} {e:?}"))
                    .collect();
                println!("  slow buffer at {t:.4} s: {:.0} %  {}", times[i] / buf_s * 100.0, evs.join("; "));
            }
        }
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize] / buf_s * 100.0;
        println!(
            "{:40} load mean {:5.1} %, per buffer p99 {:6.1} %, p99.9 {:6.1} %, max {:6.1} %",
            sc.name,
            best.total / sc.seconds * 100.0,
            pct(0.99),
            pct(0.999),
            pct(1.0)
        );
        println!("{:40} peak: {} voices, {} string modes, {} sympathetic resonators", "", best.voices, best.modes, best.sym);
    }
    Ok(())
}
