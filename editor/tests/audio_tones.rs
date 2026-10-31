// The synthetic tones the audio and suggestion tests write (included by both).
use std::path::Path;

#[allow(dead_code)]
pub fn write_tone(path: &Path, f0: f32, amp: f32, ratios: &[(f32, f32)], silence_s: f32) {
    let rate = 48000u32;
    let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let n0 = (silence_s * rate as f32) as usize;
    for _ in 0..n0 {
        w.write_sample(0.0f32).unwrap();
    }
    for i in 0..(1.4 * rate as f32) as usize {
        let t = i as f32 / rate as f32;
        let mut s = 0.0;
        for (r, a) in ratios {
            s += a * (2.0 * std::f32::consts::PI * f0 * r * t).sin();
        }
        w.write_sample(amp * s * (-1.5 * t).exp()).unwrap();
    }
    w.finalize().unwrap();
}

#[allow(dead_code)]
pub fn harmonic() -> Vec<(f32, f32)> {
    (1..=8).map(|k| (k as f32, 1.0 / k as f32)).collect()
}

/// partials (ratio, amplitude, decay rate in 1/s), 2.4 s after the silence
#[allow(dead_code)]
pub fn write_partials(path: &Path, f0: f32, partials: &[(f32, f32, f32)], silence_s: f32) {
    let rate = 48000u32;
    let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..(silence_s * rate as f32) as usize {
        w.write_sample(0.0f32).unwrap();
    }
    for i in 0..(2.4 * rate as f32) as usize {
        let t = i as f32 / rate as f32;
        let s: f32 = partials.iter().map(|(r, a, sigma)| a * (-sigma * t).exp() * (2.0 * std::f32::consts::PI * f0 * r * t).sin()).sum();
        w.write_sample(0.3 * s).unwrap();
    }
    w.finalize().unwrap();
}
