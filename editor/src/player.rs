//! Playing a sample or a clip through the default output (cpal): the mapping view's click on a
//! row, the Compare page's A/B. The audio callback never blocks: it plays silence while the UI
//! swaps what plays.
use crate::audio::{Clip, decode};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Playback {
    data: Vec<f32>,
    pos: usize,
}

pub struct Player {
    _stream: cpal::Stream,
    shared: Arc<Mutex<Playback>>,
    rate: u32,
    channels: usize,
}

/// a clip at the output's rate and channel count: linear interpolation; one channel to every
/// output, several to the outputs of their number (the last one repeated), or mixed to one
pub fn convert(clip: &Clip, rate: u32, channels: usize) -> Vec<f32> {
    let cin = clip.channels.max(1);
    let frames = clip.data.len() / cin;
    if frames == 0 || rate == 0 {
        return Vec::new();
    }
    let out_frames = ((frames as u64 * rate as u64) / clip.rate as u64).max(1) as usize;
    let step = clip.rate as f64 / rate as f64;
    let at = |f: usize, c: usize| clip.data[f.min(frames - 1) * cin + c];
    let mut out = Vec::with_capacity(out_frames * channels);
    for t in 0..out_frames {
        let src = t as f64 * step;
        let (i, frac) = (src.floor() as usize, (src - src.floor()) as f32);
        for c in 0..channels {
            let v = |f: usize| {
                if channels == 1 && cin > 1 {
                    (0..cin).map(|k| at(f, k)).sum::<f32>() / cin as f32
                } else {
                    at(f, c.min(cin - 1))
                }
            };
            out.push(v(i) * (1.0 - frac) + v(i + 1) * frac);
        }
    }
    out
}

pub fn gained(clip: &Clip, gain_db: f32) -> Clip {
    let g = 10f32.powf(gain_db / 20.0);
    Clip { rate: clip.rate, channels: clip.channels, data: clip.data.iter().map(|v| v * g).collect() }
}

/// the channels mixed to one (the Compare page's Mono)
pub fn mono(clip: &Clip) -> Clip {
    Clip { rate: clip.rate, channels: 1, data: clip.mono() }
}

/// `a`, `gap_s` of silence, then `b` at `a`'s rate and channel count
pub fn a_then_b(a: &Clip, b: &Clip, gap_s: f32) -> Clip {
    let c = a.channels.max(1);
    let mut data = a.data.clone();
    data.extend(std::iter::repeat_n(0.0, (gap_s * a.rate as f32).round() as usize * c));
    data.extend(convert(b, a.rate, c));
    Clip { rate: a.rate, channels: c, data }
}

/// where the other clip is at the same time since its onset as `pos_s` is in this one
pub fn swap_to(pos_s: f32, from_onset_s: f32, to_onset_s: f32) -> f32 {
    (to_onset_s + pos_s - from_onset_s).max(0.0)
}

impl Player {
    pub fn new() -> Result<Player, String> {
        let device = cpal::default_host().default_output_device().ok_or("no audio output")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        if supported.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!("the output's format is {:?}; the player plays f32", supported.sample_format()));
        }
        let config: cpal::StreamConfig = supported.into();
        let (rate, channels) = (config.sample_rate, config.channels as usize);
        let shared = Arc::new(Mutex::new(Playback::default()));
        let cb = shared.clone();
        let stream = device
            .build_output_stream(
                config,
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| match cb.try_lock() {
                    Ok(mut p) => {
                        let n = out.len().min(p.data.len().saturating_sub(p.pos));
                        out[..n].copy_from_slice(&p.data[p.pos..p.pos + n]);
                        out[n..].fill(0.0);
                        p.pos += n;
                    }
                    Err(_) => out.fill(0.0),
                },
                |e| eprintln!("pianoengine-editor: audio: {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Player { _stream: stream, shared, rate, channels })
    }

    /// the file from its start (up to 20 s), replacing what plays
    pub fn play(&self, path: &Path) -> Result<(), String> {
        let data = convert(&decode(path, Some(20.0))?, self.rate, self.channels);
        let mut p = self.shared.lock().unwrap();
        p.data = data;
        p.pos = 0;
        Ok(())
    }

    pub fn stop(&self) {
        let mut p = self.shared.lock().unwrap();
        p.data.clear();
        p.pos = 0;
    }

    /// a clip from `from_s`, replacing what plays
    pub fn play_clip(&self, clip: &Clip, from_s: f32) {
        let data = convert(clip, self.rate, self.channels);
        let start = ((from_s.max(0.0) * self.rate as f32) as usize * self.channels).min(data.len());
        let mut p = self.shared.lock().unwrap();
        p.data = data;
        p.pos = start;
    }

    /// seconds into what plays (none when nothing does)
    pub fn position_s(&self) -> Option<f32> {
        let p = self.shared.lock().unwrap();
        (p.pos < p.data.len()).then(|| (p.pos / self.channels.max(1)) as f32 / self.rate as f32)
    }

    pub fn playing(&self) -> bool {
        self.position_s().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_resamples_and_spreads_channels() {
        let mono = Clip { rate: 24000, channels: 1, data: vec![0.0, 1.0] };
        assert_eq!(convert(&mono, 48000, 2), vec![0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0]);
        let stereo = Clip { rate: 48000, channels: 2, data: vec![0.1, 0.2, 0.3, 0.4] };
        let mono = convert(&stereo, 48000, 1);
        assert!(mono.len() == 2 && (mono[0] - 0.15).abs() < 1e-6 && (mono[1] - 0.35).abs() < 1e-6, "{mono:?}");
        assert_eq!(convert(&stereo, 48000, 2), vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn a_then_b_puts_silence_between() {
        let a = Clip { rate: 10, channels: 1, data: vec![1.0; 10] };
        let b = Clip { rate: 10, channels: 1, data: vec![0.5; 5] };
        let ab = a_then_b(&a, &b, 0.3);
        assert_eq!((ab.rate, ab.channels), (10, 1));
        assert_eq!(ab.data.len(), 10 + 3 + 5);
        assert!(ab.data[10..13].iter().all(|v| *v == 0.0) && ab.data[13] == 0.5);
    }

    #[test]
    fn gained_scales_by_decibels() {
        let c = gained(&Clip { rate: 1, channels: 1, data: vec![0.5] }, -6.0206);
        assert!((c.data[0] - 0.25).abs() < 1e-4);
    }

    #[test]
    fn swap_keeps_the_time_since_onset() {
        assert!((swap_to(2.0, 0.5, 0.1) - 1.6).abs() < 1e-6);
        assert_eq!(swap_to(0.2, 0.5, 0.0), 0.0);
    }

    #[test]
    fn mono_mixes_the_channels() {
        let c = mono(&Clip { rate: 10, channels: 2, data: vec![1.0, 0.0, 0.5, 0.5] });
        assert_eq!((c.channels, c.data), (1, vec![0.5, 0.5]));
    }
}
