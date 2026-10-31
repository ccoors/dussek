//! Render a Standard MIDI file to a stereo WAV with the piano engine (and, with `--video`, a video
//! of it). `midi2wav --help` lists the options (`Cli` below); docs/engine.md describes them. The
//! `midi2wav` binary and the standalone's `render` (`pe_cli`, through the C interface) run `main`.
//!
//! The engine is sequential, so the piece is cut into time segments (`--jobs`), each rendered by
//! its own engine that first replays at least `--warmup` seconds of the music before it, from the
//! latest moment before that when the pedal is up and no key is held (at most `--maxwarm` ahead).

mod video;

use std::time::{Duration, Instant};

use std::ffi::OsString;
use std::io::Write;

use clap::{CommandFactory, Parser};

use crate::midi::{self, Action};
use crate::{Engine, Event, Instrument, Mix, Params, RadiationCache, Tuning};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Opts {
    rate: f64,
    tail: f64,
    curve: crate::velocity::VelocityCurve,
    param: String,
    tuning: String,
    bits: u16,
    normalize: Option<f64>,
    scl: Option<String>,
    mics: Vec<String>,
    wall: Option<f64>,
    key: usize,
    jobs: usize,
    warmup: f64,
    maxwarm: f64,
    latch: bool,
    stems: bool,
    morph: Vec<String>,
    felt: String,
    video: Option<video::VideoOpts>,
}

/// The command line (`clap`): `-h` the summary, `--help` everything, with the keys of `--param` and
/// `--tuning`. Every value is checked here, before anything is loaded or rendered.
#[derive(Parser)]
#[command(
    name = "midi2wav",
    version,
    about = "Render a Standard MIDI file to a stereo WAV with the piano engine (and, with --video, a video of it)",
    after_help = "Examples:\n  midi2wav instruments/grand_a song.mid song.wav\n  midi2wav instruments/grand_a song.mid song.wav --param room=0.5 --tuning a4=442 --normalize\n  midi2wav instruments/grand_a song.mid song.wav --video song.mp4\n\nThe plugin's Copy CLI (the ⋯ menu) writes a line for its current settings. `--help` lists every key of --param and --tuning.",
    max_term_width = 100
)]
struct Cli {
    /// The instrument's directory (holding its instrument.json), e.g. instruments/grand_a
    #[arg(value_name = "INSTRUMENT")]
    instrument: String,
    /// The Standard MIDI file to play
    #[arg(value_name = "IN.mid")]
    midi: String,
    /// The WAV file to write
    #[arg(value_name = "OUT.wav")]
    out: String,

    /// The sample rate (Hz, at least 8000)
    #[arg(long, value_name = "HZ", default_value_t = 48000.0, value_parser = rate, help_heading = "Rendering")]
    rate: f64,
    /// Seconds rendered after the last event
    #[arg(long, value_name = "S", default_value_t = 6.0, value_parser = seconds, help_heading = "Rendering")]
    tail: f64,

    /// Live parameters as key=value pairs, e.g. room=0.5,volume=0.8
    #[arg(long, value_name = "KEY=VALUE,…", value_parser = param_spec, long_help = param_help(), help_heading = "Playing")]
    param: Option<String>,
    /// The velocity curve: in:out pairs, e.g. 1:1,64:80,127:127 (the plugin's Touch tab)
    #[arg(long, value_name = "IN:OUT,…", value_parser = velocity_curve, help_heading = "Playing")]
    velocity_curve: Option<String>,
    /// The sustain pedal held down throughout, as the plugin's Sustain latch
    ///
    /// The file's own sustain pedal (CC64) then changes nothing.
    #[arg(long, help_heading = "Playing")]
    latch: bool,
    /// The player's felt, as the plugin's Felt tab saves it
    ///
    /// E.g. "s:21=6,60=0;p:;c:;d:". On a hammer piano it and --param hardness go through the build, every key's
    /// loudness kept (a few seconds).
    #[arg(long, value_name = "TEXT", value_parser = felt, help_heading = "Playing")]
    felt: Option<String>,

    /// Compiled settings as key=value pairs, e.g. a4=442,temperament=3,mic=1
    #[arg(long, value_name = "KEY=VALUE,…", value_parser = tuning_spec, long_help = tuning_help(), help_heading = "Instrument and tuning")]
    tuning: Option<String>,
    /// A Scala scale (.scl) as the temperament
    ///
    /// Any note count; a key map beside it of the same name (.kbm) lays it on the keys.
    #[arg(long, value_name = "FILE", help_heading = "Instrument and tuning")]
    scl: Option<String>,
    /// The Scala scale's first note on pitch class N (0 C … 11 B), A kept at the concert pitch
    #[arg(long, value_name = "N", default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=11), requires = "scl", help_heading = "Instrument and tuning")]
    key: u8,
    /// Mix in the instrument in DIR at weight W (repeatable)
    ///
    /// The INSTRUMENT keeps the rest, 1 − ΣW: an instrument between hammer-strung pianos.
    #[arg(long, value_name = "DIR=W", value_parser = morph, help_heading = "Instrument and tuning")]
    morph: Vec<String>,

    /// A free microphone, given twice: left, then right
    ///
    /// X, Y, Z in meters from the board's center; PATTERN 0 omni … 0.5 cardioid … 1 figure-8; aimed at the board
    /// unless YAW and PITCH (degrees) are given. It overrides --tuning mic=…
    #[arg(long, value_name = "X,Y,Z[,PATTERN[,YAW[,PITCH]]]", value_parser = mic, allow_hyphen_values = true, help_heading = "Microphones")]
    mic: Vec<String>,
    /// The wall behind the microphones at Y meters (the --mic setup only)
    #[arg(long, value_name = "Y", value_parser = number, allow_negative_numbers = true, requires = "mic", help_heading = "Microphones")]
    wall: Option<f64>,

    /// Scale the whole file to a peak of DBFS (default −1)
    ///
    /// One gain, the dynamics untouched. Without it the engine's calibration stands: a median ff single note peaks
    /// at −15 dBFS.
    #[arg(long, value_name = "DBFS", num_args = 0..=1, default_missing_value = "-1", allow_negative_numbers = true, value_parser = peak, help_heading = "Output")]
    normalize: Option<f64>,
    /// The sample format: 16 or 24 bits (integer) or 32 (float)
    #[arg(long, value_name = "16|24|32", default_value_t = 32, value_parser = bits, help_heading = "Output")]
    bits: u16,
    /// The parts as their own files beside OUT.wav
    ///
    /// OUT.strings.wav, OUT.ringing.wav, OUT.room.wav, OUT.noises.wav: they sum to OUT.wav with its gain; each costs
    /// a render.
    #[arg(long, help_heading = "Output")]
    stems: bool,

    /// The threads (auto: up to one per core, as the piece's length allows)
    ///
    /// The piece is cut into N segments, each rendered by its own engine that first replays the music before it;
    /// 1 renders it in one go (the reference).
    #[arg(long, value_name = "N|auto", default_value = "auto", value_parser = jobs, help_heading = "Parallel render")]
    jobs: usize,
    /// The least seconds replayed before each segment
    ///
    /// From the latest moment before them with every damper down.
    #[arg(long, value_name = "S", default_value_t = 12.0, value_parser = seconds, help_heading = "Parallel render")]
    warmup: f64,
    /// The most seconds replayed before a segment
    #[arg(long, value_name = "S", default_value_t = 40.0, value_parser = seconds, help_heading = "Parallel render")]
    maxwarm: f64,

    /// Also a video of the piece, with OUT.wav as its sound (needs ffmpeg)
    ///
    /// The strings, dampers, hammers, keys and pedals as the engine plays them: the plugin's Visualization panel.
    #[arg(long, value_name = "OUT", help_heading = "Video")]
    video: Option<String>,
    /// The video's size (even sides)
    #[arg(long, value_name = "WxH", default_value = "1920x1080", value_parser = video::parse_size, requires = "video", help_heading = "Video")]
    size: (u32, u32),
    /// The video's frame rate (1 … 240)
    #[arg(long, value_name = "N", default_value_t = 60.0, value_parser = fps, requires = "video", help_heading = "Video")]
    fps: f64,
    /// ffmpeg's output options, in place of the default
    #[arg(long, value_name = "ARGS", allow_hyphen_values = true, long_help = ffmpeg_help(), requires = "video", help_heading = "Video")]
    ffmpeg: Option<String>,
    /// No background: its alpha kept for compositing
    ///
    /// The strings' light keeps an alpha as bright as it is, the keys and hammers are solid. By default ProRes 4444
    /// in a .mov.
    #[arg(long, requires = "video", help_heading = "Video")]
    transparent: bool,
}

fn number(s: &str) -> std::result::Result<f64, String> {
    let x: f64 = s.trim().parse().map_err(|_| format!("`{s}` is not a number"))?;
    if x.is_finite() { Ok(x) } else { Err(format!("`{s}` is not a finite number")) }
}

fn rate(s: &str) -> std::result::Result<f64, String> {
    let x = number(s)?;
    if (8000.0..=768000.0).contains(&x) { Ok(x) } else { Err("from 8000 to 768000 Hz".into()) }
}

fn seconds(s: &str) -> std::result::Result<f64, String> {
    let x = number(s)?;
    if x >= 0.0 { Ok(x) } else { Err("seconds, 0 or more".into()) }
}

fn peak(s: &str) -> std::result::Result<f64, String> {
    // a file name after a bare --normalize lands here
    let x = number(s).map_err(|e| format!("{e}: the peak in dBFS, e.g. -1 (a bare --normalize takes the next argument: put it after the files, or write --normalize=-1)"))?;
    if x <= 0.0 { Ok(x) } else { Err("the peak in dBFS, 0 or below (e.g. -1)".into()) }
}

fn fps(s: &str) -> std::result::Result<f64, String> {
    let x = number(s)?;
    if (1.0..=240.0).contains(&x) { Ok(x) } else { Err("1 to 240 frames a second".into()) }
}

fn bits(s: &str) -> std::result::Result<u16, String> {
    match s.trim() {
        "16" => Ok(16),
        "24" => Ok(24),
        "32" => Ok(32),
        _ => Err("16, 24 or 32".into()),
    }
}

fn jobs(s: &str) -> std::result::Result<usize, String> {
    match s.trim() {
        "auto" => Ok(0),
        n => n.parse::<usize>().ok().filter(|&n| n > 0).ok_or_else(|| "a number of threads (1 or more) or `auto`".into()),
    }
}

fn param_spec(s: &str) -> std::result::Result<String, String> {
    crate::spec::params(Params::default(), s).map(|_| s.to_string()).map_err(|e| e.to_string())
}

fn tuning_spec(s: &str) -> std::result::Result<String, String> {
    crate::spec::tuning(Tuning::default(), s).map(|_| s.to_string()).map_err(|e| e.to_string())
}

fn velocity_curve(s: &str) -> std::result::Result<String, String> {
    crate::velocity::VelocityCurve::try_parse(s)
        .map(|_| s.to_string())
        .ok_or_else(|| "in:out pairs between 1:1 and 127:127, the ins rising and the outs never falling, e.g. 1:1,64:80,127:127".into())
}

fn felt(s: &str) -> std::result::Result<String, String> {
    crate::felt::FeltEdit::try_parse(s)
        .map(|_| s.to_string())
        .ok_or_else(|| "groups s, p, c, d of key=bend points (s:21=6,60=0;p:;c:;d:), the keys rising within 21–108, at most 8 a group".into())
}

fn mic(s: &str) -> std::result::Result<String, String> {
    crate::spec::mic(s).map(|_| s.to_string()).map_err(|e| e.to_string())
}

fn morph(s: &str) -> std::result::Result<String, String> {
    let (_, w) = s.rsplit_once('=').ok_or("DIR=W, e.g. instruments/grand_d=0.5")?;
    let w = number(w)?;
    if (0.0..=1.0).contains(&w) { Ok(s.to_string()) } else { Err("the weight W from 0 to 1".into()) }
}

/// a key list for `--help`, one per line
fn key_list(keys: &[(&str, &str)]) -> String {
    let width = keys.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    keys.iter().map(|(k, d)| format!("  {k:width$}  {d}")).collect::<Vec<_>>().join("\n")
}

fn ffmpeg_help() -> String {
    format!(
        "ffmpeg's output options, split at spaces, in place of the default:\n  \"{}\" (H.264 and AAC),\n  with --transparent \"{}\" (ProRes 4444).\nE.g. AV1 and Opus for a .webm:\n  \"-c:v libsvtav1 -crf 30 -preset 6 -pix_fmt yuv420p -c:a libopus -b:a 192k\"",
        video::DEFAULT_FFMPEG,
        video::TRANSPARENT_FFMPEG
    )
}

fn param_help() -> String {
    format!("Live parameters as key=value pairs, e.g. room=0.5,volume=0.8. The keys:\n{}", key_list(crate::spec::PARAM_KEYS))
}

fn tuning_help() -> String {
    let temperaments: Vec<String> = crate::temperament::NAMES.iter().enumerate().map(|(i, n)| format!("{i} {n}")).collect();
    format!(
        "Compiled settings as key=value pairs, e.g. a4=442,temperament=3,mic=1. The keys:\n{}\nThe temperaments: {}.",
        key_list(crate::spec::TUNING_KEYS),
        temperaments.join(", ")
    )
}

/// The command line, checked as a whole (what `clap` cannot check value by value), as the options.
fn parse_args<I, T>(args: I) -> std::result::Result<(String, String, String, Opts), clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let c = Cli::try_parse_from(args)?;
    let fail = |kind, msg: &str| Err(Cli::command().error(kind, msg));
    if !c.mic.is_empty() && c.mic.len() != 2 {
        return fail(clap::error::ErrorKind::WrongNumberOfValues, "--mic is given twice: the left microphone, then the right");
    }
    if c.maxwarm < c.warmup {
        return fail(clap::error::ErrorKind::ValueValidation, "--maxwarm is shorter than --warmup");
    }
    let o = Opts {
        rate: c.rate,
        tail: c.tail,
        curve: c.velocity_curve.as_deref().map_or(crate::velocity::VelocityCurve::STRAIGHT, |s| crate::velocity::VelocityCurve::try_parse(s).unwrap()),
        param: c.param.unwrap_or_default(),
        tuning: c.tuning.unwrap_or_default(),
        bits: c.bits,
        normalize: c.normalize,
        scl: c.scl,
        mics: c.mic,
        wall: c.wall,
        key: c.key as usize,
        jobs: c.jobs,
        warmup: c.warmup,
        maxwarm: c.maxwarm,
        latch: c.latch,
        stems: c.stems,
        morph: c.morph,
        felt: c.felt.unwrap_or_default(),
        video: c.video.map(|path| video::VideoOpts { path, width: c.size.0, height: c.size.1, fps: c.fps, ffmpeg: c.ffmpeg, transparent: c.transparent }),
    };
    Ok((c.instrument, c.midi, c.out, o))
}

/// An event on the global sample axis.
type Timed = (u64, Event);

/// Renders output frames `from..to` (global sample positions, the engine's latency
/// already compensated) with an engine that starts at the earlier `start`; the music
/// before `from` only warms the strings up.
#[allow(clippy::too_many_arguments)]
fn render_segment(
    inst: &Instrument,
    tuning: Tuning,
    params: Params,
    mix: Mix,
    sr: f64,
    events: &[Timed],
    start: u64,
    from: u64,
    to: u64,
) -> Result<Vec<[f32; 2]>> {
    let mut engine = Engine::with_cache(inst.clone(), sr, tuning, &mut RadiationCache::default())?;
    engine.set_params(params);
    engine.set_mix(mix);
    // the state at `start`: keys held and the pedal, as of the events before it
    let mut down = [false; 128];
    let (mut pedal, mut sostenuto, mut soft) = (0.0f32, false, 0.0f32);
    let (mut lifted, mut cycles) = (false, 0usize);
    // the Bebung's controllers and the pitch wheel (a sound-off keeps them, as the engine does)
    let (mut wheel, mut chan, mut pressure, mut bend) = (0.0f32, 0.0f32, [0.0f32; 128], 0.0f32);
    for &(_, ev) in events.iter().take_while(|e| e.0 < start) {
        match ev {
            Event::ModWheel { value } => wheel = value,
            Event::PitchBend { value } => bend = value,
            Event::ChannelPressure { value } => chan = value,
            Event::KeyPressure { key, value } => pressure[key as usize] = value,
            Event::NoteOn { key, .. } | Event::KeyDown { key } => down[key as usize] = true,
            Event::KeyUp { key } => down[key as usize] = false,
            Event::ReleaseVelocity { .. } => {}
            Event::Pedal { depth } => pedal = depth,
            Event::PedalDown => pedal = 1.0,
            Event::PedalUp => pedal = 0.0,
            Event::SostenutoDown => sostenuto = true,
            Event::SostenutoUp => sostenuto = false,
            Event::SoftPedal { depth } => soft = depth,
            Event::Silence => {
                (down, pedal, sostenuto, soft) = ([false; 128], 0.0, false, 0.0);
            }
        }
        // the engine alternates its pedal-thump takes once per lift-and-drop cycle
        let now_lifted = crate::engine::pedal_engage(pedal as f64) <= 0.5;
        cycles += (lifted && !now_lifted) as usize;
        lifted = now_lifted;
    }
    engine.set_pedal_cycles(cycles);
    for (key, _) in down.iter().enumerate().filter(|d| *d.1) {
        engine.queue(0, Event::KeyDown { key: key as u8 });
    }
    if pedal > 0.0 {
        engine.queue(0, Event::Pedal { depth: pedal });
    }
    // a sostenuto held across the start catches what is lifted then (the keys held and
    // the pedal); a key it caught earlier and that is up by now is not known
    if sostenuto {
        engine.queue(0, Event::SostenutoDown);
    }
    if soft > 0.0 {
        engine.queue(0, Event::SoftPedal { depth: soft });
    }
    if wheel > 0.0 {
        engine.queue(0, Event::ModWheel { value: wheel });
    }
    if chan > 0.0 {
        engine.queue(0, Event::ChannelPressure { value: chan });
    }
    for (key, &value) in pressure.iter().enumerate().filter(|p| *p.1 > 0.0) {
        engine.queue(0, Event::KeyPressure { key: key as u8, value });
    }
    if bend != 0.0 {
        engine.queue(0, Event::PitchBend { value: bend });
    }
    for &(t, ev) in events.iter().filter(|e| e.0 >= start) {
        engine.queue(t - start, ev);
    }
    let lat = engine.latency() as u64;
    let n = (to - start + lat) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    engine.process(&mut l, &mut r);
    let skip = (from - start + lat) as usize;
    Ok(l[skip..].iter().zip(&r[skip..]).map(|(&l, &r)| [l, r]).collect())
}

/// The command line `args` (the first the program's name): the exit status (0, 1 a failed render,
/// 2 a usage error; --help and --version print and return 0). Never exits the process.
pub fn main<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let code = match parse_args(args) {
        Err(e) => {
            let _ = e.print();
            e.exit_code()
        }
        Ok((dir, midi_path, out_path, o)) => match run(&dir, &midi_path, &out_path, o) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
    };
    let _ = std::io::stdout().flush();
    code
}

/// `dir` an instrument's directory: its instrument.json there, or an error that says so
fn instrument_dir(dir: &str, what: &str) -> Result<()> {
    let path = std::path::Path::new(dir);
    if !path.is_dir() {
        return Err(format!("{what} `{dir}`: no such directory (an instrument's directory, e.g. instruments/grand_a)").into());
    }
    if !path.join("instrument.json").is_file() {
        return Err(format!("{what} `{dir}`: no instrument.json in it (an instrument's directory, e.g. instruments/grand_a)").into());
    }
    Ok(())
}

/// the folder a file goes into exists (checked before the render, not after it)
fn writable(path: &str, what: &str) -> Result<()> {
    match std::path::Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(folder) if !folder.is_dir() => Err(format!("{what} `{path}`: the folder `{}` does not exist", folder.display()).into()),
        _ => Ok(()),
    }
}

fn run(dir: &str, midi_path: &str, out_path: &str, o: Opts) -> Result<()> {
    let out_path = out_path.to_string();
    writable(&out_path, "OUT.wav")?;
    if let Some(v) = &o.video {
        writable(&v.path, "--video")?;
    }
    let t0 = Instant::now();
    let bytes = std::fs::read(midi_path).map_err(|e| format!("cannot read the MIDI file `{midi_path}`: {e}"))?;
    let score = midi::parse(&bytes).map_err(|e| format!("`{midi_path}` is not a Standard MIDI file: {e}"))?;
    if score.events.is_empty() {
        return Err(format!("the MIDI file `{midi_path}` has no notes on the piano's keys").into());
    }
    instrument_dir(dir, "INSTRUMENT")?;
    for side in &o.morph {
        instrument_dir(side.rsplit_once('=').map_or(side.as_str(), |(d, _)| d), "--morph")?;
    }
    let sides = crate::spec::morph_sides(dir, &o.morph)?;
    let mut inst = Instrument::morph(&sides).map_err(|e| format!("cannot load the instrument `{dir}`: {e}"))?;
    let load = t0.elapsed();
    let sr = o.rate;
    let mut tuning = crate::spec::tuning(inst.tuning, &o.tuning)?;
    if let Some(path) = &o.scl {
        tuning.keys = Some(crate::temperament::load_scale(std::path::Path::new(path), o.key).map_err(|e| format!("--scl `{path}`: {e}"))?.0);
    }
    let mut params = crate::spec::params(Params::default(), &o.param)?;
    // the felt and, on a hammer piano, Hammer hardness at the build, as the plugin plays them
    let mut felt = crate::felt::FeltEdit::try_parse(&o.felt).ok_or("--felt: unreadable")?; // checked by the parser
    if inst.is_hammer_piano() {
        felt.hardness_db = 20.0 * (params.hardness as f64).log10();
        params.hardness = 1.0;
        if !felt.is_shipped() {
            let rows = inst.level_rows()?;
            inst = inst.with_felt_edit(&felt, &rows)?;
        }
    }

    if !o.mics.is_empty() {
        tuning.mic = crate::mic::MicPosition::Custom;
        tuning.mic_setup.mics = [crate::spec::mic(&o.mics[0])?, crate::spec::mic(&o.mics[1])?];
    }
    if let Some(w) = o.wall {
        tuning.mic_setup.wall = w;
    }
    // velocities → hammer speeds need the instrument's anchors and the params (the Dynamics); any
    // engine will do, and the one built first doubles as the reference for the single-segment case
    let mut probe = Engine::with_tuning(inst.clone(), sr, tuning)?;
    probe.set_params(params);
    // the latch: the pedal down from the start and again after a sound-off (it releases the pedal)
    let latched = o.latch.then_some((0, Event::PedalDown));
    let events: Vec<Timed> = latched
        .into_iter()
        .chain(score.events.iter().flat_map(|&(t, a)| {
            let at = (t * sr) as u64;
            let mut out = [None, None];
            match a {
                Action::NoteOn { key, velocity } => out[0] = Some(Event::NoteOn { key, speed: probe.hammer_speed(key, o.curve.apply(velocity)) }),
                Action::KeyUp { key, velocity } => {
                    // the release velocity first, so that the key-up finds it
                    if velocity > 0 {
                        out[0] = Some(Event::ReleaseVelocity { key, velocity });
                    }
                    out[1] = Some(Event::KeyUp { key });
                }
                Action::Pedal { depth } => out[0] = (!o.latch).then_some(Event::Pedal { depth }),
                Action::Soft { depth } => out[0] = Some(Event::SoftPedal { depth }),
                Action::Sostenuto { down } => out[0] = Some(if down { Event::SostenutoDown } else { Event::SostenutoUp }),
                Action::SoundOff => {
                    out[0] = Some(Event::Silence);
                    out[1] = o.latch.then_some(Event::PedalDown);
                }
                Action::ModWheel { value } => out[0] = Some(Event::ModWheel { value }),
                Action::ChannelPressure { value } => out[0] = Some(Event::ChannelPressure { value }),
                Action::KeyPressure { key, value } => out[0] = Some(Event::KeyPressure { key, value }),
                Action::PitchBend { value } => out[0] = Some(Event::PitchBend { value }),
            }
            out.into_iter().flatten().map(move |ev| (at, ev))
        }))
        .collect();
    drop(probe);

    let total = ((score.length + o.tail) * sr) as u64;
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let (warm, max_warm) = ((o.warmup * sr) as u64, (o.maxwarm * sr) as u64);
    // moments when every damper is down: pedals up, no key held
    let mut quiet = vec![0u64];
    let (mut down, mut pedal, mut sostenuto) = ([false; 128], 0.0f32, false);
    for &(t, ev) in &events {
        match ev {
            Event::NoteOn { key, .. } | Event::KeyDown { key } => down[key as usize] = true,
            Event::KeyUp { key } => down[key as usize] = false,
            Event::ReleaseVelocity { .. } => {}
            Event::Pedal { depth } => pedal = depth,
            Event::PedalDown => pedal = 1.0,
            Event::PedalUp => pedal = 0.0,
            Event::SostenutoDown => sostenuto = true,
            Event::SostenutoUp => sostenuto = false,
            // the soft pedal, the Bebung's controllers and the pitch wheel touch no damper
            Event::SoftPedal { .. } | Event::ModWheel { .. } | Event::ChannelPressure { .. } | Event::KeyPressure { .. } | Event::PitchBend { .. } => {}
            Event::Silence => (down, pedal, sostenuto) = ([false; 128], 0.0, false),
        }
        if pedal == 0.0 && !sostenuto && !down.iter().any(|&d| d) {
            quiet.push(t);
        }
    }
    let start_for = |from: u64| {
        let latest = from.saturating_sub(warm);
        let i = quiet.partition_point(|&q| q <= latest);
        let q = quiet[i - 1];
        if q + max_warm >= from { q } else { from.saturating_sub(max_warm) }
    };
    // a segment is worth its warm-up only when it is much longer than it
    let jobs = if o.jobs > 0 { o.jobs } else { cores.min((total / (3 * warm.max(1))).max(1) as usize) };
    let t1 = Instant::now();
    let bounds: Vec<u64> = (0..=jobs).map(|j| total * j as u64 / jobs as u64).collect();
    // the same segments for every mix: the stems' sum is the full render's
    let render_mix = |mix: Mix| -> Result<Vec<Vec<[f32; 2]>>> {
        let parts: Vec<Result<Vec<[f32; 2]>>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..jobs)
                .map(|j| {
                    let (inst, events, start_for) = (&inst, &events, &start_for);
                    let (from, to) = (bounds[j], bounds[j + 1]);
                    s.spawn(move || {
                        let start = start_for(from);
                        render_segment(inst, tuning, params, mix, sr, events, start, from, to).map_err(|e| e.to_string())
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("render thread").map_err(Into::into)).collect()
        });
        parts.into_iter().collect()
    };
    let parts = render_mix(Mix::ALL)?;
    let render = t1.elapsed();

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr as u32,
        bits_per_sample: o.bits,
        sample_format: if o.bits == 32 { hound::SampleFormat::Float } else { hound::SampleFormat::Int },
    };
    let raw_peak = parts.iter().flatten().flatten().fold(0.0f32, |m, x| m.max(x.abs()));
    let gain = match o.normalize {
        Some(db) if raw_peak > 0.0 => 10f32.powf(db as f32 / 20.0) / raw_peak,
        _ => 1.0,
    };
    // one gain for the full render and its stems; the clipped samples counted
    let write = |path: &str, parts: &[Vec<[f32; 2]>]| -> Result<u64> {
        let mut w = hound::WavWriter::create(path, spec).map_err(|e| format!("cannot write `{path}`: {e}"))?;
        let scale = (1i64 << (o.bits - 1)) as f32;
        let mut clipped = 0u64;
        for [l, r] in parts.iter().flatten() {
            for x in [l * gain, r * gain] {
                match o.bits {
                    32 => w.write_sample(x)?,
                    _ => {
                        clipped += (x.abs() >= 1.0) as u64;
                        w.write_sample((x * scale).round().clamp(-scale, scale - 1.0) as i32)?
                    }
                }
            }
        }
        w.finalize()?;
        Ok(clipped)
    };
    let clipped = write(&out_path, &parts)?;
    let secs = total as f64 / sr;
    println!(
        "{:.1} s of music → {out_path}: rendered in {:.2} s ({:.1}× real time, {jobs} thread{}), instrument loaded in {:.2} s, peak {:.1} dBFS{}",
        secs,
        render.as_secs_f64(),
        secs / render.as_secs_f64().max(Duration::from_millis(1).as_secs_f64()),
        if jobs == 1 { "" } else { "s" },
        load.as_secs_f64(),
        20.0 * (raw_peak * gain).max(1e-9).log10(),
        if o.normalize.is_some() { format!(" (normalized by {:+.1} dB)", 20.0 * gain.log10()) } else { String::new() }
    );
    if clipped > 0 {
        eprintln!("warning: {clipped} samples clip at {} bits; lower --param volume=", o.bits);
    }
    if o.stems {
        let base = out_path.strip_suffix(".wav").unwrap_or(&out_path);
        for (name, mix) in Mix::STEMS {
            let path = format!("{base}.{name}.wav");
            write(&path, &render_mix(mix)?)?;
            println!("  {name} → {path}");
        }
    }
    if let Some(v) = &o.video {
        video::render(v, &inst, tuning, params, sr, &events, total, &out_path)?;
    }
    Ok(())
}
