//! The `midi2wav` tool: a piece cut into segments for several threads sounds like the
//! same piece rendered in one go (where the notes have been damped between the cuts).
//! Needs the exported instruments (skipped without them).

use std::process::Command;

use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind, num::*};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

fn midi(path: &std::path::Path) {
    midi_with_pedal(path, &[]);
}

/// The notes of `midi` and the sustain pedal (CC64) at (tick, value).
fn midi_with_pedal(path: &std::path::Path, pedal: &[(u32, u8)]) {
    let note = |key: u8, vel: u8| TrackEventKind::Midi {
        channel: u4::new(0),
        message: MidiMessage::NoteOn { key: u7::new(key), vel: u7::new(vel) },
    };
    let cc = |value: u8| TrackEventKind::Midi {
        channel: u4::new(0),
        message: MidiMessage::Controller { controller: u7::new(64), value: u7::new(value) },
    };
    // 480 ticks per beat at 120 bpm: 960 ticks per second; notes of half a second every 4 s
    // the pedal first: before the notes of its tick (sorted stably below)
    let mut at: Vec<_> = pedal.iter().map(|&(t, v)| (t, cc(v))).collect();
    for (i, key) in [48u8, 60, 67].into_iter().enumerate() {
        let t = i as u32 * 4 * 960;
        at.push((t, note(key, 90)));
        at.push((t + 480, note(key, 0)));
    }
    at.sort_by_key(|&(t, _)| t);
    let mut track = vec![];
    let mut last = 0;
    for (t, kind) in at {
        track.push(TrackEvent { delta: u28::new(t - last), kind });
        last = t;
    }
    track.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) });
    let smf = Smf { header: Header::new(Format::SingleTrack, Timing::Metrical(u15::new(480))), tracks: vec![track] };
    smf.save(path).unwrap();
}

fn render(mid: &std::path::Path, out: &std::path::Path, extra: &[&str]) -> Vec<f32> {
    let status = Command::new(env!("CARGO_BIN_EXE_midi2wav"))
        .args([DIR, mid.to_str().unwrap(), out.to_str().unwrap(), "--rate", "32000", "--tail", "2"])
        .args(extra)
        .status()
        .unwrap();
    assert!(status.success());
    hound::WavReader::open(out).unwrap().samples::<f32>().map(Result::unwrap).collect()
}

#[test]
fn segments_match_the_single_render() {
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_test.mid");
    midi(&mid);
    let one = render(&mid, &dir.join("one.wav"), &["--jobs", "1"]);
    let two = render(&mid, &dir.join("two.wav"), &["--jobs", "2", "--warmup", "1", "--maxwarm", "2"]);
    // 8.5 s of music + 2 s tail, stereo
    assert_eq!(one.len(), (10.5 * 32000.0) as usize * 2);
    assert_eq!(one.len(), two.len());
    let peak = one.iter().fold(0f32, |m, x| m.max(x.abs()));
    assert!(peak > 0.01, "silence");
    let worst = one.iter().zip(&two).fold(0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(worst < 1e-3 * peak, "worst difference {worst} against a peak of {peak}");
}

#[test]
fn normalize_sets_the_peak() {
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_norm.mid");
    midi(&mid);
    let loud = render(&mid, &dir.join("norm.wav"), &["--normalize", "-6"]);
    let peak = loud.iter().fold(0f32, |m, x| m.max(x.abs()));
    assert!((peak - 0.5012).abs() < 1e-3, "peak {peak}");
    // the level is optional (-1 dBFS) and must not swallow the paths after it
    let default = render(&mid, &dir.join("norm1.wav"), &["--normalize"]);
    let peak = default.iter().fold(0f32, |m, x| m.max(x.abs()));
    assert!((peak - 0.8913).abs() < 1e-3, "peak {peak}");
}

#[test]
fn scl_retunes_the_render() {
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_scl.mid");
    midi(&mid);
    let scl = |name: &str, g_cents: &str| {
        let path = dir.join(name);
        let mut text = String::from("test\n12\n");
        for i in 1..12 {
            text += &if i == 7 { format!("{g_cents}\n") } else { format!("{}.0\n", i * 100) };
        }
        text += "2/1\n";
        std::fs::write(&path, text).unwrap();
        path
    };
    let (equal, wide) = (scl("equal.scl", "700.0"), scl("wide.scl", "710.0"));
    let plain = render(&mid, &dir.join("scl0.wav"), &[]);
    let same = render(&mid, &dir.join("scl1.wav"), &["--scl", equal.to_str().unwrap()]);
    // the played notes are C, C and G (48, 60, 67): the G is 10 cents higher, in key C
    let moved = render(&mid, &dir.join("scl2.wav"), &["--scl", wide.to_str().unwrap()]);
    assert_eq!(plain, same, "an equal scale changes nothing");
    assert_ne!(plain, moved);
}

#[test]
fn two_mic_options_equal_the_preset() {
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_mic.mid");
    midi(&mid);
    // the grand's Player preset: 0.5 m apart, 1.25 m in front of the board, 0.45 m up
    let preset = render(&mid, &dir.join("mic_preset.wav"), &["--jobs", "1", "--tuning", "mic=1"]);
    let custom = render(&mid, &dir.join("mic_custom.wav"), &["--jobs", "1", "--mic", "-0.25,-1.25,0.45", "--mic", "0.25,-1.25,0.45"]);
    assert_eq!(preset, custom);
    assert!(preset.iter().any(|v| v.abs() > 1e-3));
    let other = render(&mid, &dir.join("mic_other.wav"), &["--jobs", "1", "--mic", "-3,-2,1", "--mic", "3,-2,1"]);
    assert_ne!(preset, other);
}

#[test]
fn one_mic_option_is_an_error() {
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_mic1.mid");
    midi(&mid);
    let out = Command::new(env!("CARGO_BIN_EXE_midi2wav"))
        .args([DIR, mid.to_str().unwrap(), dir.join("mic_err.wav").to_str().unwrap(), "--mic", "0,-1.5,0.45"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--mic"));
}

#[test]
fn latch_holds_the_pedal_throughout() {
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let (plain, held, lifted) = (dir.join("latch_plain.mid"), dir.join("latch_held.mid"), dir.join("latch_lifted.mid"));
    midi(&plain);
    midi_with_pedal(&held, &[(0, 127)]);
    // the pedal lifted after the first note: the latch keeps it down (the deeper counts)
    midi_with_pedal(&lifted, &[(0, 127), (960, 0)]);
    let latched = render(&plain, &dir.join("latch_a.wav"), &["--jobs", "1", "--latch"]);
    let pedalled = render(&held, &dir.join("latch_b.wav"), &["--jobs", "1"]);
    let kept = render(&lifted, &dir.join("latch_c.wav"), &["--jobs", "1", "--latch"]);
    let damped = render(&lifted, &dir.join("latch_d.wav"), &["--jobs", "1"]);
    let peak = pedalled.iter().fold(0f32, |m, x| m.max(x.abs()));
    let diff = |a: &[f32], b: &[f32]| a.iter().zip(b).fold(0f32, |m, (x, y)| m.max((x - y).abs()));
    assert!(peak > 0.01, "silence");
    assert_eq!(diff(&latched, &pedalled), 0.0, "the latch is the pedal down from the start");
    assert_eq!(diff(&kept, &pedalled), 0.0, "with the latch the file's pedal changes nothing");
    assert!(diff(&damped, &pedalled) > 0.05 * peak, "without it the lifted pedal damps");
}

#[test]
fn a_morph_with_nothing_mixed_in_is_the_instrument() {
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_morph.mid");
    midi(&mid);
    let other = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_d");
    let plain = render(&mid, &dir.join("morph_plain.wav"), &["--jobs", "1"]);
    let zero = render(&mid, &dir.join("morph_zero.wav"), &["--jobs", "1", "--morph", &format!("{other}=0")]);
    assert_eq!(plain, zero);
    let refused = Command::new(env!("CARGO_BIN_EXE_midi2wav"))
        .args([DIR, mid.to_str().unwrap(), dir.join("morph_bad.wav").to_str().unwrap(), "--morph", &format!("{other}=1.5")])
        .status()
        .unwrap();
    assert!(!refused.success());
}

fn peak_db(x: &[f32]) -> f64 {
    20.0 * (x.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64).log10()
}

#[test]
fn the_felt_renders_as_the_plugin_builds_it() {
    // the plugin's Felt tab (the "felt" state) and, on a hammer piano, its Hammer hardness go through
    // the build: the tone changes, every key's loudness stays
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_felt.mid");
    midi(&mid);
    let plain = render(&mid, &dir.join("felt0.wav"), &[]);
    let felt = render(&mid, &dir.join("felt1.wav"), &["--felt", "s:21=12,60=12;p:21=0.5"]);
    let hard = render(&mid, &dir.join("felt2.wav"), &["--param", "hardness=0.0631"]);
    assert!(felt != plain && hard != plain);
    for (name, x) in [("felt", &felt), ("hardness", &hard)] {
        let d = peak_db(x) - peak_db(&plain);
        assert!(d.abs() <= 1.0, "{name}: the peak moved {d:.2} dB");
    }
    // unreadable text: an error, not the shipped felt in silence
    let status = Command::new(env!("CARGO_BIN_EXE_midi2wav"))
        .args([DIR, mid.to_str().unwrap(), dir.join("felt3.wav").to_str().unwrap(), "--felt", "s:60=1,40=2"])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn the_dynamics_set_the_hammer_speeds() {
    // --param dynamics narrows the level range as the plugin's Dynamics does: the velocity-90 notes
    // come up towards the ff (the velocities turn into hammer speeds with the given params)
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_dynamics.mid");
    midi(&mid);
    let plain = render(&mid, &dir.join("dyn0.wav"), &[]);
    let narrow = render(&mid, &dir.join("dyn1.wav"), &["--param", "dynamics=15"]);
    let d = peak_db(&narrow) - peak_db(&plain);
    assert!(d > 2.0, "Dynamics 15 moved the peak {d:.2} dB");
}

#[test]
fn a_video_comes_with_the_sound() {
    // --video: ffmpeg's file holds a video stream of the size asked and the sound, as long as out.wav
    let ffmpeg = Command::new("ffprobe").arg("-version").output().is_ok_and(|o| o.status.success());
    if !std::path::Path::new(DIR).exists() || !ffmpeg {
        eprintln!("skipped: no instrument or no ffmpeg");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let (mid, wav, mp4) = (dir.join("midi2wav_video.mid"), dir.join("video.wav"), dir.join("video.mp4"));
    midi(&mid);
    let _ = std::fs::remove_file(&mp4);
    let sound = render(&mid, &wav, &["--video", mp4.to_str().unwrap(), "--size", "160x90", "--fps", "10"]);
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type,width,height:format=duration", "-of", "compact=p=0:nk=1"])
        .arg(&mp4)
        .output()
        .unwrap();
    let text = String::from_utf8(probe.stdout).unwrap();
    assert!(text.contains("video|160|90"), "{text}");
    assert!(text.contains("audio"), "{text}");
    let duration: f64 = text.lines().last().unwrap().trim().parse().unwrap();
    let seconds = sound.len() as f64 / 2.0 / 32000.0;
    assert!((duration - seconds).abs() < 0.2, "{duration} s of video, {seconds} s of sound");
    // --transparent: ProRes 4444 with its alpha by default
    let mov = dir.join("video.mov");
    render(&mid, &wav, &["--video", mov.to_str().unwrap(), "--size", "160x90", "--fps", "10", "--transparent"]);
    let probe = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_name,pix_fmt", "-of", "compact=p=0:nk=1"]).arg(&mov).output().unwrap();
    let text = String::from_utf8(probe.stdout).unwrap();
    assert!(text.contains("prores|yuva444"), "{text}");
    // an odd size or no frames at all is refused before anything is rendered
    for bad in [["--size", "161x90"], ["--fps", "0"]] {
        let status = Command::new(env!("CARGO_BIN_EXE_midi2wav"))
            .args([DIR, mid.to_str().unwrap(), dir.join("video2.wav").to_str().unwrap(), "--video", "x.mp4"])
            .args(bad)
            .status()
            .unwrap();
        assert!(!status.success(), "{bad:?}");
    }
}

#[test]
fn the_command_line_helps_and_its_errors_say_what_is_wrong() {
    let run = |args: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_midi2wav")).args(args).output().unwrap();
        (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr))
    };
    // --help: every key of --param and --tuning, the temperaments; -h the summary alone
    let (code, help) = run(&["--help"]);
    assert_eq!(code, Some(0));
    for (k, _) in pianoengine::spec::PARAM_KEYS.iter().chain(pianoengine::spec::TUNING_KEYS) {
        assert!(help.contains(&format!("  {k} ")), "--help lacks {k}");
    }
    assert!(help.contains("Werckmeister III"));
    let (code, short) = run(&["-h"]);
    assert!(code == Some(0) && short.len() < help.len() / 2 && !short.contains("sympathetic"));
    assert_eq!(run(&["--version"]).0, Some(0));
    // usage errors (exit 2) name the option and the trouble, before anything is loaded
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("midi2wav_cli.mid");
    midi(&mid);
    let (m, out) = (mid.to_str().unwrap(), dir.join("cli.wav"));
    let out = out.to_str().unwrap();
    for (args, says) in [
        (vec!["--parm", "room=1"], "a similar argument exists: '--param'"),
        (vec!["--param", "roomm=1"], "did you mean `room`?"),
        (vec!["--param", "room=loud"], "`loud` is not a number"),
        (vec!["--tuning", "mic_patern1=1"], "did you mean `mic_pattern1`?"),
        (vec!["--rate", "fast"], "`fast` is not a number"),
        (vec!["--bits", "20"], "16, 24 or 32"),
        (vec!["--mic", "1,2,3"], "given twice"),
        (vec!["--fps", "30"], "--video"),
        (vec!["--video", "v.mp4", "--size", "1920"], "WxH"),
        (vec!["--normalize", "3"], "0 or below"),
    ] {
        let (code, text) = run(&[&[DIR, m, out][..], &args].concat());
        assert_eq!(code, Some(2), "{args:?}: {text}");
        assert!(text.contains(says), "{args:?}: {text}");
    }
    // a missing input or folder: exit 1, its name in the message
    let (code, text) = run(&["no/such/instrument", m, out]);
    assert!(code == Some(1) && text.contains("`no/such/instrument`: no such directory"), "{text}");
    let (code, text) = run(&[DIR, "no-such.mid", out]);
    assert!(code == Some(1) && text.contains("cannot read the MIDI file `no-such.mid`"), "{text}");
    let (code, text) = run(&[DIR, m, "/no/such/folder/out.wav"]);
    assert!(code == Some(1) && text.contains("the folder `/no/such/folder` does not exist"), "{text}");
}

/// pe_cli as the standalone calls it: `render` first, then midi2wav's arguments; its exit status
fn pe_cli(args: &[&str]) -> i32 {
    let owned: Vec<std::ffi::CString> = std::iter::once("render").chain(args.iter().copied())
        .map(|a| std::ffi::CString::new(a).unwrap()).collect();
    let ptrs: Vec<*const std::os::raw::c_char> = owned.iter().map(|c| c.as_ptr()).collect();
    unsafe { pianoengine::ffi::pe_cli(ptrs.len() as i32, ptrs.as_ptr()) }
}

#[test]
fn the_standalones_render_writes_what_midi2wav_writes() {
    // `pianoengine render …` (the standalone) runs midi2wav's code through the C interface
    if !std::path::Path::new(DIR).exists() {
        eprintln!("skipped: no instrument");
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mid = dir.join("pe_cli.mid");
    midi(&mid);
    let (a, b) = (dir.join("pe_cli_tool.wav"), dir.join("pe_cli_entry.wav"));
    render(&mid, &a, &["--jobs", "2"]);
    let args = [DIR, mid.to_str().unwrap(), b.to_str().unwrap(), "--rate", "32000", "--tail", "2", "--jobs", "2"];
    assert_eq!(pe_cli(&args), 0);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap(), "not the same file");
    // a usage error is an exit status, not the end of the process (the standalone prints it and exits)
    assert_eq!(pe_cli(&[DIR, mid.to_str().unwrap(), b.to_str().unwrap(), "--rate", "fast"]), 2);
    assert_eq!(pe_cli(&["--version"]), 0);
}
