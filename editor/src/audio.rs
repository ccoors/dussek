//! What the suggestion and the mapping view measure in a sample: its onset, its level over the
//! first 0.5 s, and its strongest spectral peaks over the first second (the pitch scores read
//! them); cached per project (cache/editor/audio.json) by path, size and modification time.
use crate::project::write_atomic;
use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub const EXTENSIONS: [&str; 5] = ["wav", "flac", "aif", "aiff", "ogg"];

pub fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

#[derive(Clone)]
pub struct Clip {
    pub rate: u32,
    pub channels: usize,
    pub data: Vec<f32>,
}

impl Clip {
    pub fn mono(&self) -> Vec<f32> {
        let c = self.channels.max(1);
        self.data.chunks(c).map(|f| f.iter().sum::<f32>() / c as f32).collect()
    }
}

/// a file's samples: symphonia, and for a WAV it refuses (a PCM fmt chunk longer than its
/// format needs: some sets' files) the plain reader below
pub fn decode(path: &Path, max_seconds: Option<f32>) -> Result<Clip, String> {
    decode_symphonia(path, max_seconds).or_else(|e| {
        let wav = path.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("wav"));
        if wav { decode_wav(path, max_seconds).map_err(|_| e) } else { Err(e) }
    })
}

/// a RIFF WAVE file of integer PCM (8 … 32 bit) or 32-bit float, any fmt chunk length
fn decode_wav(path: &Path, max_seconds: Option<f32>) -> Result<Clip, String> {
    let b = std::fs::read(path).map_err(|e| e.to_string())?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("not a RIFF WAVE file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (mut fmt, mut data) = (None, None);
    let mut i = 12;
    while i + 8 <= b.len() {
        let (id, len) = (&b[i..i + 4], u32_at(i + 4) as usize);
        let body = i + 8;
        let end = (body + len).min(b.len());
        if id == b"fmt " && len >= 16 {
            fmt = Some(body);
        } else if id == b"data" {
            data = Some((body, end));
        }
        i = body + len + (len & 1);
    }
    let (f, (d0, d1)) = (fmt.ok_or("no fmt chunk")?, data.ok_or("no data chunk")?);
    let mut tag = u16_at(f);
    if tag == 0xFFFE && u16_at(f + 16) >= 22 {
        tag = u16_at(f + 24); // WAVE_FORMAT_EXTENSIBLE: the subformat's first two bytes
    }
    let (channels, rate, bits) = (u16_at(f + 2) as usize, u32_at(f + 4), u16_at(f + 14) as usize);
    let width = bits.div_ceil(8);
    if channels == 0 || width == 0 || !(tag == 1 || (tag == 3 && bits == 32)) {
        return Err(format!("WAV format {tag} with {bits} bits"));
    }
    let mut n = (d1 - d0) / width;
    if let Some(s) = max_seconds {
        n = n.min((s * rate as f32) as usize * channels);
    }
    let data = (0..n).map(|k| {
        let o = d0 + k * width;
        if tag == 3 {
            return f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        }
        if width == 1 {
            return (b[o] as f32 - 128.0) / 128.0;
        }
        let mut v: i32 = 0;
        for j in 0..width {
            v |= (b[o + j] as i32) << (8 * j + 32 - 8 * width);
        }
        v as f32 / 2147483648.0
    }).collect();
    Ok(Clip { rate, channels, data })
}

fn decode_symphonia(path: &Path, max_seconds: Option<f32>) -> Result<Clip, String> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| err(&e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| err(&e))?;
    let track = format.default_track(TrackType::Audio).ok_or_else(|| err(&"no audio track"))?;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(|| err(&"no audio codec"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| err(&e))?;
    let id = track.id;
    let (mut rate, mut channels, mut data, mut buf) = (0u32, 1usize, Vec::new(), Vec::new());
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(err(&e)),
        };
        if packet.track_id != id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(b) => {
                rate = b.spec().rate();
                channels = b.spec().channels().count().max(1);
                buf.resize(b.samples_interleaved(), 0.0f32);
                b.copy_to_slice_interleaved(&mut buf);
                data.extend_from_slice(&buf);
                if max_seconds.is_some_and(|s| data.len() >= (s * rate as f32) as usize * channels) {
                    break;
                }
            }
            Err(Error::DecodeError(_)) => {}
            Err(e) => return Err(err(&e)),
        }
    }
    if rate == 0 {
        return Err(err(&"no audio decoded"));
    }
    Ok(Clip { rate, channels, data })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub onset_s: f32,
    pub level_db: f32,
    pub nyquist: f32,
    /// [Hz, dB re the strongest], strongest first
    pub peaks: Vec<[f32; 2]>,
    /// [Hz, decay rate in 1/s] of the strongest distinct peaks, strongest first
    pub decays: Vec<[f32; 2]>,
    /// the level in ENV_HOP_S frames from the onset to the end (at most MEASURE_MAX_S), dBFS
    pub env_db: Vec<f32>,
    /// the power in 1/6-octave bands (band_hz) over the first 0.5 s from the onset, dB
    pub bands_db: Vec<f32>,
}

/// how much of a file is decoded for its measurement (the whole decay of most notes)
pub const MEASURE_MAX_S: f32 = 30.0;
/// the peaks, decays and level read the first 2.5 s of the file, as before the envelope
const HEAD_S: f32 = 2.5;
/// the envelope's frame
pub const ENV_HOP_S: f32 = 0.05;

/// the center of 1/6-octave band k (from 20 Hz)
pub fn band_hz(k: usize) -> f32 {
    20.0 * 2f32.powf(k as f32 / 6.0)
}

pub fn measure(path: &Path) -> Result<Measure, String> {
    let c = decode(path, Some(MEASURE_MAX_S))?;
    let all = c.mono();
    let rate = c.rate as f32;
    let x = &all[..all.len().min((HEAD_S * rate) as usize)];
    let on = onset(x);
    let from = on + (0.05 * rate) as usize;
    let peaks = peaks(x, from, rate as usize, rate);
    Ok(Measure {
        onset_s: on as f32 / rate,
        level_db: level_db(x, on, (0.5 * rate) as usize),
        nyquist: rate / 2.0,
        decays: decays(x, from, rate, &peaks),
        peaks,
        env_db: envelope_db(&all, on, rate),
        bands_db: bands_db(&all, on, rate),
    })
}

/// the level in ENV_HOP_S frames from `from` to the end, dBFS (level_db: −120 when silent)
pub fn envelope_db(x: &[f32], from: usize, rate: f32) -> Vec<f32> {
    let hop = (ENV_HOP_S * rate) as usize;
    (from..x.len()).step_by(hop.max(1)).filter(|i| i + hop <= x.len()).map(|i| level_db(x, i, hop)).collect()
}

/// the power in each 1/6-octave band (band_hz, edges a 1/12 octave either side) below the Nyquist
/// frequency over [from, from + 0.5 s), Hann, dB
pub fn bands_db(x: &[f32], from: usize, rate: f32) -> Vec<f32> {
    let from = from.min(x.len());
    let n = ((0.5 * rate) as usize).min(x.len() - from);
    let mut out = Vec::new();
    if n < 256 {
        return out;
    }
    let size = n.next_power_of_two();
    let mut input = vec![0.0f32; size];
    for i in 0..n {
        let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos();
        input[i] = x[from + i] * w;
    }
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
    let mut spec = fft.make_output_vec();
    fft.process(&mut input, &mut spec).unwrap();
    let bin_hz = rate / size as f32;
    let mut k = 0;
    while band_hz(k) < rate / 2.0 {
        let (lo, hi) = (band_hz(k) * 2f32.powf(-1.0 / 12.0), band_hz(k) * 2f32.powf(1.0 / 12.0));
        let p: f32 = spec.iter().enumerate().filter(|(i, _)| { let f = *i as f32 * bin_hz; f >= lo && f < hi })
            .map(|(_, c)| c.norm_sqr()).sum();
        out.push(10.0 * (p + 1e-20).log10());
        k += 1;
    }
    out
}

/// the first sample at 10 % of the peak
pub fn onset(x: &[f32]) -> usize {
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak == 0.0 { 0 } else { x.iter().position(|v| v.abs() >= 0.1 * peak).unwrap_or(0) }
}

/// RMS over [from, from + n) in dBFS (−120 when empty or silent)
pub fn level_db(x: &[f32], from: usize, n: usize) -> f32 {
    let s = &x[from.min(x.len())..(from + n).min(x.len())];
    if s.is_empty() {
        return -120.0;
    }
    let rms = (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt();
    20.0 * rms.max(1e-6).log10()
}

pub fn peaks(x: &[f32], from: usize, n: usize, rate: f32) -> Vec<[f32; 2]> {
    let from = from.min(x.len());
    let n = n.min(x.len() - from);
    if n < 256 {
        return Vec::new();
    }
    let size = (4 * n).next_power_of_two();
    let mut input = vec![0.0f32; size];
    for i in 0..n {
        let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos();
        input[i] = x[from + i] * w;
    }
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
    let mut spec = fft.make_output_vec();
    fft.process(&mut input, &mut spec).unwrap();
    let mag: Vec<f32> = spec.iter().map(|c| c.norm()).collect();
    let max = mag.iter().cloned().fold(0.0f32, f32::max);
    if max == 0.0 {
        return Vec::new();
    }
    let bin_hz = rate / size as f32;
    let floor = max * 1e-3; // −60 dB
    let mut out: Vec<[f32; 2]> = Vec::new();
    for i in 1..mag.len() - 1 {
        if mag[i] > mag[i - 1] && mag[i] >= mag[i + 1] && mag[i] > floor && i as f32 * bin_hz >= 20.0 {
            let (a, b, c) = (mag[i - 1].max(1e-12).ln(), mag[i].ln(), mag[i + 1].max(1e-12).ln());
            let d = a - 2.0 * b + c;
            let p = if d != 0.0 { (0.5 * (a - c) / d).clamp(-0.5, 0.5) } else { 0.0 };
            out.push([(i as f32 + p) * bin_hz, 20.0 * (mag[i] / max).log10()]);
        }
    }
    out.sort_by(|a, b| b[1].total_cmp(&a[1]));
    out.truncate(60);
    out
}

/// the strongest peaks a decay is measured for; peaks within 3 % of a stronger one are its
const DECAY_PEAKS: usize = 8;
/// the decay is fitted while the level stays within this of its start
const DECAY_RANGE_DB: f32 = 45.0;

/// each of the strongest distinct peaks' decay rate (1/s): the slope of its level in 50 ms frames
/// (Hann, half overlapping) from `from` until it falls DECAY_RANGE_DB or the recording ends;
/// fewer than three frames: none
pub fn decays(x: &[f32], from: usize, rate: f32, peaks: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let n = (0.05 * rate) as usize;
    let hop = n / 2;
    let frames = x.len().saturating_sub(from + n) / hop + 1;
    if x.len() < from + n {
        return Vec::new();
    }
    let mut taken: Vec<f32> = Vec::new();
    let mut out = Vec::new();
    for p in peaks {
        if taken.len() == DECAY_PEAKS {
            break;
        }
        if taken.iter().any(|f| (p[0] / f - 1.0).abs() < 0.03) {
            continue;
        }
        taken.push(p[0]);
        let lv = track(x, from, rate, p[0], frames);
        let first = lv[0];
        let pts: Vec<(f32, f32)> = lv.iter().enumerate().map(|(k, l)| (k as f32 * hop as f32 / rate, *l))
            .take_while(|(_, l)| *l > first - DECAY_RANGE_DB).collect();
        if let Some(r) = rate_of(&pts) {
            out.push([p[0], r]);
        }
    }
    out
}

/// a partial's level (dB) in 50 ms Hann frames, 25 ms apart, from `from`: at most `frames`,
/// fewer where the signal ends
pub fn track(x: &[f32], from: usize, rate: f32, hz: f32, frames: usize) -> Vec<f32> {
    let n = (0.05 * rate) as usize;
    let hop = n / 2;
    if x.len() < from + n {
        return Vec::new();
    }
    let have = (x.len() - from - n) / hop + 1;
    // the window times the partial's complex exponential
    let dw = 2.0 * std::f64::consts::PI * hz as f64 / rate as f64;
    let kern: Vec<(f32, f32)> = (0..n).map(|i| {
        let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos();
        let a = dw * i as f64;
        (w * a.cos() as f32, -w * a.sin() as f32)
    }).collect();
    (0..have.min(frames)).map(|k| {
        let s = &x[from + k * hop..from + k * hop + n];
        let (re, im) = s.iter().zip(&kern).fold((0.0f32, 0.0f32), |(re, im), (v, c)| (re + v * c.0, im + v * c.1));
        20.0 * (re * re + im * im).sqrt().max(1e-12).log10()
    }).collect()
}

/// a line's slope through (t, dB) points as a decay rate (1/s; 20 log10 e = 8.686 dB per
/// neper); fewer than three points: none
fn rate_of(pts: &[(f32, f32)]) -> Option<f32> {
    line(pts).map(|(_, _, slope)| (-slope / 8.686).max(0.0))
}

/// a line through (t, dB) points: its mean time, mean level and slope (dB/s); fewer than three
/// points: none
fn line(pts: &[(f32, f32)]) -> Option<(f32, f32, f32)> {
    if pts.len() < 3 {
        return None;
    }
    let m = pts.len() as f32;
    let (tm, lm) = (pts.iter().map(|q| q.0).sum::<f32>() / m, pts.iter().map(|q| q.1).sum::<f32>() / m);
    let (sxy, sxx) = pts.iter().fold((0.0, 0.0), |(a, b), q| (a + (q.0 - tm) * (q.1 - lm), b + (q.0 - tm) * (q.0 - tm)));
    (sxx > 0.0).then(|| (tm, lm, sxy / sxx))
}

/// how well a line fits (t, dB) points: r² (none: fewer than three points, or a flat line)
fn r2(pts: &[(f32, f32)]) -> Option<f32> {
    let (tm, lm, slope) = line(pts)?;
    let (sxx, syy) = pts.iter().fold((0.0, 0.0), |(a, b), q| (a + (q.0 - tm).powi(2), b + (q.1 - lm).powi(2)));
    (syy > 0.0).then(|| slope * slope * sxx / syy)
}

/// a spectrum's shape: its partials (ratio, tolerance in cents), its weight, whether its
/// fundamental must sound and whether a harmonic series rules it out (a bar's)
pub struct Profile {
    pub partials: &'static [(f32, f32)],
    pub weight: f32,
    pub fundamental: bool,
    pub no_series: bool,
}

/// harmonic; bars tuned 1 : 4 : 10; free bars, their modes spread as the committed bars' are
/// (the second 2.66 … 3.29, the third 4.9 … 7.2: the textbook's 2.76 and 5.40 within), with
/// their fundamental (a reading below a plate's partials puts them in the bands). Neither bar
/// sounds a harmonic series, peaks at twice and three times the fundamental: every series holds
/// 1, 4 and 10 (with weak partials between, piano keys read as tuned bars), a string's third
/// harmonic lies in the free bar's band; a bar's mode may lie at the octave alone (the crotales')
pub const PROFILES: [Profile; 3] = [
    Profile { partials: &[(1.0, 40.0), (2.0, 40.0), (3.0, 40.0), (4.0, 40.0), (5.0, 40.0), (6.0, 40.0), (7.0, 40.0), (8.0, 40.0)],
        weight: 1.0, fundamental: false, no_series: false },
    Profile { partials: &[(1.0, 40.0), (4.0, 40.0), (10.0, 40.0)], weight: 0.8, fundamental: false, no_series: true },
    Profile { partials: &[(1.0, 40.0), (2.96, 185.0), (5.94, 340.0)], weight: 0.8, fundamental: true, no_series: true },
];

pub fn midi_hz(note: f64) -> f32 {
    (440.0 * 2f64.powf((note - 69.0) / 12.0)) as f32
}

fn power(db: f32) -> f32 {
    10f32.powf(db / 10.0)
}

fn cents(f: f32, target: f32) -> f32 {
    1200.0 * (f / target).log2()
}

fn usable(partials: &[(f32, f32)], f0: f32, nyquist: f32) -> Vec<(f32, f32)> {
    let limit = (0.9 * nyquist).min(16000.0);
    partials.iter().copied().filter(|(r, _)| r * f0 <= limit).collect()
}

fn profile_score(peaks: &[[f32; 2]], f0: f32, profile: &Profile, nyquist: f32) -> f32 {
    let rs = usable(profile.partials, f0, nyquist);
    // one partial in range fits any reading: a profile needs two
    if rs.len() < 2 {
        return 0.0;
    }
    let at = |r: f32| peaks.iter().any(|p| p[1] >= -30.0 && cents(p[0], r * f0).abs() <= 40.0);
    if (profile.fundamental && !at(1.0)) || (profile.no_series && at(2.0) && at(3.0)) {
        return 0.0;
    }
    let (r_max, tol_max) = rs[rs.len() - 1];
    let (lo, hi) = (0.7 * f0, 1.1 * r_max * f0 * 2f32.powf(tol_max / 1200.0));
    let total: f32 = peaks.iter().filter(|p| p[0] >= lo && p[0] <= hi).map(|p| power(p[1])).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let (mut captured, mut hits) = (0.0f32, 0usize);
    for (r, tol) in &rs {
        let best = peaks.iter().filter(|p| cents(p[0], r * f0).abs() <= *tol).map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        if best.is_finite() {
            captured += power(best);
            if best >= -30.0 {
                hits += 1;
            }
        }
    }
    (captured / total).min(1.0) * hits as f32 / rs.len() as f32
}

/// how well a reading's fundamental f0 explains the peaks (0 … 1) and the profile that does best
pub fn score(peaks: &[[f32; 2]], f0: f32, nyquist: f32) -> (f32, usize) {
    PROFILES.iter().enumerate()
        .map(|(i, p)| (p.weight * profile_score(peaks, f0, p, nyquist), i))
        .fold((0.0, 0), |a, b| if b.0 > a.0 { b } else { a })
}

/// the strongest partial's distance in cents from where the reading puts it
pub fn deviation_cents(peaks: &[[f32; 2]], f0: f32, nyquist: f32) -> Option<f32> {
    let (_, k) = score(peaks, f0, nyquist);
    let mut best: Option<(f32, f32)> = None;
    // the partials a profile places exactly (a free bar's upper modes are anywhere in a band)
    for (r, _) in usable(PROFILES[k].partials, f0, nyquist).into_iter().filter(|(_, tol)| *tol <= 40.0) {
        for p in peaks.iter().filter(|p| cents(p[0], r * f0).abs() <= 150.0) {
            if best.is_none_or(|(db, _)| p[1] > db) {
                best = Some((p[1], cents(p[0], r * f0)));
            }
        }
    }
    best.map(|(_, c)| c)
}

#[derive(Clone, Debug, Default)]
pub struct Measures {
    pub ok: BTreeMap<String, Measure>,
    pub failed: BTreeMap<String, String>,
    /// how many files were decoded (not taken from the cache)
    pub decoded: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    files: BTreeMap<String, Entry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    size: u64,
    mtime_ns: u128,
    measure: Option<Measure>,
    error: Option<String>,
}

/// raised when the decoding or the measurements change: older entries are measured again
const CACHE_VERSION: u32 = 4;

fn stamp(path: &Path) -> Option<(u64, u128)> {
    let md = std::fs::metadata(path).ok()?;
    let t = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some((md.len(), t))
}

/// every path measured, from the cache where its size and modification time match, the rest
/// decoded on all cores; `progress` counts the files done
pub fn measure_all(paths: &[PathBuf], cache: &Path, progress: &AtomicUsize) -> Measures {
    let old: CacheFile = std::fs::read_to_string(cache).ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(|c: &CacheFile| c.version == CACHE_VERSION)
        .unwrap_or_default();
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    let mut todo: Vec<(String, &PathBuf, Option<(u64, u128)>)> = Vec::new();
    for p in paths {
        let key = p.to_string_lossy().into_owned();
        let st = stamp(p);
        match (old.files.get(&key), st) {
            (Some(e), Some((size, t))) if e.size == size && e.mtime_ns == t => {
                entries.insert(key, e.clone());
                progress.fetch_add(1, Ordering::Relaxed);
            }
            _ => todo.push((key, p, st)),
        }
    }
    let decoded = todo.len();
    let next = AtomicUsize::new(0);
    let done = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((key, path, st)) = todo.get(i) else { break };
                let (size, t) = st.unwrap_or((0, 0));
                let e = match measure(path) {
                    Ok(m) => Entry { size, mtime_ns: t, measure: Some(m), error: None },
                    Err(err) => Entry { size, mtime_ns: t, measure: None, error: Some(err) },
                };
                done.lock().unwrap().push((key.clone(), e));
                progress.fetch_add(1, Ordering::Relaxed);
            });
        }
    });
    entries.extend(done.into_inner().unwrap());
    let mut file = old;
    file.version = CACHE_VERSION;
    // the cache keeps the files that are still there
    file.files.retain(|k, _| Path::new(k).exists());
    file.files.extend(entries.clone());
    if let Some(dir) = cache.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = write_atomic(cache, &serde_json::to_string(&file).unwrap_or_default());
    let mut out = Measures { decoded, ..Default::default() };
    for (k, e) in entries {
        match (e.measure, e.error) {
            (Some(m), _) => {
                out.ok.insert(k, m);
            }
            (None, err) => {
                out.failed.insert(k, err.unwrap_or_default());
            }
        }
    }
    out
}

/// the cache's entries for the files under `under` written to `to` (the wizard's measurements into
/// a new project's cache); how many
pub fn copy_entries(from: &Path, to: &Path, under: &Path) -> Result<usize, String> {
    let text = std::fs::read_to_string(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let mut file: CacheFile = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", from.display()))?;
    file.files.retain(|k, _| Path::new(k).starts_with(under));
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    write_atomic(to, &serde_json::to_string(&file).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(file.files.len())
}

/// the Compare page's longest stretch after the onset (s)
pub const LONG_MAX_S: f32 = 30.0;
/// a render's partial where the recording has one and the render none (dB re its strongest)
pub const UNMATCHED_DB: f32 = -60.0;
/// the early segment's end (s after the onset; it starts at DECAY_FROM_S)
pub const EARLY_END_S: f32 = 1.0;
const DECAY_FROM_S: f32 = 0.05;
/// the partials tracked (the strongest distinct, as `decays`)
const LONG_PARTIALS: usize = 8;
/// the noise floor: the quietest stretch of this length from FLOOR_FROM_S after the onset
const FLOOR_WINDOW_S: f32 = 0.2;
const FLOOR_FROM_S: f32 = 0.5;
const FLOOR_MIN_DB: f32 = -100.0;
/// the end: where the level stays within this of the floor
const END_ABOVE_FLOOR_DB: f32 = 6.0;
/// a file cut while still ringing: its last CUT_WINDOW_S still falls faster than CUT_SLOPE_DB_S
/// (a noise floor is flat; a piano's slowest aftersound falls about 1.7 dB/s)
const CUT_WINDOW_S: f32 = 1.0;
const CUT_SLOPE_DB_S: f32 = 1.0;
/// … along a clean line (r² at least this): a floor that wobbles a few dB read as a fall, its
/// noise as signal
const CUT_R2: f32 = 0.6;
/// a partial's late fit ends where it has fallen this far from its level at DECAY_FROM_S
const LONG_RANGE_DB: f32 = 50.0;
/// the length: how long the note's band (from PARTIAL_FROM × its frequency: a room's rumble under
/// it held the level up) takes to fall this far under its level over the first 0.5 s (what Level
/// match plays at), or as far as the recording shows it, LENGTH_ABOVE_FLOOR_DB above its quietest
/// window; at least LENGTH_MIN_DB. The ear judges the ring, not the strike: 30 dB under the first
/// 0.1 s fell within a piano's first drop (a set with 40 dB under its start otherwise read as
/// stopped on most keys; 6 dB above the floor the noise still adds 1.3 dB, 10 dB above it 0.4 dB)
pub const LENGTH_DB: f32 = 50.0;
pub const LENGTH_MIN_DB: f32 = 10.0;
const LENGTH_ABOVE_FLOOR_DB: f32 = 10.0;
/// a stop (a damper, a hand, a fade-out): from STOP_FROM_S on, the broadband level falls
/// STOP_DROP_DB under the line of the second before within STOP_AFTER_S and does not come back
/// within STOP_RECOVER_DB of it for STOP_HOLD_S (frames in the noise floor aside). A string's or
/// bar's decay only slows, and a beat's dip comes back (a slope over a short window read the
/// beating of a guitar's or piano's level, and a slow piano bass's wobble, as stops)
pub const STOP_DROP_DB: f32 = 10.0;
const STOP_RECOVER_DB: f32 = 6.0;
const STOP_FROM_S: f32 = 1.0;
const STOP_AFTER_S: f32 = 0.5;
const STOP_HOLD_S: f32 = 2.0;
/// where the stop began: the last frame before the drop within this of the line
const STOP_KNEE_DB: f32 = 3.0;
/// a partial counts in the spectrum this far above its own noise
pub const PARTIAL_SNR_DB: f32 = 20.0;
/// the partials tracked lie from this times the sounding note's frequency up
const PARTIAL_FROM: f32 = 0.7;
/// a render's partial matches the recording's within this
const MATCH_CENTS: f32 = 50.0;
/// the fundamental: the strongest peak this near the sounding note, at most FUNDAMENTAL_DB under
/// the strongest peak
const FUNDAMENTAL_CENTS: f32 = 150.0;
const FUNDAMENTAL_DB: f32 = -40.0;

/// what the Compare page measures in a recording (or a render) over as long as it sounds
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Long {
    pub onset_s: f32,
    /// RMS over the first 0.5 s after the onset (dBFS)
    pub level_db: f32,
    /// the note band's quietest FLOOR_WINDOW_S from FLOOR_FROM_S after the onset (dBFS, at least
    /// FLOOR_MIN_DB)
    pub floor_db: f32,
    /// s after the onset: where the level stays within END_ABOVE_FLOOR_DB of the floor, the
    /// file's end, or LONG_MAX_S
    pub end_s: f32,
    /// s after the onset: where the sound was stopped (its fall sped up; the comparison ends there)
    pub stopped_s: Option<f32>,
    /// s after the onset to fall `length_db` (a lower bound when stopped)
    pub length_s: Option<f32>,
    /// the fall the length is read to: LENGTH_DB, or less above a recording's noise floor (a
    /// render is measured to its recording's)
    pub length_db: f32,
    /// the sound was stopped before it fell `length_db` (a damper, a hand, a fade-out, the file's
    /// end)
    pub stopped: bool,
    /// the peak nearest the sounding note's frequency
    pub fundamental_hz: Option<f32>,
    /// the strongest distinct partials over 0.05–0.55 s, strongest first
    pub partials: Vec<Partial>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Partial {
    pub hz: f32,
    /// dB re the strongest peak (over 0.05–0.55 s)
    pub db: f32,
    /// decay rates (1/s): DECAY_FROM_S … EARLY_END_S, and EARLY_END_S … until_s
    pub early: Option<f32>,
    pub late: Option<f32>,
    /// s after the onset where its fit ends
    pub until_s: f32,
    /// its level over the first 0.5 s above its own noise (dB)
    pub snr_db: f32,
}

fn hop_s(rate: f32) -> f32 {
    ((0.05 * rate) as usize / 2) as f32 / rate
}

/// the early and late rates of a partial's track (frame k at DECAY_FROM_S + k·hop after the
/// onset) up to `until_s`
fn two_rates(lv: &[f32], hop: f32, until_s: f32) -> (Option<f32>, Option<f32>) {
    // a frame counts while its whole window (two hops from its time) lies before the end
    let pts: Vec<(f32, f32)> = lv.iter().enumerate().map(|(k, l)| (DECAY_FROM_S + k as f32 * hop, *l))
        .take_while(|(t, _)| *t + 2.0 * hop <= until_s + 1e-4).collect();
    let early: Vec<(f32, f32)> = pts.iter().copied().filter(|(t, _)| *t <= EARLY_END_S).collect();
    let late: Vec<(f32, f32)> = pts.iter().copied().filter(|(t, _)| *t >= EARLY_END_S).collect();
    // a late segment shorter than 0.2 s says nothing
    let late_ok = late.last().is_some_and(|l| l.0 - EARLY_END_S >= 0.2);
    (rate_of(&early), if late_ok { rate_of(&late) } else { None })
}

/// the fundamental: the strongest peak near the sounding note
fn fundamental(peaks: &[[f32; 2]], note: f64) -> Option<f32> {
    let f = midi_hz(note);
    peaks.iter().filter(|p| p[1] >= FUNDAMENTAL_DB && cents(p[0], f).abs() <= FUNDAMENTAL_CENTS)
        .max_by(|a, b| a[1].total_cmp(&b[1])).map(|p| p[0])
}

/// the strongest distinct peaks (within 3 % of a stronger one: its)
fn distinct(peaks: &[[f32; 2]], n: usize) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::new();
    for p in peaks {
        if out.len() == n {
            break;
        }
        if !out.iter().any(|q| (p[0] / q[0] - 1.0).abs() < 0.03) {
            out.push(*p);
        }
    }
    out
}

/// the broadband level (dB RMS) in 50 ms frames, 25 ms apart, from `from`
pub fn broadband(x: &[f32], from: usize, rate: f32, frames: usize) -> Vec<f32> {
    let n = (0.05 * rate) as usize;
    let hop = n / 2;
    (0..frames).take_while(|k| from + k * hop + n <= x.len()).map(|k| level_db(x, from + k * hop, n)).collect()
}

/// where the broadband level was stopped (where its drop began); none before STOP_FROM_S. `floor`:
/// frames at or under it say nothing of a recovery (none: a cut recording, no floor)
fn stop_at(lv: &[f32], hop: f32, end_s: f32, floor: Option<f32>) -> Option<f32> {
    let time = |k: usize| DECAY_FROM_S + k as f32 * hop;
    let first = lv.iter().enumerate().position(|(k, _)| time(k) >= STOP_FROM_S)?;
    for k in first..lv.len() {
        let t = time(k);
        if t + STOP_AFTER_S > end_s {
            break;
        }
        let before: Vec<(f32, f32)> = (0..=k).map(|j| (time(j), lv[j])).filter(|(s, _)| *s >= t - 1.0).collect();
        let Some((tm, lm, slope)) = line(&before) else { continue };
        let under = |j: usize| lm + slope * (time(j) - tm) - lv[j];
        let Some(drop) = (k + 1..lv.len()).take_while(|j| time(*j) <= t + STOP_AFTER_S).find(|j| under(*j) >= STOP_DROP_DB) else { continue };
        let back = (drop + 1..lv.len()).take_while(|j| time(*j) <= time(drop) + STOP_HOLD_S)
            .any(|j| floor.is_none_or(|f| lv[j] > f) && under(j) < STOP_RECOVER_DB);
        if !back {
            let began = (k..drop).rev().find(|j| under(*j) < STOP_KNEE_DB).unwrap_or(k);
            return Some(time(began) + hop);
        }
    }
    None
}

/// the time after the onset the track is last above `target` (dB; frames at their centers, read
/// between them), before `until` (s); still above it there: `until` (a lower bound) and `true`
fn length_of(lv: &[f32], hop: f32, until: f32, target: f32) -> (Option<f32>, bool) {
    let time = |k: usize| DECAY_FROM_S + k as f32 * hop + hop;
    let n = lv.iter().enumerate().take_while(|(k, _)| time(*k) <= until).count();
    if n == 0 {
        return (None, false);
    }
    match (0..n).rev().find(|k| lv[*k] > target) {
        None => (Some(time(0)), false),
        Some(k) if k + 1 == n => (Some(time(k).min(until)), true),
        Some(k) => {
            let (l0, l1) = (lv[k], lv[k + 1]);
            (Some(time(k) + (l0 - target) / (l0 - l1).max(1e-6) * hop), false)
        }
    }
}

/// `x` high-passed at `hz` (two 2nd-order sections, 24 dB per octave)
pub fn highpassed(x: &[f32], rate: f32, hz: f32) -> Vec<f32> {
    let w = 2.0 * std::f64::consts::PI * (hz as f64 / rate as f64).min(0.45);
    let (cw, alpha) = (w.cos(), w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2));
    let a0 = 1.0 + alpha;
    let (b0, b1, b2) = ((1.0 + cw) / 2.0 / a0, -(1.0 + cw) / a0, (1.0 + cw) / 2.0 / a0);
    let (a1, a2) = (-2.0 * cw / a0, (1.0 - alpha) / a0);
    let mut y: Vec<f64> = x.iter().map(|v| *v as f64).collect();
    for _ in 0..2 {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for v in y.iter_mut() {
            let x0 = *v;
            let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, x0, y1, y0);
            *v = y0;
        }
    }
    y.into_iter().map(|v| v as f32).collect()
}

fn mean(v: &[f32]) -> Option<f32> {
    (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
}

/// a recording measured over as long as it sounds (`note`: the sounding note)
pub fn measure_long(x: &[f32], rate: f32, note: f64) -> Long {
    let on = onset(x);
    let onset_s = on as f32 / rate;
    let len_s = (x.len() - on) as f32 / rate;
    // the note's band: a room's rumble under the note is none of its sound (it held the level up,
    // ended the fall, set the floor)
    let lowest = PARTIAL_FROM * midi_hz(note);
    let band = highpassed(x, rate, lowest);
    // the floor: the band's quietest window from FLOOR_FROM_S after the onset, half overlapping
    let w = (FLOOR_WINDOW_S * rate) as usize;
    let starts: Vec<usize> = (on + (FLOOR_FROM_S * rate) as usize..x.len().saturating_sub(w)).step_by(w / 2).collect();
    let levels: Vec<(usize, f32)> = starts.iter().map(|&s| (s, level_db(&band, s, w))).collect();
    // a tail a library faded to digital zero is no floor: the quietest window that holds sound
    let heard: Vec<&(usize, f32)> = levels.iter()
        .filter(|l| level_db(x, l.0, w / 2) > FLOOR_MIN_DB && level_db(x, l.0 + w / 2, w - w / 2) > FLOOR_MIN_DB).collect();
    let quiet = heard.iter().min_by(|a, b| a.1.total_cmp(&b.1));
    let floor_db = quiet.map_or(FLOOR_MIN_DB, |l| l.1.max(FLOOR_MIN_DB));
    let quiet_start = quiet.map_or(on, |l| l.0);
    // the end: after the last window louder than the floor by END_ABOVE_FLOOR_DB
    let loud_until = levels.iter().rev().find(|l| l.1 > floor_db + END_ABOVE_FLOOR_DB).map(|l| (l.0 + w - on) as f32 / rate);
    // cut while still ringing: the last second's windows still fall (a floor is flat), so the
    // quietest window is the signal, and the comparison runs to the file's end
    let last: Vec<(f32, f32)> = levels.iter().map(|l| (l.0 as f32 / rate, l.1))
        .filter(|(t, _)| *t >= (x.len() - w) as f32 / rate - CUT_WINDOW_S).collect();
    // (a file ending in digital zero was faded, not cut)
    let faded = levels.last().is_some_and(|l| l.1 <= FLOOR_MIN_DB);
    let cut = !faded && line(&last).is_some_and(|(_, _, slope)| slope < -CUT_SLOPE_DB_S) && r2(&last).is_some_and(|r| r >= CUT_R2);
    let end_s = match loud_until {
        // still loud in the last window: the file's end
        _ if cut => len_s,
        Some(_) if levels.last().is_some_and(|l| l.1 > floor_db + END_ABOVE_FLOOR_DB) => len_s,
        Some(t) => t,
        None => len_s,
    }.min(LONG_MAX_S).min(len_s);
    let from = on + (DECAY_FROM_S * rate) as usize;
    let all = peaks(x, from, (0.5 * rate) as usize, rate);
    let hop = hop_s(rate);
    let frames = ((end_s - DECAY_FROM_S) / hop).max(0.0) as usize + 1;
    let above: Vec<[f32; 2]> = all.iter().copied().filter(|p| p[0] >= lowest).collect();
    let tops = distinct(&above, LONG_PARTIALS);
    let tracks: Vec<Vec<f32>> = tops.iter().map(|p| track(x, from, rate, p[0], frames)).collect();
    let broad = broadband(&band, from, rate, frames);
    let stopped_s = stop_at(&broad, hop, end_s, (!cut).then_some(floor_db + END_ABOVE_FLOOR_DB));
    let stop = stopped_s.unwrap_or(end_s);
    // the fall read as far as the recording shows it above its quietest window (its noise floor,
    // or a cut recording's last of its signal): a shallower fall, not a lower bound
    let head = level_db(&band, on, (0.5 * rate) as usize);
    let length_db = LENGTH_DB.min(head - floor_db - LENGTH_ABOVE_FLOOR_DB);
    let (length_s, short) = if length_db >= LENGTH_MIN_DB { length_of(&broad, hop, stop, head - length_db) } else { (None, false) };
    // each partial's own noise (a recording's noise read as brightness otherwise): its track over
    // the 0.2 s before the onset when the file has them and they are not digital silence, else
    // over the quietest window; a cut recording without them has no noise to read
    let w_frames = (w.saturating_sub((0.05 * rate) as usize) as f32 / (hop * rate)) as usize + 1;
    let pre = (0.05 * rate) as usize + w;
    let noise_from = match () {
        _ if on >= pre && level_db(x, on - pre, w) > FLOOR_MIN_DB => Some(on - pre),
        _ if !cut => Some(quiet_start),
        _ => None,
    };
    let partials = tops.iter().zip(&tracks).map(|(p, lv)| {
        let first = lv.first().copied().unwrap_or(-120.0);
        let fell = lv.iter().position(|l| *l < first - LONG_RANGE_DB).map(|k| DECAY_FROM_S + k as f32 * hop);
        let until_s = fell.map_or(stop, |t| t.min(stop));
        let (early, late) = two_rates(lv, hop, until_s);
        let noise = noise_from.map_or(Vec::new(), |from| track(x, from, rate, p[0], w_frames));
        let head: Vec<f32> = lv.iter().take(((0.5 / hop) as usize).max(1)).copied().collect();
        // the noise in power: a window reaching into a faded tail's digital zero is no quieter
        let noise_db = mean(&noise.iter().map(|db| 10f32.powf(db / 10.0)).collect::<Vec<_>>()).map_or(-240.0, |p| 10.0 * p.max(1e-24).log10());
        let snr_db = (mean(&head).unwrap_or(-240.0) - noise_db).min(240.0);
        Partial { hz: p[0], db: p[1], early, late, until_s, snr_db }
    }).collect();
    Long { onset_s, level_db: level_db(x, on, (0.5 * rate) as usize), floor_db, end_s, stopped_s, length_s,
        length_db, stopped: short, fundamental_hz: fundamental(&all, note), partials }
}

/// a render measured over the recording's times: its own onset, level and fundamental, the
/// recording's end and stop, its own length (no floor: to its own fall), each of the recording's partials at the render's own peak
/// within MATCH_CENTS (none: UNMATCHED_DB, no rates) fitted until the same time
pub fn measure_like(x: &[f32], rate: f32, note: f64, like: &Long) -> Long {
    let on = onset(x);
    let from = on + (DECAY_FROM_S * rate) as usize;
    let all = peaks(x, from, (0.5 * rate) as usize, rate);
    let hop = hop_s(rate);
    // the partials matched after the fundamentals' ratio: a render at another pitch (a set recorded
    // at A = 415) still compares (the pitch shows its offset)
    let fundamental_hz = fundamental(&all, note);
    let shift = match (like.fundamental_hz, fundamental_hz) {
        (Some(a), Some(b)) => b / a,
        _ => 1.0,
    };
    let partials = like.partials.iter().map(|p| {
        let mine = all.iter().filter(|q| cents(q[0], p.hz * shift).abs() <= MATCH_CENTS).max_by(|a, b| a[1].total_cmp(&b[1]));
        match mine {
            Some(q) => {
                let frames = ((p.until_s - DECAY_FROM_S) / hop).max(0.0) as usize + 1;
                let (early, late) = two_rates(&track(x, from, rate, q[0], frames), hop, p.until_s);
                Partial { hz: q[0], db: q[1], early, late, until_s: p.until_s, snr_db: p.snr_db }
            }
            None => Partial { hz: p.hz, db: UNMATCHED_DB, early: None, late: None, until_s: p.until_s, snr_db: p.snr_db },
        }
    }).collect();
    // the render's own fall in the note's band over its whole length, to the recording's depth
    // under its own first 0.5 s: a lower bound when it did not fall in time
    let all_frames = (x.len().saturating_sub(from) as f32 / (hop * rate)) as usize + 1;
    let (length_s, short) = match like.length_s {
        Some(_) => {
            let band = highpassed(x, rate, PARTIAL_FROM * midi_hz(note));
            let target = level_db(&band, on, (0.5 * rate) as usize) - like.length_db;
            length_of(&broadband(&band, from, rate, all_frames), hop, (x.len() - on) as f32 / rate, target)
        }
        None => (None, false),
    };
    Long { onset_s: on as f32 / rate, level_db: level_db(x, on, (0.5 * rate) as usize), floor_db: FLOOR_MIN_DB,
        end_s: like.end_s, stopped_s: like.stopped_s, length_s, length_db: like.length_db, stopped: short, fundamental_hz, partials }
}
