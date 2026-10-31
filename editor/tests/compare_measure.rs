//! The Compare page's long measurement on synthetic tones (in memory): the noise floor, the
//! end, the stop, the length, two decay segments per partial, the partials' noise, and a render
//! measured like a recording.
use pe_editor::audio::{self, Long};

const SR: f32 = 48000.0;

/// partials (Hz, amplitude, early rate, late rate in 1/s; the rate changes at `knee_s`) after
/// `pre_s` of digital silence, `secs` long, plus white noise at `noise_db` dBFS (none: −200)
fn tone(parts: &[(f32, f32, f32, f32)], knee_s: f32, pre_s: f32, secs: f32, noise_db: f32) -> Vec<f32> {
    let mut seed = 1u32;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };
    let noise = 10f32.powf(noise_db / 20.0) * 3f32.sqrt(); // uniform: rms = amplitude / √3
    let pre = (pre_s * SR) as usize;
    let mut x = vec![0.0f32; pre];
    for i in 0..(secs * SR) as usize {
        let t = i as f32 / SR;
        let s: f32 = parts.iter().map(|&(f, a, e, l)| {
            let env = if t < knee_s { (-e * t).exp() } else { (-e * knee_s).exp() * (-l * (t - knee_s)).exp() };
            a * env * (2.0 * std::f32::consts::PI * f * t).sin()
        }).sum();
        x.push(0.3 * s + noise * rnd());
    }
    x
}

/// how far a decay at `a` (1/s) lies under its start averaged over its first 0.5 s, in power (dB):
/// the length's reference
fn head_under(a: f32) -> f32 {
    -10.0 * ((1.0 - (-a).exp()) / a).log10()
}

fn near(a: f32, b: f32, rel: f32) -> bool {
    (a / b - 1.0).abs() <= rel
}

#[test]
fn two_decay_segments_per_partial() {
    let x = tone(&[(220.0, 1.0, 3.0, 0.5), (440.0, 0.5, 4.0, 0.8)], 1.0, 0.0, 8.0, -200.0);
    let m = audio::measure_long(&x, SR, 57.0);
    let p = &m.partials[0];
    assert!((p.hz - 220.0).abs() < 1.0, "{m:?}");
    assert!(near(p.early.unwrap(), 3.0, 0.08), "early {:?}", p.early);
    assert!(near(p.late.unwrap(), 0.5, 0.08), "late {:?}", p.late);
    let q = m.partials.iter().find(|q| (q.hz - 440.0).abs() < 2.0).unwrap();
    assert!(near(q.early.unwrap(), 4.0, 0.08) && near(q.late.unwrap(), 0.8, 0.08), "{q:?}");
    assert!((m.fundamental_hz.unwrap() - 220.0).abs() < 1.0);
}

#[test]
fn noise_floor_and_end() {
    // falls 1 dB per 0.115 s (1/s): from about −16 dBFS to the −70 dBFS noise in about 6 s
    let x = tone(&[(330.0, 1.0, 1.0, 1.0)], 1.0, 0.0, 12.0, -70.0);
    let m = audio::measure_long(&x, SR, 64.0);
    assert!((m.floor_db + 70.0).abs() < 3.0, "floor {}", m.floor_db);
    assert!(m.end_s > 4.0 && m.end_s < 8.0, "end {}", m.end_s);
    assert!(m.stopped_s.is_none());
}

#[test]
fn floor_ignores_leading_digital_silence() {
    let x = tone(&[(330.0, 1.0, 1.0, 1.0)], 1.0, 0.5, 12.0, -70.0);
    let m = audio::measure_long(&x, SR, 64.0);
    assert!((m.onset_s - 0.5).abs() < 0.01, "onset {}", m.onset_s);
    assert!((m.floor_db + 70.0).abs() < 3.0, "floor {}", m.floor_db);
}

#[test]
fn end_is_the_file_end_when_still_loud() {
    let x = tone(&[(330.0, 1.0, 0.2, 0.2)], 1.0, 0.0, 4.0, -200.0);
    let m = audio::measure_long(&x, SR, 64.0);
    assert!(m.floor_db >= -100.0, "floor {}", m.floor_db);
    assert!((m.end_s - 4.0).abs() < 0.15, "end {}", m.end_s);
}

#[test]
fn a_damper_landing_cuts_the_comparison() {
    // rings at 0.5/s, then every partial at 25/s from 2.5 s (a damper)
    let x = tone(&[(262.0, 1.0, 0.5, 25.0), (524.0, 0.6, 0.5, 25.0), (786.0, 0.4, 0.5, 25.0)], 2.5, 0.0, 6.0, -80.0);
    let m = audio::measure_long(&x, SR, 60.0);
    let d = m.stopped_s.unwrap_or_else(|| panic!("no damper found: {m:?}"));
    assert!((d - 2.5).abs() < 0.15, "damped at {d}");
    assert!(m.stopped);
    assert!(m.partials.iter().all(|p| p.until_s <= d + 0.01));
    assert!(near(m.partials[0].late.unwrap(), 0.5, 0.15), "late {:?}", m.partials[0].late);
}

#[test]
fn short_recording_has_no_late_decay() {
    let x = tone(&[(1047.0, 1.0, 30.0, 30.0)], 1.0, 0.0, 0.8, -200.0);
    let m = audio::measure_long(&x, SR, 84.0);
    assert!(m.partials[0].early.is_some());
    assert!(m.partials.iter().all(|p| p.late.is_none()), "{m:?}");
}

#[test]
fn a_render_is_measured_over_the_recordings_times() {
    let rec = tone(&[(220.0, 1.0, 3.0, 0.5), (440.0, 0.5, 4.0, 0.8)], 1.0, 0.0, 8.0, -70.0);
    let m = audio::measure_long(&rec, SR, 57.0);
    // the render: no noise, a partial missing, twice as fast late
    let ren = tone(&[(220.0, 1.0, 3.0, 1.0)], 1.0, 0.0, 10.0, -200.0);
    let r: Long = audio::measure_like(&ren, SR, 57.0, &m);
    assert_eq!(r.end_s, m.end_s);
    assert_eq!(r.stopped_s, m.stopped_s);
    assert_eq!(r.partials.len(), m.partials.len());
    assert!(near(r.partials[0].late.unwrap(), 1.0, 0.08), "{:?}", r.partials[0]);
    let missing = r.partials.iter().find(|p| (p.hz - 440.0).abs() < 2.0).unwrap();
    assert_eq!(missing.db, audio::UNMATCHED_DB);
    assert!(missing.early.is_none() && missing.late.is_none());
}

#[test]
fn a_rumble_below_the_note_is_no_partial() {
    // a room's rumble at 50 Hz under a 440 Hz tone (A4): not one of its partials
    let x = tone(&[(440.0, 1.0, 2.0, 2.0), (880.0, 0.3, 2.0, 2.0), (50.0, 0.1, 0.1, 0.1)], 1.0, 0.0, 3.0, -200.0);
    let m = audio::measure_long(&x, SR, 69.0);
    assert!(m.partials.iter().all(|p| p.hz > 300.0), "{:?}", m.partials);
    assert!(m.partials.iter().any(|p| (p.hz - 880.0).abs() < 2.0));
}

#[test]
fn a_detuned_render_still_matches_its_partials() {
    // a set recorded at A = 415 against a render at 440: every partial 100 cents off; matched
    // after the fundamentals' ratio, the decay still compares (the pitch lane shows the offset)
    let rec = tone(&[(207.65, 1.0, 3.0, 0.5), (415.3, 0.5, 4.0, 0.8)], 1.0, 0.0, 6.0, -70.0);
    let m = audio::measure_long(&rec, SR, 56.0);
    let ren = tone(&[(220.0, 1.0, 3.0, 0.5), (440.0, 0.5, 4.0, 0.8)], 1.0, 0.0, 7.0, -200.0);
    let r = audio::measure_like(&ren, SR, 56.0, &m);
    assert!(r.partials.iter().all(|p| p.db > audio::UNMATCHED_DB), "{:?}", r.partials);
    assert!((r.partials[0].hz - 220.0).abs() < 1.0 && near(r.partials[0].late.unwrap(), 0.5, 0.08), "{:?}", r.partials[0]);
}

#[test]
fn a_recording_cut_while_ringing_ends_at_the_file_end() {
    // 10 s falling 4.3 dB/s with no noise: its quietest window is the signal, not a floor; the
    // comparison runs to the file's end
    let x = tone(&[(330.0, 1.0, 0.5, 0.5)], 1.0, 0.0, 10.0, -200.0);
    let m = audio::measure_long(&x, SR, 64.0);
    assert!((m.end_s - 10.0).abs() < 0.15, "end {} (floor {})", m.end_s, m.floor_db);
}

#[test]
fn the_length_of_a_plain_decay() {
    // 3/s: 26.06 dB/s; 50 dB under its first 0.5 s (4.99 dB under its start): 2.11 s
    let x = tone(&[(440.0, 1.0, 3.0, 3.0)], 1.0, 0.0, 6.0, -200.0);
    let m = audio::measure_long(&x, SR, 69.0);
    assert!(!m.stopped && m.length_db == 50.0, "{m:?}");
    let l = m.length_s.unwrap();
    let expect = (head_under(3.0) + 50.0) / 26.06;
    assert!((l - expect).abs() < 0.03, "length {l}, expected {expect}");
}

#[test]
fn a_decay_that_speeds_up_is_stopped_there() {
    // 0.5/s, then 6/s from 2 s (a hand on the bell): stopped at 2 s, its length a lower bound
    let x = tone(&[(440.0, 1.0, 0.5, 6.0)], 2.0, 0.0, 6.0, -90.0);
    let m = audio::measure_long(&x, SR, 69.0);
    let s = m.stopped_s.unwrap_or_else(|| panic!("not stopped: {m:?}"));
    assert!((s - 2.0).abs() < 0.1, "stopped at {s}");
    assert!(m.stopped && (m.length_s.unwrap() - s).abs() < 0.1, "{m:?}");
}

#[test]
fn a_file_ending_before_the_fall_is_measured_to_the_depth_it_shows() {
    // 0.5/s (4.3 dB/s) for 6 s, no noise: 26 dB fallen at the end, the quietest window the signal's
    // (cut while ringing); read to 10 dB above it, not a lower bound (a pp key 18 dB above its
    // noise read so as stopped at the file's end)
    let x = tone(&[(440.0, 1.0, 0.5, 0.5)], 1.0, 0.0, 6.0, -200.0);
    let m = audio::measure_long(&x, SR, 69.0);
    assert!(!m.stopped && m.length_db > 10.0 && m.length_db < 20.0, "{m:?}");
    let expect = (head_under(0.5) + m.length_db) / 4.343;
    assert!((m.length_s.unwrap() - expect).abs() < 0.1, "{m:?}, expected {expect}");
}

#[test]
fn a_slowing_decay_is_not_stopped() {
    // a piano's double decay: 4/s, then 0.4/s from 1 s (slower): it falls 30 dB, never stops
    let x = tone(&[(220.0, 1.0, 4.0, 0.4), (440.0, 0.5, 5.0, 0.6)], 1.0, 0.0, 12.0, -200.0);
    let m = audio::measure_long(&x, SR, 57.0);
    assert!(m.stopped_s.is_none() && !m.stopped, "{m:?}");
    assert!(m.length_s.is_some());
}

#[test]
fn a_partial_in_the_noise_is_marked() {
    // a −40 dB partial under −45 dBFS noise (8 s: the file ends in its noise, not cut): its SNR
    // under 20 dB; the strong one far above
    let x = tone(&[(440.0, 1.0, 1.0, 1.0), (1320.0, 0.01, 1.0, 1.0)], 1.0, 0.5, 8.0, -45.0);
    let m = audio::measure_long(&x, SR, 69.0);
    let strong = m.partials.iter().find(|p| (p.hz - 440.0).abs() < 2.0).unwrap();
    assert!(strong.snr_db > audio::PARTIAL_SNR_DB, "{strong:?}");
    if let Some(weak) = m.partials.iter().find(|p| (p.hz - 1320.0).abs() < 4.0) {
        assert!(weak.snr_db < audio::PARTIAL_SNR_DB, "{weak:?}");
    }
}

#[test]
fn digital_silence_counts_every_partial() {
    // no noise at all (0.5 s of zeros before): every partial's SNR is far above the threshold
    let x = tone(&[(440.0, 1.0, 1.0, 1.0), (880.0, 0.05, 1.0, 1.0)], 1.0, 0.5, 3.0, -200.0);
    let m = audio::measure_long(&x, SR, 69.0);
    assert!(m.partials.iter().all(|p| p.snr_db.is_finite() && p.snr_db > audio::PARTIAL_SNR_DB), "{:?}", m.partials);
}

#[test]
fn a_beating_decay_is_not_stopped() {
    // two unisons 3 Hz apart beat 20 dB deep three times a second (a guitar's or a piano's
    // broadband level jitters so): the dips recover, no stop
    let x = tone(&[(440.0, 1.0, 0.8, 0.8), (443.0, 0.9, 0.8, 0.8)], 1.0, 0.0, 8.0, -80.0);
    let m = audio::measure_long(&x, SR, 69.0);
    assert!(m.stopped_s.is_none(), "{m:?}");
}

#[test]
fn a_slow_decay_is_not_stopped() {
    // 0.15/s (1.3 dB/s, a low piano string's), a shallow slow beat (±1.5 dB at 0.4 Hz) and light
    // noise: no stop
    let x = tone(&[(55.0, 1.0, 0.15, 0.15), (55.4, 0.2, 0.15, 0.15), (110.0, 0.5, 0.2, 0.2), (165.0, 0.3, 0.25, 0.25)], 1.0, 0.0, 15.0, -90.0);
    let m = audio::measure_long(&x, SR, 33.0);
    assert!(m.stopped_s.is_none(), "{m:?}");
}

#[test]
fn a_stop_after_the_fall_leaves_the_length() {
    // 3/s (50 dB under its first 0.5 s in 2.11 s), damped at 2.5 s: the length was reached before
    // the stop
    let x = tone(&[(440.0, 1.0, 3.0, 40.0)], 2.5, 0.0, 5.0, -120.0);
    let m = audio::measure_long(&x, SR, 69.0);
    let expect = (head_under(3.0) + 50.0) / 26.06;
    assert!(!m.stopped && (m.length_s.unwrap() - expect).abs() < 0.03, "{m:?}, expected {expect}");
}

#[test]
fn a_noisy_recording_is_measured_to_the_depth_it_shows() {
    // 1/s (8.7 dB/s) from about −15.5 dBFS (its first 0.5 s) into −45 dBFS noise: about 20 dB to
    // the floor + 10 dB, not 50; measured to that depth, not stopped; a clean render of the same
    // decay measured like it reads the same length
    let x = tone(&[(440.0, 1.0, 1.0, 1.0)], 1.0, 0.0, 8.0, -45.0);
    let m = audio::measure_long(&x, SR, 69.0);
    assert!(!m.stopped && m.length_db < 25.0 && m.length_db > 10.0, "{m:?}");
    let expect = (head_under(1.0) + m.length_db) / 8.686;
    assert!((m.length_s.unwrap() - expect).abs() < 0.1, "length {:?}, expected {expect}", m.length_s);
    let clean = tone(&[(440.0, 1.0, 1.0, 1.0)], 1.0, 0.0, 8.0, -200.0);
    let r = audio::measure_like(&clean, SR, 69.0, &m);
    assert!((r.length_s.unwrap() / m.length_s.unwrap() - 1.0).abs() < 0.05, "{:?} vs {:?}", r.length_s, m.length_s);
}

/// white noise at `db` dBFS whose level steps up and down `wobble_db` every 0.3 s (a real floor's
/// bursts)
fn bursts(n: usize, db: f32, wobble_db: f32) -> Vec<f32> {
    let mut seed = 7u32;
    (0..n).map(|i| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let v = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        let up = (i as f32 / SR / 0.3) as usize % 2 == 0;
        10f32.powf((db + if up { wobble_db } else { -wobble_db }) / 20.0) * 3f32.sqrt() * v
    }).collect()
}

#[test]
fn a_wobbling_floor_is_no_cut() {
    // a weak partial (−50 dB, about 10 dB over the noise in its band) under a floor at −45 dBFS that
    // wobbles ±4 dB, no pre-roll: the floor is a floor (the recording not cut while ringing), the
    // weak partial's noise read from it
    let mut x = tone(&[(440.0, 1.0, 2.0, 2.0), (1320.0, 0.003, 2.0, 2.0)], 1.0, 0.0, 6.0, -200.0);
    for (v, b) in x.iter_mut().zip(bursts(6 * SR as usize, -45.0, 4.0)) {
        *v += b;
    }
    let m = audio::measure_long(&x, SR, 69.0);
    let weak = m.partials.iter().find(|p| (p.hz - 1320.0).abs() < 4.0).expect("the weak partial tracked");
    assert!(weak.snr_db < audio::PARTIAL_SNR_DB, "{weak:?}");
}

#[test]
fn a_tail_faded_to_silence_is_no_noise() {
    // a noisy recording (−45 dBFS) whose last 1.5 s a library faded to digital zero: the noise is
    // read where there is noise, not from the zeros
    let mut x = tone(&[(440.0, 1.0, 1.0, 1.0), (1320.0, 0.01, 1.0, 1.0)], 1.0, 0.0, 8.0, -45.0);
    let n = x.len();
    for v in &mut x[n - (1.5 * SR) as usize..] {
        *v = 0.0;
    }
    let m = audio::measure_long(&x, SR, 69.0);
    let weak = m.partials.iter().find(|p| (p.hz - 1320.0).abs() < 4.0).expect("the weak partial tracked");
    assert!(weak.snr_db < audio::PARTIAL_SNR_DB, "{weak:?}");
}

#[test]
fn a_rumble_under_the_note_is_no_length() {
    // B4 at 2/s (17.4 dB/s) over a steady 20 Hz rumble 30 dB under it (an Iowa take's room): the
    // rumble neither ends the fall nor holds the level up; 50 dB under the first 0.5 s at 3.1 s
    let mut x = tone(&[(494.0, 1.0, 2.0, 2.0)], 1.0, 0.0, 8.0, -200.0);
    for (i, v) in x.iter_mut().enumerate() {
        *v += 0.3 * 0.03 * (2.0 * std::f32::consts::PI * 20.0 * i as f32 / SR).sin();
    }
    let m = audio::measure_long(&x, SR, 71.0);
    let expect = (head_under(2.0) + 50.0) / 17.37;
    assert!(!m.stopped && m.length_db == 50.0 && (m.length_s.unwrap() - expect).abs() < 0.05, "{m:?}, expected {expect}");
}

#[test]
fn a_fast_first_drop_then_a_long_ring_is_read_in_the_ring() {
    // a strike that drops fast (10/s) and a partial 30 dB under it that rings on (0.2/s, a piano's
    // aftersound): the ear hears the ring; 50 dB under the first 0.5 s falls in it, at about 17 s
    let x = tone(&[(440.0, 1.0, 10.0, 10.0), (660.0, 0.03, 0.2, 0.2)], 1.0, 0.0, 25.0, -200.0);
    let m = audio::measure_long(&x, SR, 69.0);
    assert!(!m.stopped && m.length_s.unwrap() > 12.0, "{m:?}");
}
