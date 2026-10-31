//! The measurements on synthetic tones: onset, level, the pitch score's octave choice, the
//! deviation, and the cache.
use pe_editor::audio::{self, Measures};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

mod audio_tones {
    include!("audio_tones.rs");
}
use audio_tones::{harmonic, write_partials, write_tone};

#[test]
fn onset_and_level() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("t.wav");
    write_tone(&p, 220.0, 0.5, &[(1.0, 1.0)], 0.2);
    let m = audio::measure(&p).unwrap();
    assert!((m.onset_s - 0.2).abs() < 0.005, "{}", m.onset_s);
    // a sine of 0.5 decaying 1.5/s: about −9.0 dB at the start, −10.6 dB averaged over 0.5 s
    assert!(m.level_db < -9.0 && m.level_db > -12.0, "{}", m.level_db);
    assert!((m.peaks[0][0] - 220.0).abs() < 1.0, "{:?}", &m.peaks[..2]);
    assert_eq!(m.peaks[0][1], 0.0);
}

#[test]
fn the_score_picks_the_octave_of_a_harmonic_tone() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("h.wav");
    write_tone(&p, 220.0, 0.3, &harmonic(), 0.05);
    let m = audio::measure(&p).unwrap();
    let s = |f| audio::score(&m.peaks, f, m.nyquist).0;
    assert!(s(220.0) > 0.9, "{}", s(220.0));
    assert!(s(110.0) < 0.6 && s(440.0) < 0.6, "{} {}", s(110.0), s(440.0));
    assert!(s(220.0 * 2f32.powf(5.0 / 12.0)) < 0.3);
}

#[test]
fn the_score_picks_the_octave_of_a_bar() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("b.wav");
    let f0 = audio::midi_hz(60.0);
    write_tone(&p, f0, 0.3, &[(1.0, 1.0), (4.0, 0.5), (10.0, 0.25)], 0.05);
    let m = audio::measure(&p).unwrap();
    let s = |f| audio::score(&m.peaks, f, m.nyquist);
    assert_eq!(s(f0).1, 1, "the tuned-bar profile");
    assert!(s(f0).0 > s(f0 / 2.0).0 && s(f0).0 > s(f0 * 2.0).0);
}

#[test]
fn the_score_takes_a_free_bar_whose_second_mode_is_off_the_textbook_ratio() {
    // a glockenspiel's bar: the second mode at 3.04 (the textbook's free bar 2.76), weak, the third
    // weaker; the octave reading keeps it
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("g.wav");
    let f0 = audio::midi_hz(79.0);
    write_tone(&p, f0, 0.3, &[(1.0, 1.0), (3.04, 0.1), (5.69, 0.03)], 0.05);
    let m = audio::measure(&p).unwrap();
    let s = |f| audio::score(&m.peaks, f, m.nyquist);
    assert_eq!(s(f0).1, 2, "the free-bar profile: {:?}", s(f0));
    assert!(s(f0).0 >= 0.5, "{:?}", s(f0));
    assert!(s(f0).0 > s(f0 / 2.0).0 && s(f0).0 > s(f0 * 2.0).0, "{:?} {:?} {:?}", s(f0), s(f0 / 2.0), s(f0 * 2.0));
}

#[test]
fn a_string_with_weak_upper_harmonics_stays_harmonic() {
    // its third harmonic lies where a bar's second mode may: the octave above says it is a string's
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("s.wav");
    write_tone(&p, 220.0, 0.3, &[(1.0, 1.0), (2.0, 0.3), (3.0, 0.1), (4.0, 0.02), (5.0, 0.01), (6.0, 0.02)], 0.05);
    let m = audio::measure(&p).unwrap();
    assert_eq!(audio::score(&m.peaks, 220.0, m.nyquist).1, 0);
}

#[test]
fn a_harmonic_series_is_no_tuned_bar() {
    // every harmonic series holds 1, 4 and 10: with weak partials between them the tuned-bar
    // profile won on piano keys (bar shares up to 0.52 on the grands); a string sounds its second
    // and third harmonics, no bar both
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("s.wav");
    write_tone(&p, 220.0, 0.3, &[(1.0, 1.0), (2.0, 0.2), (3.0, 0.1), (4.0, 0.5), (5.0, 0.01), (6.0, 0.01), (10.0, 0.25)], 0.05);
    let m = audio::measure(&p).unwrap();
    assert_eq!(audio::score(&m.peaks, 220.0, m.nyquist).1, 0, "{:?}", audio::score(&m.peaks, 220.0, m.nyquist));
}

#[test]
fn a_bars_mode_at_the_octave_alone_keeps_it_a_bar() {
    // the crotales' second mode runs 2.15 → 1.91 times the fundamental, at the octave on their
    // middle keys, with nothing at three times it
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("c.wav");
    let f0 = audio::midi_hz(96.0);
    write_tone(&p, f0, 0.3, &[(1.0, 1.0), (2.01, 0.3), (4.0, 0.3)], 0.05);
    let m = audio::measure(&p).unwrap();
    assert!(audio::score(&m.peaks, f0, m.nyquist).1 > 0, "{:?}", audio::score(&m.peaks, f0, m.nyquist));
}

#[test]
fn a_free_bar_reading_needs_its_fundamental() {
    // a plate's 1 and 2.41 read from 19 semitones below land at 3.0 and 7.2: in the free bar's
    // bands, but no bar lacks its fundamental
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("p.wav");
    let f0 = audio::midi_hz(108.0);
    write_tone(&p, f0, 0.3, &[(1.0, 1.0), (2.41, 0.7), (4.13, 0.5), (5.93, 0.4)], 0.05);
    let m = audio::measure(&p).unwrap();
    let low = f0 * 2f32.powf(-19.0 / 12.0);
    assert!(audio::score(&m.peaks, low, m.nyquist).0 < 0.5, "{:?}", audio::score(&m.peaks, low, m.nyquist));
}

#[test]
fn the_decays_of_the_strongest_partials() {
    // a slow fundamental (1/s, a steel bar's) and a fast third partial (8/s): each measured
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("d.wav");
    write_partials(&p, 440.0, &[(1.0, 1.0, 1.0), (3.0, 0.5, 8.0)], 0.1);
    let m = audio::measure(&p).unwrap();
    let sigma = |f: f32| m.decays.iter().find(|x| (x[0] / f - 1.0).abs() < 0.01).map(|x| x[1]);
    let (s1, s3) = (sigma(440.0).unwrap(), sigma(1320.0).unwrap());
    assert!((s1 - 1.0).abs() < 0.15, "{s1}");
    assert!((s3 - 8.0).abs() < 1.2, "{s3}");
}

#[test]
fn a_wooden_bars_fast_decay() {
    // 17/s (150 dB/s, a xylophone's fundamental): gone within half a second, still measured
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("w.wav");
    write_partials(&p, 1046.5, &[(1.0, 1.0, 17.0)], 0.1);
    let m = audio::measure(&p).unwrap();
    let s = m.decays.iter().find(|x| (x[0] / 1046.5 - 1.0).abs() < 0.01).map(|x| x[1]).unwrap();
    assert!((s - 17.0).abs() < 4.0, "{s}");
}

#[test]
fn a_mistuned_tone_shows_its_deviation() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("m.wav");
    write_tone(&p, 220.0 * 2f32.powf(1.0 / 12.0), 0.3, &harmonic(), 0.05);
    let m = audio::measure(&p).unwrap();
    let dev = audio::deviation_cents(&m.peaks, 220.0, m.nyquist).unwrap();
    assert!((dev - 100.0).abs() < 10.0, "{dev}");
}

#[test]
fn measure_all_reuses_the_cache() {
    let d = tempfile::tempdir().unwrap();
    let paths: Vec<PathBuf> = (0..3).map(|i| d.path().join(format!("{i}.wav"))).collect();
    for (i, p) in paths.iter().enumerate() {
        write_tone(p, 200.0 + 50.0 * i as f32, 0.3, &harmonic(), 0.05);
    }
    let bad = d.path().join("bad.wav");
    std::fs::write(&bad, b"not audio").unwrap();
    let mut all = paths.clone();
    all.push(bad.clone());
    let cache = d.path().join("cache/editor/audio.json");
    let progress = AtomicUsize::new(0);
    let m: Measures = audio::measure_all(&all, &cache, &progress);
    assert_eq!((m.ok.len(), m.failed.len(), m.decoded), (3, 1, 4));
    assert_eq!(progress.load(std::sync::atomic::Ordering::Relaxed), 4);
    let again = audio::measure_all(&all, &cache, &AtomicUsize::new(0));
    assert_eq!((again.ok.len(), again.failed.len(), again.decoded), (3, 1, 0));
    assert_eq!(again.ok, m.ok);
    write_tone(&paths[1], 300.0, 0.3, &harmonic(), 0.25);
    let third = audio::measure_all(&all, &cache, &AtomicUsize::new(0));
    assert_eq!(third.decoded, 1);
}

#[test]
fn a_wav_with_a_long_fmt_chunk_decodes() {
    // a 20-byte fmt chunk (PCM with cbSize and two spare bytes): symphonia refuses it, Python reads it
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("long-fmt.wav");
    let frames: Vec<i16> = (0..4800).map(|i| ((i as f32 * 0.05).sin() * 16000.0) as i16).collect();
    let mut b: Vec<u8> = Vec::new();
    let data_len = (frames.len() * 2 * 2) as u32;
    b.extend(b"RIFF");
    b.extend((4 + 8 + 20 + 8 + data_len).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(20u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(48000u32.to_le_bytes());
    b.extend((48000u32 * 4).to_le_bytes());
    b.extend(4u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend([0u8; 4]);
    b.extend(b"data");
    b.extend(data_len.to_le_bytes());
    for s in &frames {
        b.extend(s.to_le_bytes());
        b.extend((-s).to_le_bytes());
    }
    std::fs::write(&p, b).unwrap();
    let c = audio::decode(&p, None).unwrap();
    assert_eq!((c.rate, c.channels, c.data.len()), (48000, 2, 9600));
    assert!((c.data[2] - frames[1] as f32 / 32768.0).abs() < 1e-6);
    assert!((c.data[3] + frames[1] as f32 / 32768.0).abs() < 1e-6);
}

#[test]
fn a_lone_partial_explains_nothing() {
    // near the top of the range a profile keeps one partial below 16 kHz: one peak fits any reading
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("hi.wav");
    write_tone(&p, audio::midi_hz(108.0), 0.3, &[(1.0, 1.0)], 0.05);
    let m = audio::measure(&p).unwrap();
    assert!(audio::score(&m.peaks, audio::midi_hz(108.0), m.nyquist).0 < 0.5);
}

#[test]
fn a_cache_of_an_older_measurement_is_measured_again() {
    // the decoder or the measurements changed (CACHE_VERSION): an old entry, an error among them, is not reused
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.wav");
    write_tone(&p, 220.0, 0.3, &harmonic(), 0.05);
    let cache = d.path().join("audio.json");
    audio::measure_all(&[p.clone()], &cache, &AtomicUsize::new(0));
    let mut c: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&cache).unwrap()).unwrap();
    c["version"] = serde_json::json!(1);
    std::fs::write(&cache, c.to_string()).unwrap();
    assert_eq!(audio::measure_all(&[p], &cache, &AtomicUsize::new(0)).decoded, 1);
}

#[test]
fn the_cache_drops_files_that_are_gone() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("t.wav");
    write_tone(&p, 220.0, 0.2, &[(1.0, 1.0)], 0.05);
    let cache = d.path().join("audio.json");
    let gone = d.path().join("gone.wav").to_string_lossy().into_owned();
    std::fs::write(&cache, serde_json::json!({"version": 2, "files": {gone.clone(): {"size": 1, "mtime_ns": 1, "measure": null, "error": "x"}}}).to_string()).unwrap();
    audio::measure_all(&[p], &cache, &AtomicUsize::new(0));
    let text = std::fs::read_to_string(&cache).unwrap();
    assert!(!text.contains(&gone), "a file no longer there");
    assert!(text.contains("t.wav"));
}

/// a sum of sines (Hz, amplitude), decaying `db_per_s`, `seconds` long, no silence before
fn write_decay(path: &std::path::Path, partials: &[(f32, f32)], db_per_s: f32, seconds: f32) {
    let rate = 48000u32;
    let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(seconds * rate as f32) as usize {
        let t = i as f32 / rate as f32;
        let g = 10f32.powf(-db_per_s * t / 20.0);
        let s: f32 = partials.iter().map(|(f, a)| a * (2.0 * std::f32::consts::PI * f * t).sin()).sum();
        w.write_sample(0.5 * g * s).unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn the_envelope_follows_a_decay_to_the_end() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("d.wav");
    write_decay(&p, &[(440.0, 1.0)], 10.0, 6.0);
    let m = audio::measure(&p).unwrap();
    let want = (6.0 / audio::ENV_HOP_S) as i64;
    assert!((m.env_db.len() as i64 - want).abs() <= 1, "{} frames", m.env_db.len());
    let fall = m.env_db[60] - m.env_db[20];
    assert!((fall + 20.0).abs() < 1.0, "{fall} dB over 2 s");
}

#[test]
fn the_bands_hold_the_partials() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("b.wav");
    write_decay(&p, &[(220.0, 1.0), (1760.0, 0.25)], 0.0, 1.0);
    let m = audio::measure(&p).unwrap();
    let band = |hz: f32| (0..m.bands_db.len()).min_by(|a, b| (audio::band_hz(*a).log2() - hz.log2()).abs()
        .total_cmp(&(audio::band_hz(*b).log2() - hz.log2()).abs())).unwrap();
    let top = m.bands_db.iter().copied().fold(f32::MIN, f32::max);
    assert_eq!(m.bands_db[band(220.0)], top);
    let under = top - m.bands_db[band(1760.0)];
    assert!((under - 12.0).abs() < 2.0, "{under} dB under");
}
