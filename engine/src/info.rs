//! What an instrument has and which controls do nothing on it, from its instrument.json alone
//! (the plugin's menu and its visible controls; engine/tests/no_effect.rs checks the rules
//! against renders, for every exported instrument). The rules read the data the engine
//! branches on, never an instrument's name.

use std::path::Path;

use crate::instrument::{Error, Facts};
use crate::mic::Body;

/// What plucks or strikes the strings (the plugin's word for the hardness control).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exciter {
    Hammer = 0,
    Quill = 1,
    Finger = 2,
    Pick = 3,
    Tangent = 4,
    Mallet = 5,
    Thumb = 6,
    Breath = 7,
}

/// The controls of `engine/tests/no_effect.rs` CONTROLS, in that order (their bit in `Info::inert`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Sympathetic = 0,
    PedalNoise,
    Knock,
    Release,
    HammerHardness,
    HammerGrooves,
    DamperBuzz,
    Rattle,
    WearHammers,
    WearVoicing,
    WearDampers,
    WearAction,
    Phantom,
    UnisonWidth,
    Stretch,
    WearUnisons,
    WearTuning,
    WearStrings,
    WearSoundboard,
    WearFalseBeats,
    AudioInput,
    Dynamics,
    Sustain,
    Variation,
    BoardBass,
    Restrike,
    PedalRing,
}

/// The controls and their symbols (the plugin's parameter symbols).
pub const CONTROLS: [(Control, &str); 27] = [
    (Control::Sympathetic, "sympathetic"),
    (Control::PedalNoise, "pedal_noise"),
    (Control::Knock, "knock"),
    (Control::Release, "release"),
    (Control::HammerHardness, "hammer_hardness"),
    (Control::HammerGrooves, "hammer_grooves"),
    (Control::DamperBuzz, "damper_buzz"),
    (Control::Rattle, "rattle"),
    (Control::WearHammers, "wear_hammers"),
    (Control::WearVoicing, "wear_voicing"),
    (Control::WearDampers, "wear_dampers"),
    (Control::WearAction, "wear_action"),
    (Control::Phantom, "phantom"),
    (Control::UnisonWidth, "unison_width"),
    (Control::Stretch, "stretch"),
    (Control::WearUnisons, "wear_unisons"),
    (Control::WearTuning, "wear_tuning"),
    (Control::WearStrings, "wear_strings"),
    (Control::WearSoundboard, "wear_soundboard"),
    (Control::WearFalseBeats, "wear_false_beats"),
    (Control::AudioInput, "audio_input"),
    (Control::Dynamics, "dynamics"),
    (Control::Sustain, "sustain"),
    (Control::Variation, "variation"),
    (Control::BoardBass, "board_bass"),
    (Control::Restrike, "restrike"),
    (Control::PedalRing, "pedal_ring"),
];

/// A control's label in the instrument's own words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Label {
    pub name: String,
    pub description: Option<String>,
}

/// The export's menu block (`[menu]` in the project file).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {
    pub name: String,
    pub group: String,
    pub hidden: bool,
    /// the keyboard's colors reversed: dark naturals, light sharps (the plugin's strip)
    pub reversed_keys: bool,
    /// a new kind of instrument, gaps allowed: the plugin marks it in the menu
    pub experimental: bool,
    /// the plugin's default choice among its instruments (a fresh instance, the Default preset), its place in the menu kept
    pub first: bool,
    /// the mallets' display names, in the file's order (empty: none given)
    pub mallets: Vec<String>,
    /// by parameter symbol, sorted
    pub labels: Vec<(String, Label)>,
}

/// What a brass instrument's window shows.
#[derive(Clone, Debug, PartialEq)]
pub struct BrassInfo {
    /// the lowest and highest key that plays (past them gray and silent)
    pub low: u8,
    pub high: u8,
    /// its mutes, "open" first, in the project's order
    pub mutes: Vec<String>,
    /// it has the plunger
    pub plunger: bool,
    /// it glides on a slide
    pub slide: bool,
    pub experimental: bool,
    /// where its bell points as played
    pub pose: crate::mic::BellPose,
}

/// An instrument's report.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    /// a brass instrument's own (its sound is its players')
    pub brass: Option<BrassInfo>,
    pub body: Body,
    /// which instruments morph together (equal and nonzero; 1 the hammer-strung pianos, 2 the
    /// harpsichords, 3 the harps)
    pub morph_family: u32,
    /// a hammer piano (`Instrument::is_hammer_piano`'s rule): its felt bends at the build
    pub hammer_piano: bool,
    pub exciter: Exciter,
    pub recorded: (u8, u8),
    /// 1 without format 6
    pub registers: usize,
    /// the stops that play a register, as bits of `Params::stops` (none with one register)
    pub stops: u32,
    pub buff: bool,
    pub lute: bool,
    pub motor: bool,
    pub bend: bool,
    pub bebung: bool,
    /// some note has a damper (the damper fall, the pedal's latch, the sustain and sostenuto pedals)
    pub dampers: bool,
    /// bars on a frame
    pub bars: bool,
    /// the body lays out a range of its own (the bars, a harp's or a zither's bridge, a guitar's
    /// strings): Spread all keys acts (at a microphone position)
    pub spread: bool,
    /// the body has a lid
    pub lid: bool,
    /// some note is struck (the engine's soft pedal ignores plucked notes)
    pub soft_pedal: bool,
    /// the file's mallet names
    pub mallets: Vec<String>,
    /// Prep indices 1 … 5 (Plucked, Muted, Harmonic, Bolt, Paper) that act; None is always there
    pub preparations: Vec<u8>,
    /// bit `Control as u32`: the control has no effect
    pub inert: u32,
    pub menu: Option<Menu>,
}

impl Info {
    pub fn is_inert(&self, c: Control) -> bool {
        self.inert >> c as u32 & 1 == 1
    }

    /// the inert controls' symbols, in CONTROLS order
    pub fn inert_symbols(&self) -> Vec<&'static str> {
        CONTROLS.iter().filter(|(c, _)| self.is_inert(*c)).map(|(_, s)| *s).collect()
    }
}

#[derive(serde::Deserialize)]
struct RawMenu {
    name: String,
    group: String,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    reversed_keys: bool,
    #[serde(default)]
    experimental: bool,
    #[serde(default)]
    first: bool,
    #[serde(default)]
    labels: std::collections::BTreeMap<String, RawLabel>,
    #[serde(default)]
    mallets: Vec<String>,
}

#[derive(serde::Deserialize)]
struct RawLabel {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// The report of the instrument in `dir` (reads instrument.json, not data.bin).
pub fn info(dir: impl AsRef<Path>) -> Result<Info, Error> {
    let f = crate::instrument::facts(dir.as_ref())?;
    let menu = match f.menu.clone() {
        None => None,
        Some(v) => {
            let m: RawMenu = serde_json::from_value(v).map_err(|e| Error(format!("instrument.json: menu: {e}")))?;
            Some(Menu {
                name: m.name,
                group: m.group,
                hidden: m.hidden,
                reversed_keys: m.reversed_keys,
                experimental: m.experimental,
                first: m.first,
                mallets: m.mallets,
                labels: m.labels.into_iter().map(|(s, l)| (s, Label { name: l.name, description: l.description })).collect(),
            })
        }
    };
    Ok(report(&f, menu))
}

fn report(f: &Facts, menu: Option<Menu>) -> Info {
    let any = |p: &dyn Fn(&crate::instrument::NoteFacts) -> bool| f.notes.iter().any(p);
    let all = |p: &dyn Fn(&crate::instrument::NoteFacts) -> bool| f.notes.iter().all(p);
    let hammer = any(&|n| n.hammer);
    let plucked = any(&|n| !n.hammer);
    let dampers = any(&|n| n.has_damper);
    let exciter = if any(&|n| n.tangent) {
        Exciter::Tangent
    } else if f.body == Body::Kalimba {
        Exciter::Thumb
    } else if f.body.is_bars() {
        Exciter::Mallet
    } else if hammer {
        Exciter::Hammer
    } else if f.body.is_harp() || matches!(f.body, Body::Guitar | Body::Strumstick) {
        Exciter::Finger
    } else if f.body.is_zither() {
        Exciter::Pick
    } else {
        Exciter::Quill
    };
    let exciter = if f.brass.is_some() { Exciter::Breath } else { exciter };
    let mut preparations = Vec::new();
    if hammer {
        preparations.push(1);
    }
    if dampers {
        preparations.push(2);
    }
    preparations.extend([3, 4]);
    if f.mallets.is_empty() {
        preparations.push(5);
    }
    let mut inert = 0u32;
    let mut set = |c: Control, on: bool| {
        if on {
            inert |= 1 << c as u32;
        }
    };
    set(Control::PedalNoise, !f.pedal);
    set(Control::PedalRing, !f.pedal_ring);
    set(Control::HammerGrooves, !hammer);
    set(Control::Release, all(&|n| !n.impacts));
    // the Action knock control sets the knock or a piano's impact on the board
    let knock = (f.knock && any(&|n| n.knock)) || f.impact;
    set(Control::Knock, !knock);
    set(Control::WearAction, !knock);
    set(Control::DamperBuzz, !dampers);
    set(Control::WearDampers, !dampers);
    // `LiveWear::of`: a hand's fingers, a pick or a thumb do not harden
    set(Control::WearHammers, crate::physics::LiveWear::of(f.body).hammers == 0.0);
    let single = all(&|n| n.strings == 1);
    set(Control::UnisonWidth, single);
    set(Control::WearUnisons, single);
    // Instrument::compile: plucked instruments are tuned without a stretch
    set(Control::Stretch, plucked);
    set(Control::Sympathetic, all(&|n| n.kappa == 0.0 && !n.halo) && !f.membrane_sympathetic);
    // the wear this kind of instrument can show (`Wear::on`: bars have no soundboard or strings,
    // metal ones hold their pitch)
    let full = crate::physics::Wear { unisons: 1.0, tuning: 1.0, strings: 1.0, soundboard: 1.0, false_beats: 1.0, seed: 0 }.on(f.body);
    set(Control::WearFalseBeats, all(&|n| n.split) || full.false_beats == 0.0);
    set(Control::WearSoundboard, full.soundboard == 0.0);
    set(Control::WearStrings, full.strings == 0.0);
    set(Control::WearTuning, full.tuning == 0.0);
    // the input drives the sympathetic bank only
    set(Control::AudioInput, all(&|n| n.kappa == 0.0));
    // the velocity maps through a level table on hammer pianos alone (`Instrument::is_hammer_piano`)
    // drawn at a strike (a hammer's or a mallet's contact); a pluck has none
    set(Control::Variation, !any(&|n| n.hammer));
    // a hammer's contact meets the ringing strings, a mallet the swinging bar; a pluck starts from rest
    set(Control::Restrike, !(any(&|n| n.hammer) || exciter == Exciter::Mallet));
    set(Control::Dynamics, !(f.level_table && all(&|n| n.hammer && !n.tangent) && f.mallets.is_empty() && !f.body.is_bars()));
    // the board's bass is a hammer piano's alone (`Instrument::is_hammer_piano`)
    let hammer_piano = !f.notes.is_empty() && all(&|n| n.hammer && !n.tangent) && f.mallets.is_empty() && !f.body.is_bars();
    set(Control::BoardBass, !hammer_piano);
    let body = match (&f.brass, f.body) { (Some(b), Body::Bell(_)) => Body::Bell(b.pose), (_, body) => body };
    // a bell's players: of all this only the Dynamics and the variation act, the strings are silent
    if f.brass.is_some() {
        preparations.clear();
        inert = u32::MAX & !(1 << Control::Dynamics as u32) & !(1 << Control::Variation as u32);
    }
    Info {
        brass: f.brass.clone().map(|b| BrassInfo { experimental: menu.as_ref().is_some_and(|m| m.experimental), ..b }),
        body,
        morph_family: if f.brass.is_some() { 0 } else { f.morph_family },
        hammer_piano,
        exciter,
        recorded: f.recorded,
        registers: f.registers,
        stops: f.stops,
        buff: f.buff,
        lute: f.lute,
        motor: f.motor,
        bend: f.bend,
        bebung: exciter == Exciter::Tangent,
        dampers,
        bars: body.is_bars(),
        spread: body.own_range().is_some(),
        lid: !(body.is_bars() || body.is_harp() || matches!(body, Body::Guitar | Body::Strumstick | Body::Bell(_)) || body.is_zither()),
        soft_pedal: hammer,
        mallets: f.mallets.clone(),
        preparations,
        inert,
        menu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_bass_is_a_hammer_piano_s() {
        let dir = |n: &str| format!("{}/../instruments/{n}", env!("CARGO_MANIFEST_DIR"));
        let Ok(piano) = info(dir("grand_a")) else { return };
        assert!(!piano.inert_symbols().contains(&"board_bass"), "a grand shows it");
        for other in ["harpsichord", "marimba"] {
            let Ok(i) = info(dir(other)) else { continue };
            assert!(i.inert_symbols().contains(&"board_bass"), "{other} hides it");
        }
    }
}
