//! The brass family: `proto/brass/rt.c` and `live.py`, `auto_player.py` in Rust, at the fits' own 44.1 kHz.

pub mod bell;
pub mod core;
pub mod family;
pub mod player;
pub mod rate;
pub mod section;
pub mod shape;
pub mod streams;
pub mod tables;

pub use family::{BrassParams, Family};

/// `Params::variation` on brass: each note differing from the last as the virtual player's were fitted.
pub const VARIATION_AS_FITTED: f32 = 1.0;

pub use tables::Tables;
