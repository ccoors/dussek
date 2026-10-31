//! Rolled pianos: new pianos from the exported ones (design log steps 117–118).
//! A frame piano's export edited as JSON and `data.bin` arrays: the strings blended from other
//! pianos', a body from knobs measured on the references, a fine structure mixed from real
//! responses, checked against the references' range.
pub mod bands;
pub mod basis;
pub mod body;
pub mod calibrate;
pub mod files;
pub mod generate;
pub mod measure;
pub mod parts;
pub mod recipe;
