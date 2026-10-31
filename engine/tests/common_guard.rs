//! The shared loader's guard: a committed instrument that is missing (a renamed or mistyped
//! directory) fails the test instead of letting it return early, as a missing dataset does.
mod common;

#[test]
#[should_panic(expected = "renamed?")]
fn a_missing_committed_instrument_panics() {
    common::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/no_such"));
}

#[test]
fn a_directory_outside_the_instruments_is_just_absent() {
    assert!(common::load("/nonexistent/no_such").is_none());
}
