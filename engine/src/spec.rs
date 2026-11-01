//! `key=value,…` option strings for the command-line tools.

use crate::{Params, Tuning};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn items(spec: &str) -> impl Iterator<Item = Result<(&str, f64)>> {
    spec.split(',').filter(|kv| !kv.is_empty()).map(|kv| {
        let (k, v) = kv.split_once('=').ok_or(format!("`{kv}`: expected key=value"))?;
        let x = v.trim().parse::<f64>().map_err(|_| format!("`{kv}`: `{v}` is not a number"))?;
        if !x.is_finite() {
            return Err(format!("`{kv}`: not a finite number").into());
        }
        Ok((k.trim(), x))
    })
}

/// The `--param` keys and what each sets (`midi2wav --help`; `params` takes every one).
pub const PARAM_KEYS: &[(&str, &str)] = &[
    ("volume", "the output's gain (linear, 1 as calibrated)"),
    ("room", "the room's level (× as recorded)"),
    ("knock", "the hammer's knock on the board (× as fitted)"),
    ("release", "the release noise (× as fitted)"),
    ("pedal", "the sustain pedal's thumps (× as fitted)"),
    ("pedal_ring", "the strings' ring as the sustain pedal lifts their dampers (× as fitted)"),
    ("sympathetic", "the strings ringing along (× the fitted coupling)"),
    ("hardness", "the hammer felt's stiffness (× as fitted)"),
    ("dynamics", "a hammer piano's level range over the velocity (dB, 10–70)"),
    ("variation", "each strike slightly different (0 none … 1 as measured)"),
    ("restrike", "a re-strike meets the strings or the bar where they ring (0 off, 1 on)"),
    ("hammer", "wear: the hammers' felt compressed (0 as fitted … 1)"),
    ("voicing", "wear: the hammers worn unevenly (0 … 1)"),
    ("damper", "wear: the damper felt (0 … 1)"),
    ("action", "wear: the action's noise (0 … 1)"),
    ("phantom", "phantom partials of old, stiff strings (0 … 1, in the bass)"),
    ("grooves", "hammer grooves: a rough attack (0 off … 1)"),
    ("buzz", "damper buzz when a damper lands (0 off … 1)"),
    ("rattle", "a loose part on the board buzzing on loud passages (0 off … 1)"),
    ("damper_fall", "the dampers' fall time (× the normal, 0 instant)"),
    ("prep", "preparation: 0 none, 1 plucked, 2 muted, 3 harmonic, 4 bolt, 5 paper"),
    ("pluck_pos", "plucked: the point along the string (fraction of its length)"),
    ("pluck_bright", "plucked: the brightness (× a fingertip's)"),
    ("mute", "muted: how hard the hand rests on the strings (0 … 1)"),
    ("harmonic", "harmonic: the node a finger touches (2 the octave, 3 the twelfth, …)"),
    ("paper", "paper: how much it buzzes (0 … 1)"),
    ("stops", "the registers a note plays, as bits (1 the 8′, 2 the 4′, 4 the 8′ II)"),
    ("buff", "the harpsichord's buff stop (0 off, 1 on)"),
    ("lute", "the lute stop (0 off, 1 on)"),
    ("motor", "the vibraphone's motor (0 off, 1 on)"),
    ("motor_rate", "the motor's pulses per second"),
    ("motor_depth", "the motor's depth (0 … 1)"),
    ("mallet", "the mallet's index (the instrument's own mallets)"),
    ("bebung", "the clavichord's Bebung: the pitch rise at full pressure (cents)"),
    ("bend", "the pitch wheel's range (semitones, 0 … 12)"),
    ("input", "the audio input's level (linear, 0 off … 16; --input names its file)"),
];

/// The `--tuning` keys and what each sets (`midi2wav --help`; `tuning` takes every one, and the
/// free microphones' `mic_x1` … `mic_pitch2`).
pub const TUNING_KEYS: &[(&str, &str)] = &[
    ("a4", "the concert pitch (Hz)"),
    ("temperament", "the temperament's index (below; 0 equal)"),
    ("key", "the temperament's key (0 C … 11 B)"),
    ("stretch", "a tuner's stretch: 1 adds it, −1 takes it off, 0 as recorded"),
    ("width", "the unisons' detuning (× as fitted, 0 perfectly in tune)"),
    ("sustain", "how long the strings ring (× as fitted)"),
    ("bass", "the board's bass on a hammer piano's strings (dB)"),
    ("unisons", "wear: the unisons drifted apart (0 … 1)"),
    ("tuning", "wear: the notes drifted (0 … 1)"),
    ("strings", "wear: old strings (0 … 1)"),
    ("board", "wear: the soundboard and bridge (0 … 1)"),
    ("falsebeats", "wear: false beats (0 … 1)"),
    ("seed", "the wear's random draw"),
    ("mic", "microphones: 0 recording, 1 player, 2 audience, 3 hammers, 4 tail, 5 free"),
    ("spacing", "the microphone pair's spacing (m)"),
    ("distance", "the microphones' distance (m; 0 the position's own)"),
    ("direct", "the level kept by the direct sound (1) or by the total (0)"),
    ("lid", "how far the lid is open (0 closed … 1 open, 2 taken off)"),
    ("pillow", "a pillow against the soundboard (0 none … 1)"),
    ("spread", "bars: all 88 keys spread along the frame (1) or its own range (0)"),
    ("bolt", "the prepared piano's bolt: its mass (0 none … 1)"),
    ("bolt_pos", "the bolt's place along the string (fraction)"),
    ("mic_x1", "mic=5: the left microphone at mic_x1, mic_y1, mic_z1 (m)"),
    ("mic_pattern1", "its pattern (0 omni … 1 figure-8); mic_yaw1, mic_pitch1 (degrees)"),
    ("mic_x2", "the right microphone: the same keys ending in 2"),
    ("mic_wall", "the wall behind them (m)"),
];

/// every key `tuning` takes for the free microphones
fn mic_keys() -> impl Iterator<Item = String> {
    ["x", "y", "z", "pattern", "yaw", "pitch"].into_iter().flat_map(|f| [format!("mic_{f}1"), format!("mic_{f}2")]).chain(std::iter::once("mic_wall".to_string()))
}

/// The nearest of `known` to `k` within a few edits: the "did you mean" of an unknown key.
pub fn nearest<'a>(k: &str, known: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let distance = |a: &str, b: &str| {
        let b: Vec<char> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, &cb) in b.iter().enumerate() {
                let next = (prev + (ca != cb) as usize).min(row[j] + 1).min(row[j + 1] + 1);
                prev = row[j + 1];
                row[j + 1] = next;
            }
        }
        row[b.len()]
    };
    let limit = (k.chars().count() / 3).clamp(1, 3);
    known.map(|c| (distance(k, c), c)).filter(|&(d, _)| d <= limit).min_by_key(|&(d, _)| d).map(|(_, c)| c)
}

fn unknown(what: &str, k: &str, known: Vec<&str>) -> Box<dyn std::error::Error> {
    let hint = nearest(k, known.into_iter()).map_or(String::new(), |n| format!(" (did you mean `{n}`?)"));
    format!("unknown {what} `{k}`{hint}; `midi2wav --help` lists them").into()
}

/// `mic_x1 … mic_pitch2` and `mic_wall`: the free microphones (with `mic=5`).
fn mic_field(t: &mut Tuning, k: &str, x: f64) -> bool {
    if k == "mic_wall" {
        t.mic_setup.wall = x;
        return true;
    }
    let Some(rest) = k.strip_prefix("mic_") else { return false };
    let (field, i) = match (rest.strip_suffix('1'), rest.strip_suffix('2')) {
        (Some(f), _) => (f, 0),
        (_, Some(f)) => (f, 1),
        _ => return false,
    };
    let m = &mut t.mic_setup.mics[i];
    match field {
        "x" => m.pos[0] = x,
        "y" => m.pos[1] = x,
        "z" => m.pos[2] = x,
        "pattern" => m.pattern = x,
        "yaw" => m.yaw = x,
        "pitch" => m.pitch = x,
        _ => return false,
    }
    true
}

/// A `--mic` argument: `X,Y,Z[,PATTERN[,YAW[,PITCH]]]` (omni and aimed at the board unless given).
pub fn mic(arg: &str) -> Result<crate::mic::Mic> {
    let v: Vec<f64> = arg.split(',').map(|s| s.trim().parse::<f64>().map_err(|_| format!("`{s}` is not a number; expected X,Y,Z[,PATTERN[,YAW[,PITCH]]] in meters and degrees"))).collect::<std::result::Result<_, _>>()?;
    if !(3..=6).contains(&v.len()) {
        return Err(format!("{} numbers; expected X,Y,Z[,PATTERN[,YAW[,PITCH]]] in meters and degrees", v.len()).into());
    }
    let mut m = crate::mic::Mic::toward_board([v[0], v[1], v[2]]);
    if let Some(&p) = v.get(3) {
        m.pattern = p;
    }
    if let Some(&y) = v.get(4) {
        m.yaw = y;
    }
    if let Some(&p) = v.get(5) {
        m.pitch = p;
    }
    Ok(m)
}

/// Tuning keys: a4, stretch, bolt (mass 0…1), bolt_pos, width, sustain (the strings' ring re fitted), unisons, tuning, strings, board, falsebeats, seed, mic,
/// bass (the board's, dB),
/// spacing, distance, direct, lid (0 closed … 1 open, 2 taken off), pillow (0 none … 1), spread (bars: 1 all keys along the frame),
/// temperament (`temperament::NAMES` index, 0 equal) and key (its key, 0 C … 11 B), mic_x1 … mic_pitch2, mic_wall.
/// `mic` is 0..5, 5 = free microphones.
pub fn tuning(mut t: Tuning, spec: &str) -> Result<Tuning> {
    let mut temperament: Option<(usize, usize)> = None;
    for kv in items(spec) {
        let (k, x) = kv?;
        if mic_field(&mut t, k, x) {
            continue;
        }
        match k {
            "temperament" | "key" => {
                let n = if k == "temperament" { crate::temperament::NAMES.len() } else { 12 };
                if !(x >= 0.0 && x < n as f64 && x.fract() == 0.0) {
                    return Err(format!("`{k}={x}`: a whole number from 0 to {}", n - 1).into());
                }
                let (i, key) = temperament.get_or_insert((0, 0));
                *(if k == "temperament" { i } else { key }) = x as usize;
            }
            "a4" => t.pitch_ratio = x / 440.0,
            "stretch" => t.stretch = x,
            "bolt" => t.bolt = x,
            "bolt_pos" => t.bolt_pos = x,
            "width" => t.unison_width = x,
            "sustain" => t.sustain = x,
            "bass" => t.board_bass_db = x,
            "unisons" => t.wear.unisons = x,
            "tuning" => t.wear.tuning = x,
            "strings" => t.wear.strings = x,
            "board" => t.wear.soundboard = x,
            "falsebeats" => t.wear.false_beats = x,
            "seed" => t.wear.seed = x as u64,
            "mic" => t.mic = crate::mic::MicPosition::from_index(x as u32),
            "spacing" => t.mic_spacing = x,
            "distance" => t.mic_distance = x,
            "direct" => t.keep_direct = x != 0.0,
            "lid" => t.lid = x,
            "pillow" => t.pillow = x,
            "spread" => t.spread_keys = x != 0.0,
            _ => {
                let mics: Vec<String> = mic_keys().collect();
                return Err(unknown("tuning key", k, TUNING_KEYS.iter().map(|(k, _)| *k).chain(mics.iter().map(String::as_str)).collect()));
            }
        }
    }
    if let Some((i, key)) = temperament {
        t.temperament = crate::temperament::offsets(i, key);
        t.as_recorded = i == crate::temperament::AS_RECORDED;
    }
    Ok(t)
}

/// Live parameter keys: room, knock, release, pedal, sympathetic, hardness, hammer,
/// voicing, damper, action, phantom, grooves, buzz, rattle, volume, damper_fall, stops, buff, lute,
/// motor, motor_rate, motor_depth, mallet, bebung, bend (the pitch wheel's range), dynamics (dB), and
/// the prepared piano's prep (0 none, 1 plucked, 2 muted, 3 harmonic, 4 bolt, 5 paper),
/// pluck_pos, pluck_bright, mute, harmonic, paper.
pub fn params(mut p: Params, spec: &str) -> Result<Params> {
    for kv in items(spec) {
        let (k, x) = kv?;
        let f = x as f32;
        match k {
            "room" => p.room = f,
            "knock" => p.knock = f,
            "release" => p.release = f,
            "pedal" => p.pedal = f,
            "pedal_ring" => p.pedal_ring = f,
            "sympathetic" => p.sympathetic = f,
            "hardness" => p.hardness = f,
            "variation" => p.variation = f,
            "restrike" => p.restrike = f,
            "hammer" => p.hammer_wear = f,
            "voicing" => p.voicing_wear = f,
            "damper" => p.damper_wear = f,
            "action" => p.action_wear = f,
            "phantom" => p.phantom = f,
            "grooves" => p.grooves = f,
            "buzz" => p.damper_buzz = f,
            "rattle" => p.rattle = f,
            "volume" => p.volume = f,
            "damper_fall" => p.damper_fall = f,
            "prep" => p.prep = crate::engine::Prep::from_index(x as u32),
            "pluck_pos" => p.pluck_pos = f,
            "pluck_bright" => p.pluck_bright = f,
            "mute" => p.mute = f,
            "harmonic" => p.harmonic = x as u32,
            "paper" => p.paper = f,
            "stops" => p.stops = x as u32,
            "buff" => p.buff = x != 0.0,
            "lute" => p.lute = x != 0.0,
            "motor" => p.motor = x != 0.0,
            "motor_rate" => p.motor_rate = f,
            "motor_depth" => p.motor_depth = f,
            "mallet" => p.mallet = x as u32,
            "bebung" => p.bebung = f,
            "bend" => p.bend_range = f,
            "input" => p.input = f,
            "dynamics" => p.dynamics = f,
            _ => return Err(unknown("parameter", k, PARAM_KEYS.iter().map(|(k, _)| *k).collect())),
        }
    }
    Ok(p)
}

/// The options that give `midi2wav` these settings (its `--velocity-curve`, `--param`, `--tuning`), each
/// key only where it differs from the default the tool starts from. `a4` (Hz) and the temperament
/// (index, key) as given: the tuning keeps neither (the pitch ratio, the offsets). The numbers in
/// their shortest exact form: `params` and `tuning` read the same values back. The audio input's
/// level is left out (a MIDI render has no input).
pub fn format(p: &Params, curve: &crate::velocity::VelocityCurve, t: &Tuning, a4: f64, temperament: (usize, usize)) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    if !curve.is_straight() {
        write!(out, "--velocity-curve {}", curve.format()).unwrap();
    }
    let mut list = String::new();
    let mut item = |k: &str, v: String| {
        let sep = if list.is_empty() { "" } else { "," };
        write!(list, "{sep}{k}={v}").unwrap();
    };
    let d = Params::default();
    for (k, v, dv) in [
        ("room", p.room, d.room),
        ("knock", p.knock, d.knock),
        ("release", p.release, d.release),
        ("pedal", p.pedal, d.pedal),
        ("pedal_ring", p.pedal_ring, d.pedal_ring),
        ("sympathetic", p.sympathetic, d.sympathetic),
        ("hardness", p.hardness, d.hardness),
        ("hammer", p.hammer_wear, d.hammer_wear),
        ("voicing", p.voicing_wear, d.voicing_wear),
        ("damper", p.damper_wear, d.damper_wear),
        ("action", p.action_wear, d.action_wear),
        ("phantom", p.phantom, d.phantom),
        ("grooves", p.grooves, d.grooves),
        ("buzz", p.damper_buzz, d.damper_buzz),
        ("rattle", p.rattle, d.rattle),
        ("volume", p.volume, d.volume),
        ("damper_fall", p.damper_fall, d.damper_fall),
        ("pluck_pos", p.pluck_pos, d.pluck_pos),
        ("pluck_bright", p.pluck_bright, d.pluck_bright),
        ("mute", p.mute, d.mute),
        ("paper", p.paper, d.paper),
        ("motor_rate", p.motor_rate, d.motor_rate),
        ("motor_depth", p.motor_depth, d.motor_depth),
        ("bebung", p.bebung, d.bebung),
        ("bend", p.bend_range, d.bend_range),
        ("input", p.input, d.input),
        ("dynamics", p.dynamics, d.dynamics),
        ("variation", p.variation, d.variation),
        ("restrike", p.restrike, d.restrike),
    ] {
        if v != dv {
            item(k, v.to_string());
        }
    }
    for (k, v, dv) in [
        ("prep", p.prep as u32, d.prep as u32),
        ("harmonic", p.harmonic, d.harmonic),
        ("stops", p.stops, d.stops),
        ("buff", p.buff as u32, d.buff as u32),
        ("lute", p.lute as u32, d.lute as u32),
        ("motor", p.motor as u32, d.motor as u32),
        ("mallet", p.mallet, d.mallet),
    ] {
        if v != dv {
            item(k, v.to_string());
        }
    }
    let params = std::mem::take(&mut list);
    let d = Tuning::default();
    let mut item = |k: &str, v: String| {
        let sep = if list.is_empty() { "" } else { "," };
        write!(list, "{sep}{k}={v}").unwrap();
    };
    if a4 != 440.0 {
        item("a4", a4.to_string());
    }
    if temperament.0 != 0 {
        item("temperament", temperament.0.to_string());
        if temperament.1 != 0 {
            item("key", temperament.1.to_string());
        }
    }
    let mic = t.mic as u32;
    for (k, v, dv) in [
        ("stretch", t.stretch, d.stretch),
        ("bolt", t.bolt, d.bolt),
        ("bolt_pos", t.bolt_pos, d.bolt_pos),
        ("width", t.unison_width, d.unison_width),
        ("sustain", t.sustain, d.sustain),
        ("bass", t.board_bass_db, d.board_bass_db),
        ("unisons", t.wear.unisons, d.wear.unisons),
        ("tuning", t.wear.tuning, d.wear.tuning),
        ("strings", t.wear.strings, d.wear.strings),
        ("board", t.wear.soundboard, d.wear.soundboard),
        ("falsebeats", t.wear.false_beats, d.wear.false_beats),
        ("seed", t.wear.seed as f64, d.wear.seed as f64),
        ("mic", mic as f64, d.mic as u32 as f64),
        ("spacing", t.mic_spacing, d.mic_spacing),
        ("distance", t.mic_distance, d.mic_distance),
        ("direct", t.keep_direct as u32 as f64, d.keep_direct as u32 as f64),
        ("lid", t.lid, d.lid),
        ("pillow", t.pillow, d.pillow),
        ("spread", t.spread_keys as u32 as f64, d.spread_keys as u32 as f64),
    ] {
        if v != dv {
            item(k, v.to_string());
        }
    }
    // the free pair only matters at its position
    if t.mic == crate::mic::MicPosition::Custom {
        let (s, ds) = (&t.mic_setup, &d.mic_setup);
        for i in 0..2 {
            let (m, dm) = (&s.mics[i], &ds.mics[i]);
            let n = i + 1;
            for (f, v, dv) in [
                ("x", m.pos[0], dm.pos[0]),
                ("y", m.pos[1], dm.pos[1]),
                ("z", m.pos[2], dm.pos[2]),
                ("pattern", m.pattern, dm.pattern),
                ("yaw", m.yaw, dm.yaw),
                ("pitch", m.pitch, dm.pitch),
            ] {
                if v != dv {
                    item(&format!("mic_{f}{n}"), v.to_string());
                }
            }
        }
        if s.wall != ds.wall {
            item("mic_wall", s.wall.to_string());
        }
    }
    for (flag, list) in [("--param", params), ("--tuning", list)] {
        if !list.is_empty() {
            let sep = if out.is_empty() { "" } else { " " };
            write!(out, "{sep}{flag} {list}").unwrap();
        }
    }
    out
}

/// The instruments of a morph (`Instrument::morph`) from `--morph DIR=W` items: each `DIR=W`
/// (split at the last `=`), and `dir` first with the rest of the weight (1 − ΣW).
pub fn morph_sides(dir: &str, items: &[String]) -> Result<Vec<(String, f64)>> {
    let mut sides = vec![(dir.to_string(), 1.0)];
    for item in items {
        let (d, w) = item.rsplit_once('=').ok_or(format!("--morph {item}: expected DIR=W"))?;
        let w: f64 = w.parse().map_err(|e| format!("--morph {item}: {e}"))?;
        if !(w.is_finite() && w >= 0.0) {
            return Err(format!("--morph {item}: the weight must be 0 or more").into());
        }
        sides[0].1 -= w;
        sides.push((d.to_string(), w));
    }
    if sides[0].1 < -1e-9 {
        return Err("--morph: the weights add up to more than 1".into());
    }
    sides[0].1 = sides[0].1.max(0.0);
    Ok(sides)
}

#[cfg(test)]
mod format_tests {
    /// a random curve of 2–8 points from one number in [0, 1)
    fn random_curve(x: f64) -> crate::velocity::VelocityCurve {
        let mut seed = (x * 4_294_967_296.0) as u64 | 1;
        let mut next = |m: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % m
        };
        let n = 2 + next(7) as usize;
        let mut ins: Vec<u8> = (2..127u8).collect();
        let mut pts = vec![(1u8, 1 + next(40) as u8)];
        let mut mids: Vec<u8> = (0..n - 2).map(|_| ins.remove(next(ins.len() as u64) as usize)).collect();
        mids.sort();
        let mut out = pts[0].1;
        for i in mids {
            out = (out as u64 + next(30)).min(127) as u8;
            pts.push((i, out));
        }
        pts.push((127, (out as u64 + next(30)).min(127) as u8));
        crate::velocity::VelocityCurve::from_points(&pts).unwrap()
    }

    use super::*;
    use crate::mic::MicPosition;

    #[test]
    fn every_listed_key_is_taken_and_a_typo_gets_a_suggestion() {
        for (k, _) in PARAM_KEYS {
            params(Params::default(), &format!("{k}=1")).unwrap_or_else(|e| panic!("--param {k}: {e}"));
        }
        for k in TUNING_KEYS.iter().map(|(k, _)| k.to_string()).chain(mic_keys()) {
            tuning(Tuning::default(), &format!("{k}=1")).unwrap_or_else(|e| panic!("--tuning {k}: {e}"));
        }
        let e = params(Params::default(), "roomm=1").unwrap_err().to_string();
        assert!(e.contains("`roomm`") && e.contains("did you mean `room`?"), "{e}");
        let e = tuning(Tuning::default(), "mic_patern1=1").unwrap_err().to_string();
        assert!(e.contains("did you mean `mic_pattern1`?"), "{e}");
        let e = params(Params::default(), "zzzzzz=1").unwrap_err().to_string();
        assert!(!e.contains("did you mean"), "{e}");
        let e = params(Params::default(), "room=abc").unwrap_err().to_string();
        assert!(e.contains("`abc` is not a number"), "{e}");
        assert!(params(Params::default(), "room").unwrap_err().to_string().contains("key=value"));
        assert!(params(Params::default(), "room=inf").is_err());
    }

    /// A small random stream (the tests need no crate for it).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        /// the default half the time (the key then left out), else something in lo … hi
        fn or<T: Copy>(&mut self, d: T, f: impl FnOnce(f64) -> T) -> T {
            if self.next() < 0.5 { d } else { f(self.next()) }
        }
    }

    #[test]
    fn the_defaults_give_no_options() {
        assert_eq!(format(&Params::default(), &crate::velocity::VelocityCurve::STRAIGHT, &Tuning::default(), 440.0, (0, 0)), "");
    }

    #[test]
    fn the_keys_read_back_the_same_values() {
        let mut r = Lcg(7);
        for _ in 0..2000 {
            let d = Params::default();
            let p = Params {
                room: r.or(d.room, |x| (x * 4.0) as f32),
                knock: r.or(d.knock, |x| (x * 16.0) as f32),
                release: r.or(d.release, |x| x as f32 * 3.7),
                pedal: r.or(d.pedal, |x| x as f32),
                pedal_ring: r.or(d.pedal_ring, |x| x as f32 * 2.0),
                sympathetic: r.or(d.sympathetic, |x| (x * 1.3) as f32),
                hardness: r.or(d.hardness, |x| 10f32.powf((x * 2.0 - 1.0) as f32)),
                hammer_wear: r.or(d.hammer_wear, |x| x as f32),
                voicing_wear: r.or(d.voicing_wear, |x| x as f32),
                damper_wear: r.or(d.damper_wear, |x| x as f32),
                action_wear: r.or(d.action_wear, |x| x as f32),
                phantom: r.or(d.phantom, |x| x as f32),
                grooves: r.or(d.grooves, |x| x as f32),
                damper_buzz: r.or(d.damper_buzz, |x| x as f32),
                rattle: r.or(d.rattle, |x| x as f32),
                volume: r.or(d.volume, |x| 10f32.powf((x * 40.0 - 30.0) as f32 / 20.0)),
                damper_fall: r.or(d.damper_fall, |x| (x * 8.0) as f32),
                prep: r.or(d.prep, |x| crate::engine::Prep::from_index((x * 6.0) as u32)),
                pluck_pos: r.or(d.pluck_pos, |x| (0.02 + x * 0.48) as f32),
                pluck_bright: r.or(d.pluck_bright, |x| (0.1 + x * 9.9) as f32),
                mute: r.or(d.mute, |x| x as f32),
                harmonic: r.or(d.harmonic, |x| 2 + (x * 11.0) as u32),
                paper: r.or(d.paper, |x| x as f32),
                stops: r.or(d.stops, |x| (x * 8.0) as u32),
                buff: r.or(d.buff, |x| x < 0.5),
                lute: r.or(d.lute, |x| x < 0.5),
                motor: r.or(d.motor, |x| x < 0.5),
                motor_rate: r.or(d.motor_rate, |x| (0.1 + x * 19.9) as f32),
                motor_depth: r.or(d.motor_depth, |x| x as f32),
                mallet: r.or(d.mallet, |x| (x * 3.0) as u32),
                bebung: r.or(d.bebung, |x| (x * 200.0) as f32),
                bend_range: r.or(d.bend_range, |x| (x * 12.0) as f32),
                dynamics: r.or(d.dynamics, |x| (10.0 + x * 60.0) as f32),
                variation: r.or(d.variation, |x| (x * 3.0) as f32),
                restrike: r.or(d.restrike, |x| (x < 0.5) as u32 as f32),
                // no key: a MIDI render has no audio input
                input: d.input,
            };
            let dt = Tuning::default();
            let mic = r.or(dt.mic, |x| MicPosition::from_index((x * 6.0) as u32));
            let mut setup = dt.mic_setup;
            if mic == MicPosition::Custom {
                for m in &mut setup.mics {
                    m.pos = [r.or(m.pos[0], |x| x * 4.0 - 2.0), r.or(m.pos[1], |x| -x * 3.0), r.or(m.pos[2], |x| x * 2.0)];
                    m.pattern = r.or(m.pattern, |x| x);
                    m.yaw = r.or(m.yaw, |x| x * 720.0 - 360.0);
                    m.pitch = r.or(m.pitch, |x| x * 180.0 - 90.0);
                }
                setup.wall = r.or(setup.wall, |x| x * 3.0);
            }
            let t = Tuning {
                unison_width: r.or(dt.unison_width, |x| x * 5.0),
                sustain: r.or(dt.sustain, |x| 0.5 + x * 3.5),
                board_bass_db: r.or(dt.board_bass_db, |x| x * 36.0 - 12.0),
                stretch: r.or(dt.stretch, |x| x * 11.0 - 1.0),
                bolt: r.or(dt.bolt, |x| x),
                bolt_pos: r.or(dt.bolt_pos, |x| 0.02 + x * 0.48),
                wear: crate::Wear {
                    unisons: r.or(dt.wear.unisons, |x| x),
                    tuning: r.or(dt.wear.tuning, |x| x),
                    strings: r.or(dt.wear.strings, |x| x),
                    soundboard: r.or(dt.wear.soundboard, |x| x),
                    false_beats: r.or(dt.wear.false_beats, |x| x),
                    seed: r.or(dt.wear.seed, |x| (x * 1e6) as u64),
                },
                mic,
                mic_spacing: r.or(dt.mic_spacing, |x| x * 10.0),
                mic_distance: r.or(dt.mic_distance, |x| x * 20.0),
                keep_direct: r.or(dt.keep_direct, |x| x < 0.5),
                mic_setup: setup,
                lid: r.or(dt.lid, |x| x),
                pillow: r.or(dt.pillow, |x| x),
                spread_keys: r.or(dt.spread_keys, |x| x < 0.5),
                ..dt
            };
            let a4 = r.or(440.0, |x| 380.0 + x * 120.0);
            let temperament = r.or((0, 0), |x| ((x * 8.0) as usize, (x * 96.0) as usize % 12));
            let curve = r.or(crate::velocity::VelocityCurve::STRAIGHT, random_curve);
            let line = format(&p, &curve, &t, a4, temperament);
            // the options as midi2wav reads them
            let mut words = line.split(' ');
            let (mut got_p, mut got_t, mut got_curve) = (Params::default(), Tuning::default(), crate::velocity::VelocityCurve::STRAIGHT);
            while let Some(flag) = words.next() {
                let v = words.next().unwrap();
                match flag {
                    "--velocity-curve" => got_curve = crate::velocity::VelocityCurve::parse(v),
                    "--param" => got_p = params(Params::default(), v).unwrap(),
                    "--tuning" => got_t = tuning(Tuning::default(), v).unwrap(),
                    _ => panic!("{flag} in {line}"),
                }
            }
            let want = Tuning {
                pitch_ratio: a4 / 440.0,
                temperament: crate::temperament::offsets(temperament.0, temperament.1),
                ..t
            };
            assert_eq!((got_p, got_curve), (p, curve), "{line}");
            assert_eq!(got_t, want, "{line}");
        }
    }
}

#[cfg(test)]
mod mic_tests {
    use super::*;
    use crate::mic::MicPosition;

    #[test]
    fn temperament_keys() {
        let t = tuning(Tuning::default(), "key=2,temperament=3").unwrap();
        assert_eq!(t.temperament, crate::temperament::offsets(3, 2));
        assert_ne!(t.temperament, [0.0; 12]);
        let n = crate::temperament::NAMES.len();
        for bad in [format!("temperament={n}"), "temperament=1.5".into(), "key=12".into(), "key=-1".into()] {
            assert!(tuning(Tuning::default(), &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn lid_and_pillow_keys() {
        let t = tuning(Tuning::default(), "lid=0.25,pillow=0.5").unwrap();
        assert_eq!((t.lid, t.pillow), (0.25, 0.5));
        assert_eq!((Tuning::default().lid, Tuning::default().pillow), (1.0, 0.0));
    }

    #[test]
    fn the_bass_key_round_trips() {
        let t = tuning(Tuning::default(), "bass=24").unwrap();
        assert_eq!(t.board_bass_db, 24.0);
        let text = format(&Params::default(), &crate::velocity::VelocityCurve::STRAIGHT, &t, 440.0, (0, 0));
        assert!(text.contains("bass=24"), "{text}");
        let shipped = format(&Params::default(), &crate::velocity::VelocityCurve::STRAIGHT, &Tuning::default(), 440.0, (0, 0));
        assert!(!shipped.contains("bass"), "{shipped}");
    }

    #[test]
    fn tuning_keys_set_the_custom_pair() {
        let t = tuning(Tuning::default(), "mic=5,mic_x1=-1,mic_y1=-2,mic_z1=0.5,mic_pattern2=0.5,mic_yaw2=30,mic_pitch2=-10,mic_wall=1.2").unwrap();
        assert_eq!(t.mic, MicPosition::Custom);
        assert_eq!(t.mic_setup.mics[0].pos, [-1.0, -2.0, 0.5]);
        assert_eq!((t.mic_setup.mics[1].pattern, t.mic_setup.mics[1].yaw, t.mic_setup.mics[1].pitch), (0.5, 30.0, -10.0));
        assert_eq!(t.mic_setup.wall, 1.2);
        assert!(tuning(Tuning::default(), "mic_q1=3").is_err());
    }

    #[test]
    fn mic_argument() {
        let m = mic("1, -2,0.5").unwrap();
        assert_eq!((m.pos, m.pattern), ([1.0, -2.0, 0.5], 0.0));
        let m = mic("0,-1.5,0.45,0.5,10,-5").unwrap();
        assert_eq!((m.pattern, m.yaw, m.pitch), (0.5, 10.0, -5.0));
        assert!(mic("1,2").is_err() && mic("1,2,3,4,5,6,7").is_err() && mic("a,b,c").is_err());
    }
}

#[cfg(test)]
mod morph_tests {
    use super::*;

    #[test]
    fn morph_sides_give_the_rest_to_the_first() {
        let s = morph_sides("a", &["b=0.25".into(), "x=y/c=0.5".into()]).unwrap();
        assert_eq!(s, vec![("a".into(), 0.25), ("b".into(), 0.25), ("x=y/c".into(), 0.5)]);
        assert!(morph_sides("a", &["b=0.7".into(), "c=0.4".into()]).is_err()); // Σ > 1
        assert!(morph_sides("a", &["b=-0.1".into()]).is_err());
        assert!(morph_sides("a", &["b".into()]).is_err());
        assert_eq!(morph_sides("a", &[]).unwrap(), vec![("a".into(), 1.0)]);
    }
}
