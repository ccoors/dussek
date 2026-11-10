//! Dussek's real-time engine: runs an instrument exported by `proto/export.py` (Phase 3).
//!
//! Physics and parameters come from the Python prototype; this crate only executes
//! them: hammer contact, coupled string modes, dampers and pedal, sympathetic
//! resonance, soundboard/room radiation, action knock and release noise.

pub mod board_bass;
pub mod brass;
pub mod convolution;
pub mod engine;
pub mod felt;
pub mod ffi;
pub mod info;
pub mod instrument;
pub mod kernel;
pub mod mic;
pub mod midi;
pub mod midi2wav;
pub mod motor;
pub mod noise;
pub mod physics;
pub mod pillow;
pub mod resample;
pub mod resonance;
pub mod roll;
pub mod spec;
pub mod stretch;
pub mod telemetry;
pub mod temperament;
pub mod threads;
pub mod velocity;
pub mod voice;

pub use engine::{Engine, Event, Mix, Params, Prep, RadiationCache, SILENCE_S};
pub use physics::{Tuning, Wear};
pub use instrument::Instrument;
