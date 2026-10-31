//! The instrument editor's logic (no UI): the project files, the mapper (a port of
//! analysis/mapping.py), the measurements and the suggestion, the driver (proto/build.py).
pub mod project;
pub mod roll;
pub mod kind;
pub mod rule;
pub mod pyfmt;
pub mod audio;
pub mod compare;
pub mod driver;
pub mod mapping;
pub mod player;
pub mod sfz;
pub mod suggest;
pub mod takes;
pub mod view;
pub mod wizard;
