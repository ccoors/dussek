//! `--cc`: MIDI controllers tied to controls, as the plugin's MIDI map ties them (`plugin/src/MidiMap.cpp`): a value
//! 0–127 is the control's range, a switch on above 0; air and the plunger moved by MIDI take over from the keys (their
//! "by hand" switch). `plugin/tests/cc_values.txt` holds what the plugin makes of each, `ui_tests` and the tests here
//! read it.

use crate::Params;

/// What a value 0–127 moves: a range in the plugin's units, or a switch.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Range(f32, f32),
    Switch,
}

/// The controls `--cc` can drive: the plugin's symbol and its range; each a plain map of the value in the plugin
/// (the lists, the Mute's and the Glide's, depend on the plugin's own choices and are left out).
const TARGETS: &[(&str, Kind)] = &[
    ("air", Kind::Range(0.0, 127.0)),
    ("growl", Kind::Range(0.0, 100.0)),
    ("flutter", Kind::Range(0.0, 100.0)),
    ("brass_vibrato", Kind::Range(0.0, 100.0)),
    ("plunger", Kind::Range(0.0, 100.0)),
    ("glide_hold", Kind::Switch),
    ("glide_time", Kind::Range(40.0, 1200.0)),
    ("virtual_player", Kind::Switch),
    ("brass_solo", Kind::Switch),
    ("air_added", Kind::Range(0.0, 100.0)),
];

/// The plugin's ties on a brass instrument, as a fresh MIDI map has them (`MidiMap::addBrassTies`).
pub const BRASS_DEFAULT: &str = "1:1=air_added,1:11=air,1:70=brass_vibrato,1:21=growl,1:23=flutter,1:4=plunger,1:65=glide_hold,1:5=glide_time";

/// One controller's tie: channel 0–15, the controller, the control (`TARGETS`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tie {
    pub channel: u8,
    pub cc: u8,
    target: usize,
}

impl Tie {
    pub fn symbol(&self) -> &'static str {
        TARGETS[self.target].0
    }
}

/// The symbols `--cc` takes, for the help.
pub fn symbols() -> String {
    TARGETS.iter().map(|t| t.0).collect::<Vec<_>>().join(", ")
}

/// `CH:CC=SYMBOL,…` (CH 1–16, as the MIDI map's file and the plugin's MIDI tab count them) or `none`.
pub fn parse(spec: &str) -> Result<Vec<Tie>, String> {
    if spec.trim() == "none" {
        return Ok(Vec::new());
    }
    let mut ties: Vec<Tie> = Vec::new();
    for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let bad = |why: &str| format!("`{item}`: {why}");
        let (at, symbol) = item.split_once('=').ok_or_else(|| bad("expected CH:CC=SYMBOL"))?;
        let (ch, cc) = at.split_once(':').ok_or_else(|| bad("expected CH:CC=SYMBOL"))?;
        let ch: u8 = ch.trim().parse().ok().filter(|c| (1..=16).contains(c)).ok_or_else(|| bad("the channel is 1 to 16"))?;
        let cc: u8 = cc.trim().parse().ok().filter(|c| *c <= 127).ok_or_else(|| bad("the controller is 0 to 127"))?;
        if matches!(cc, 64 | 66 | 67 | 120 | 123) {
            return Err(bad("the pedals' and the panics' controllers are the engine's own"));
        }
        let target = TARGETS.iter().position(|t| t.0 == symbol.trim()).ok_or_else(|| bad(&format!("not one of {}", symbols())))?;
        if ties.iter().any(|t| t.channel == ch - 1 && t.cc == cc) {
            return Err(bad("tied twice"));
        }
        ties.push(Tie { channel: ch - 1, cc, target });
    }
    Ok(ties)
}

/// The plugin's value for `value` 0–127 (`ccToValue`, in f32 as there).
fn plugin_value(kind: Kind, value: u8) -> f32 {
    let value = value.min(127);
    match kind {
        Kind::Switch => {
            if value > 0 {
                1.0
            } else {
                0.0
            }
        }
        Kind::Range(lo, hi) => lo + (value as f32 / 127.0).clamp(0.0, 1.0) * (hi - lo),
    }
}

/// The controller's value set on `p` as the plugin's engine gets it (`EngineValues.cpp`).
pub fn apply(p: &mut Params, tie: &Tie, value: u8) {
    let (symbol, kind) = TARGETS[tie.target];
    let v = plugin_value(kind, value);
    let b = &mut p.brass;
    match symbol {
        "air" => b.air = Some(v),
        "growl" => b.growl = v / 100.0,
        "flutter" => b.flutter = v / 100.0,
        "brass_vibrato" => b.vibrato = v / 100.0,
        "plunger" => b.plunger = Some(v / 100.0),
        "glide_hold" => b.glide_hold = v >= 0.5,
        "glide_time" => b.glide_time = v / 1000.0,
        "virtual_player" => b.virtual_player = v >= 0.5,
        "brass_solo" => b.solo = v >= 0.5,
        "air_added" => b.air_added = v / 100.0,
        _ => unreachable!("{symbol}"),
    }
}

/// The engine's value of `symbol` in `p` (the parity table's column).
pub fn engine_value(p: &Params, symbol: &str) -> Option<f32> {
    let b = &p.brass;
    Some(match symbol {
        "air" => b.air.unwrap_or(-1.0),
        "growl" => b.growl,
        "flutter" => b.flutter,
        "brass_vibrato" => b.vibrato,
        "plunger" => b.plunger.unwrap_or(-1.0),
        "glide_hold" => b.glide_hold as u8 as f32,
        "glide_time" => b.glide_time,
        "virtual_player" => b.virtual_player as u8 as f32,
        "brass_solo" => b.solo as u8 as f32,
        "air_added" => b.air_added,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ties_read_as_the_map_file_counts_them() {
        let t = parse("1:11=air, 16:21=growl").unwrap();
        assert_eq!((t[0].channel, t[0].cc, t[0].symbol()), (0, 11, "air"));
        assert_eq!((t[1].channel, t[1].cc, t[1].symbol()), (15, 21, "growl"));
        assert!(parse("none").unwrap().is_empty());
        assert_eq!(parse(BRASS_DEFAULT).unwrap().len(), 8);
        for bad in ["0:11=air", "17:11=air", "1:128=air", "1:11=hammer", "1:64=air", "1:11", "1:11=air,1:11=growl"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_value_is_the_controls_range() {
        let set = |spec: &str, v: u8| {
            let mut p = Params::default();
            apply(&mut p, &parse(spec).unwrap()[0], v);
            p
        };
        assert_eq!(set("1:11=air", 127).brass.air, Some(127.0));
        assert_eq!(set("1:21=growl", 0).brass.growl, 0.0);
        assert!((set("1:5=glide_time", 127).brass.glide_time - 1.2).abs() < 1e-6);
        assert!((set("1:5=glide_time", 0).brass.glide_time - 0.04).abs() < 1e-6);
        assert!(set("1:65=glide_hold", 1).brass.glide_hold && !set("1:65=glide_hold", 0).brass.glide_hold);
        assert_eq!(set("1:4=plunger", 127).brass.plunger, Some(1.0));
    }

    #[test]
    fn the_plugin_makes_the_same_of_every_value() {
        // the table `ui_tests` writes from the plugin's own mapping (PE_WRITE_CC_TABLE=1 rewrites it)
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../plugin/tests/cc_values.txt");
        let text = std::fs::read_to_string(path).unwrap();
        let mut rows = 0;
        for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let w: Vec<&str> = line.split_whitespace().collect();
            if w[0] == "default" {
                assert_eq!(w[1], BRASS_DEFAULT, "the plugin's default ties");
                continue;
            }
            let (symbol, value, want): (&str, u8, f32) = (w[0], w[1].parse().unwrap(), w[2].parse().unwrap());
            let mut p = Params::default();
            apply(&mut p, &parse(&format!("1:2={symbol}")).unwrap()[0], value);
            let got = engine_value(&p, symbol).unwrap();
            assert!((got - want).abs() <= 1e-6 * want.abs().max(1.0), "{symbol} {value}: {got}, the plugin {want}");
            rows += 1;
        }
        assert_eq!(rows, TARGETS.len() * 5, "every control at five values");
    }
}
