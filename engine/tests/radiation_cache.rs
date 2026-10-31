//! The radiation cache keeps the last few microphone setups: switching between the plugin's
//! positions and back computes each one's responses once. Needs the exported instruments.

mod common;
use pianoengine::mic::MicPosition;
use pianoengine::{Engine, RadiationCache};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

#[test]
fn switching_back_to_a_position_reuses_its_responses() {
    let Some(inst) = common::load(DIR) else { return };
    let mut cache = RadiationCache::default();
    let at = |cache: &mut RadiationCache, pos: u32| {
        let mut t = inst.tuning;
        t.mic = MicPosition::from_index(pos);
        Engine::with_cache(inst.clone(), 48000.0, t, cache).expect("engine");
    };
    for pos in [1, 2, 3, 4, 0] {
        at(&mut cache, pos);
    }
    let built = cache.builds();
    assert_eq!(built, 5);
    for pos in [1, 2, 3, 4, 0, 2] { // back and forth: nothing new
        at(&mut cache, pos);
    }
    assert_eq!(cache.builds(), built);
    // another tuning of the strings (concert pitch) keeps the responses too
    let mut t = inst.tuning;
    t.mic = MicPosition::Player;
    t.pitch_ratio *= 1.01;
    Engine::with_cache(inst.clone(), 48000.0, t, &mut cache).expect("engine");
    assert_eq!(cache.builds(), built);
}
