//! An export's two files as the roll edits them: `instrument.json` as JSON (every field kept, the
//! engine's or not) and `data.bin` as bytes, its arrays reached by JSON pointer ("/board/f").
use crate::Instrument;
use crate::instrument::Error;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

fn err(msg: impl Into<String>) -> Error {
    Error(msg.into())
}

#[derive(Clone)]
pub struct Export {
    pub json: Value,
    pub data: Vec<u8>,
}

/// One radiation response: its polarization (0 vertical, 1 horizontal), its microphone, its samples.
pub struct Response {
    pub pol: usize,
    pub channel: usize,
    pub ir: Vec<f64>,
}

const POLS: [&str; 2] = ["v", "h"];

impl Export {
    pub fn read(dir: &Path) -> Result<Export, Error> {
        let text = fs::read_to_string(dir.join("instrument.json")).map_err(|e| err(format!("{}: instrument.json: {e}", dir.display())))?;
        let json = serde_json::from_str(&text).map_err(|e| err(format!("{}: instrument.json: {e}", dir.display())))?;
        let data = fs::read(dir.join("data.bin")).map_err(|e| err(format!("{}: data.bin: {e}", dir.display())))?;
        Ok(Export { json, data })
    }

    /// Both files into `dir` (made if missing), each written under a temporary name and renamed,
    /// `instrument.json` last: a reader never sees half a file, nor a new `instrument.json` with
    /// the old `data.bin`.
    pub fn write(&self, dir: &Path) -> Result<(), Error> {
        let io = |e: std::io::Error| err(format!("{}: {e}", dir.display()));
        fs::create_dir_all(dir).map_err(io)?;
        let text = serde_json::to_string(&self.json).map_err(|e| err(format!("instrument.json: {e}")))?;
        // temporary names of this write alone: two writers never share (and truncate) one file
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let tag = format!("{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed));
        let (data_tmp, json_tmp) = (dir.join(format!("data.bin.{tag}.tmp")), dir.join(format!("instrument.json.{tag}.tmp")));
        fs::write(&data_tmp, &self.data).map_err(io)?;
        fs::write(&json_tmp, text).map_err(io)?;
        fs::rename(&data_tmp, dir.join("data.bin")).map_err(io)?;
        fs::rename(&json_tmp, dir.join("instrument.json")).map_err(io)?;
        Ok(())
    }

    /// The instrument these files make (written to a fresh temporary folder: the engine loads
    /// from folders).
    pub fn instrument(&self) -> Result<Instrument, Error> {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("pe-roll-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
        self.write(&d)?;
        let inst = Instrument::load(&d);
        let _ = fs::remove_dir_all(&d);
        inst
    }

    fn arr(&self, ptr: &str) -> Result<(usize, usize, String), Error> {
        let r = self.json.pointer(ptr).ok_or_else(|| err(format!("instrument.json: no {ptr}")))?;
        let num = |k: &str| r.get(k).and_then(Value::as_u64).map(|v| v as usize).ok_or_else(|| err(format!("{ptr}: no {k}")));
        let t = r.get("t").and_then(Value::as_str).ok_or_else(|| err(format!("{ptr}: no t")))?;
        Ok((num("o")?, num("n")?, t.to_string()))
    }

    fn bytes(&self, ptr: &str, t: &str, width: usize) -> Result<&[u8], Error> {
        let (o, n, tt) = self.arr(ptr)?;
        if tt != t {
            return Err(err(format!("{ptr}: expected {t}, got {tt}")));
        }
        self.data.get(o..o + n * width).ok_or_else(|| err(format!("{ptr}: beyond data.bin")))
    }

    pub fn f64s(&self, ptr: &str) -> Result<Vec<f64>, Error> {
        Ok(self.bytes(ptr, "f64", 8)?.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect())
    }

    pub fn f32s(&self, ptr: &str) -> Result<Vec<f32>, Error> {
        Ok(self.bytes(ptr, "f32", 4)?.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect())
    }

    /// The array at `ptr` replaced: in place when the length matches, else appended at an 8-byte
    /// boundary and the reference moved there.
    fn put(&mut self, ptr: &str, t: &str, bytes: Vec<u8>, n: usize) -> Result<(), Error> {
        let (o, old_n, tt) = self.arr(ptr)?;
        if tt != t {
            return Err(err(format!("{ptr}: expected {t}, got {tt}")));
        }
        if n == old_n {
            self.data[o..o + bytes.len()].copy_from_slice(&bytes);
            return Ok(());
        }
        self.data.resize(self.data.len().next_multiple_of(8), 0);
        let o = self.data.len();
        self.data.extend_from_slice(&bytes);
        *self.json.pointer_mut(ptr).expect("read above") = json!({ "o": o, "n": n, "t": t });
        Ok(())
    }

    pub fn set_f64s(&mut self, ptr: &str, v: &[f64]) -> Result<(), Error> {
        self.put(ptr, "f64", v.iter().flat_map(|x| x.to_le_bytes()).collect(), v.len())
    }

    pub fn set_f32s(&mut self, ptr: &str, v: &[f32]) -> Result<(), Error> {
        self.put(ptr, "f32", v.iter().flat_map(|x| x.to_le_bytes()).collect(), v.len())
    }

    /// The radiation responses: per polarization (v, h) each microphone's.
    pub fn responses(&self) -> Result<Vec<Response>, Error> {
        let mut out = Vec::new();
        for (pol, p) in POLS.iter().enumerate() {
            let body = format!("/radiation/{p}/body");
            let len = self.json.pointer(&format!("{body}/len")).and_then(Value::as_u64).ok_or_else(|| err(format!("{body}: no len")))? as usize;
            let all = self.f32s(&format!("{body}/ir"))?;
            for (channel, c) in all.chunks_exact(len.max(1)).enumerate() {
                out.push(Response { pol, channel, ir: c.iter().map(|&x| x as f64).collect() });
            }
        }
        Ok(out)
    }

    /// One response replaced (same length).
    pub fn set_response(&mut self, pol: usize, channel: usize, ir: &[f64]) -> Result<(), Error> {
        let ptr = format!("/radiation/{}/body/ir", POLS[pol]);
        let len = self.json.pointer(&format!("/radiation/{}/body/len", POLS[pol])).and_then(Value::as_u64).unwrap_or(0) as usize;
        if ir.len() != len {
            return Err(err(format!("{ptr}: a response of {} samples, not {len}", ir.len())));
        }
        let mut all = self.f32s(&ptr)?;
        all[channel * len..(channel + 1) * len].iter_mut().zip(ir).for_each(|(d, &s)| *d = s as f32);
        self.set_f32s(&ptr, &all)
    }
}
