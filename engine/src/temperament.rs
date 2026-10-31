//! Historical temperaments, computed from their definitions: how much each of the twelve
//! fifths of the circle C–G–D–A–E–B–F♯–C♯–G♯–D♯–A♯–F–C is narrowed (cents; negative: wide).
//! The twelve temperings add up to the Pythagorean comma, so the circle closes. The
//! values are the old makers' and theorists' rules, not anyone's table; Aeolus's tables
//! (Fons Adriaensen, GPL-3) are only a cross-check in the tests.
//!
//! `offsets(index, key)`: per pitch class (C = 0) the deviation from equal temperament
//! in cents, the scale centered on `key` (its "C" on that pitch class), A kept at the
//! concert pitch. Equal temperament gives exact zeros.

/// pure fifth 3/2
const FIFTH: f64 = 701.9550008653874;
/// Pythagorean comma (12 pure fifths − 7 octaves)
const PC: f64 = 23.460010384649014;
/// syntonic comma 81/80
const SC: f64 = 21.50628959671478;

/// Names in the plugin's order (index 0 is the default). The last, "As recorded", is no
/// temperament of its own: equal's offsets, and each recorded key at its own pitch
/// (`Tuning::as_recorded`).
pub const NAMES: [&str; 9] = [
    "Equal",
    "Meantone 1/4 comma",
    "Meantone 1/6 comma",
    "Werckmeister III",
    "Kirnberger III",
    "Vallotti",
    "Young II",
    "Lehman (Bach)",
    "As recorded",
];

/// The index of "As recorded" in `NAMES`.
pub const AS_RECORDED: usize = 8;

/// The fifths' temperings (cents narrowed) in circle order from C–G to F–C.
pub fn temperings(index: usize) -> [f64; 12] {
    // a meantone: eleven fifths E♭…G♯ narrowed alike, the wolf G♯–E♭ takes the rest
    let meantone = |t: f64| {
        let mut f = [t; 12];
        f[8] = PC - 11.0 * t;
        f
    };
    match index {
        1 => meantone(SC / 4.0),
        2 => meantone(SC / 6.0),
        // C–G, G–D, D–A and B–F♯ narrowed by 1/4 Pythagorean comma, the rest pure
        3 => [PC / 4.0, PC / 4.0, PC / 4.0, 0.0, 0.0, PC / 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        // C–G–D–A–E by 1/4 syntonic comma (a pure third C–E), F♯–C♯ by the schisma
        4 => {
            let mut f = [0.0; 12];
            f[..4].fill(SC / 4.0);
            f[6] = PC - SC;
            f
        }
        // F–C–G–D–A–E–B by 1/6 Pythagorean comma, the rest pure
        5 => [PC / 6.0, PC / 6.0, PC / 6.0, PC / 6.0, PC / 6.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, PC / 6.0],
        // Young's second (1799): C–G–D–A–E–B–F♯ by 1/6 Pythagorean comma, the rest pure
        6 => [PC / 6.0, PC / 6.0, PC / 6.0, PC / 6.0, PC / 6.0, PC / 6.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        // Lehman (2005), from the title page of the Well-Tempered Clavier: F–C–G–D–A–E by
        // 1/6, E–B–F♯–C♯ pure, C♯–G♯–D♯–A♯ by 1/12, A♯–F 1/12 wide
        7 => [
            PC / 6.0,
            PC / 6.0,
            PC / 6.0,
            PC / 6.0,
            0.0,
            0.0,
            0.0,
            PC / 12.0,
            PC / 12.0,
            PC / 12.0,
            -PC / 12.0,
            PC / 6.0,
        ],
        _ => [PC / 12.0; 12],
    }
}

/// Pitch of each pitch class re C (cents, 0…1200), from the temperings.
pub fn positions(t: &[f64; 12]) -> [f64; 12] {
    let mut p = [0.0; 12];
    let (mut n, mut c) = (0usize, 0.0);
    for ti in t.iter().take(11) {
        c += FIFTH - ti;
        n = (n + 7) % 12;
        p[n] = c.rem_euclid(1200.0);
    }
    p
}

/// Deviation from equal temperament per pitch class (cents), the temperament centered on
/// `key` (0 = C … 11 = B), A unchanged. Index 0 (equal), "As recorded" or out of range: zeros.
pub fn offsets(index: usize, key: usize) -> [f64; 12] {
    if index == 0 || index == AS_RECORDED || index >= NAMES.len() {
        return [0.0; 12];
    }
    offsets_of(&positions(&temperings(index)), key)
}

/// The same for a scale given by the pitch of each degree re its first (cents, 0…1200).
pub fn offsets_of(p: &[f64; 12], key: usize) -> [f64; 12] {
    let dev = |pc: usize| {
        let d = p[(pc + 12 - key % 12) % 12] - 100.0 * ((pc + 12 - key % 12) % 12) as f64;
        // keep the deviation small (a class at 1199.x cents is −0.x)
        if d > 600.0 { d - 1200.0 } else { d }
    };
    let a = dev(9);
    let mut out = [0.0; 12];
    for (pc, o) in out.iter_mut().enumerate() {
        *o = dev(pc) - a;
    }
    out
}

/// One pitch line of a Scala file: cents (with a decimal point) or a ratio (`3/2`, `2`); a comment
/// after the number is ignored.
fn scala_pitch(l: &str) -> Result<f64, String> {
    let w = l.split_whitespace().next().ok_or("blank pitch line")?;
    if w.contains('.') {
        return w.parse::<f64>().map_err(|_| format!("bad pitch {w}"));
    }
    let (n, d) = w.split_once('/').unwrap_or((w, "1"));
    let (n, d): (f64, f64) = (n.parse().map_err(|_| format!("bad pitch {w}"))?, d.parse().map_err(|_| format!("bad pitch {w}"))?);
    if n <= 0.0 || d <= 0.0 {
        return Err(format!("bad pitch {w}"));
    }
    Ok(1200.0 * (n / d).log2())
}

/// A Scala `.scl` file: its description and the pitch of each degree 1 … n re degree 0 (cents), the
/// last the period it repeats at (the octave for most scales; any note count).
pub fn parse_scale(text: &str) -> Result<(String, Vec<f64>), String> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.starts_with('!'));
    let description = lines.next().ok_or("empty .scl file")?.to_string(); // may be blank
    let count: usize = lines
        .next()
        .and_then(|l| l.split_whitespace().next())
        .and_then(|w| w.parse().ok())
        .ok_or("missing note count")?;
    if count == 0 || count > 1024 {
        return Err(format!("{count} notes: 1 to 1024 are supported"));
    }
    let cents = lines.take(count).map(scala_pitch).collect::<Result<Vec<f64>, String>>()?;
    if cents.len() != count {
        return Err(format!("{} pitches for {count} notes", cents.len()));
    }
    if cents[count - 1] <= 0.0 {
        return Err(format!("the scale repeats at {:.2} cents: it must rise", cents[count - 1]));
    }
    Ok((description, cents))
}

/// Parse a Scala `.scl` file of a twelve-note scale closing at the octave into the pitch of each
/// degree re the first (cents; degree 0 is 0), for `offsets_of`.
pub fn parse_scl(text: &str) -> Result<[f64; 12], String> {
    let (_, cents) = parse_scale(text)?;
    if cents.len() != 12 {
        return Err(format!("{} notes per octave: only twelve-note scales are supported here", cents.len()));
    }
    if (cents[11] - 1200.0).abs() > 0.01 {
        return Err(format!("the scale closes at {:.2} cents, not at the octave", cents[11]));
    }
    let mut p = [0.0; 12];
    p[1..].copy_from_slice(&cents[..11]);
    Ok(p)
}

/// A Scala keyboard mapping (`.kbm`): the keys first … last are retuned, `middle` sounds degree 0,
/// `reference` sounds `reference_hz`; the map repeats every `map.len()` keys (0: one key per degree)
/// at `period_degree` degrees; a key the map leaves out (`x`, None) keeps equal temperament.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyMap {
    pub first: i32,
    pub last: i32,
    pub middle: i32,
    pub reference: i32,
    pub reference_hz: f64,
    pub period_degree: i64,
    pub map: Vec<Option<i64>>,
}

/// Parse a Scala `.kbm` file.
pub fn parse_kbm(text: &str) -> Result<KeyMap, String> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.starts_with('!') && !l.is_empty());
    let mut next = |what: &str| lines.next().and_then(|l| l.split_whitespace().next()).map(str::to_string).ok_or(format!("missing {what}"));
    let int = |w: String, what: &str| w.parse::<i64>().map_err(|_| format!("bad {what} {w}"));
    let size = int(next("map size")?, "map size")?;
    if !(0..=1024).contains(&size) {
        return Err(format!("map size {size}: 0 to 1024"));
    }
    let first = int(next("first key")?, "first key")? as i32;
    let last = int(next("last key")?, "last key")? as i32;
    let middle = int(next("middle key")?, "middle key")? as i32;
    let reference = int(next("reference key")?, "reference key")? as i32;
    let hz = next("reference frequency")?;
    let reference_hz: f64 = hz.parse().map_err(|_| format!("bad reference frequency {hz}"))?;
    if !(reference_hz > 0.0) {
        return Err(format!("bad reference frequency {hz}"));
    }
    let period_degree = int(next("octave degree")?, "octave degree")?;
    let mut map = Vec::with_capacity(size as usize);
    for i in 0..size {
        let w = next(&format!("map entry {i}")).unwrap_or_else(|_| "x".into()); // short maps: the rest unmapped
        map.push(if w == "x" { None } else { Some(int(w, "map entry")?) });
    }
    Ok(KeyMap { first, last, middle, reference, reference_hz, period_degree, map })
}

/// Per piano key (A0 … C8) its pitch re equal temperament at A4 = 440 Hz (cents) under `scale`
/// (`parse_scale`), laid on the keys by `map`; without one, one key per degree, degree 0 on the
/// pitch class `key` (0 C … 11 B) of middle C's octave and A4 kept at 440 Hz (a twelve-note octave
/// scale gives `offsets_of`'s values). Keys the map leaves out keep equal temperament; each key is
/// held within ±`MAX_SCALE_CENTS` of it.
pub fn scale_keys(scale: &[f64], map: Option<&KeyMap>, key: usize) -> [f64; 88] {
    let n = scale.len().max(1) as i64;
    let period = scale.last().copied().unwrap_or(1200.0);
    let at = |d: i64| d.div_euclid(n) as f64 * period + if d.rem_euclid(n) == 0 { 0.0 } else { scale[d.rem_euclid(n) as usize - 1] };
    let linear = KeyMap { first: 0, last: 127, middle: 60 + (key % 12) as i32, reference: 69, reference_hz: 440.0, period_degree: n, map: Vec::new() };
    let m = map.unwrap_or(&linear);
    // a key's degree re the middle key (None: not retuned)
    let degree = |k: i32| -> Option<i64> {
        if k < m.first || k > m.last {
            return None;
        }
        let off = (k - m.middle) as i64;
        if m.map.is_empty() {
            return Some(off);
        }
        let size = m.map.len() as i64;
        m.map[off.rem_euclid(size) as usize].map(|d| off.div_euclid(size) * m.period_degree + d)
    };
    // the reference sounds its frequency: where it is unmapped, as if mapped one key per degree
    let ref_cents = at(degree(m.reference).unwrap_or((m.reference - m.middle) as i64)) - 1200.0 * (m.reference_hz / 440.0).log2();
    let mut out = [0.0; 88];
    for (i, o) in out.iter_mut().enumerate() {
        let k = 21 + i as i32;
        if let Some(d) = degree(k) {
            *o = (at(d) - ref_cents - 100.0 * (k - 69) as f64).clamp(-MAX_SCALE_CENTS, MAX_SCALE_CENTS);
        }
    }
    out
}

/// the most a scale moves a key from equal temperament (cents): the strings are the recorded keys'
pub const MAX_SCALE_CENTS: f64 = 2400.0;

/// A Scala file's scale on the keys (`scale_keys`), with a key map of the same name beside it
/// (`name.kbm`) when there is one: the keys, the scale's description and note count, the key map's
/// path (None: one key per degree).
pub fn load_scale(path: &std::path::Path, key: usize) -> Result<([f64; 88], String, usize, Option<std::path::PathBuf>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (description, scale) = parse_scale(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let kbm = path.with_extension("kbm");
    let map = match std::fs::read_to_string(&kbm) {
        Ok(t) => Some(parse_kbm(&t).map_err(|e| format!("{}: {e}", kbm.display()))?),
        Err(_) => None,
    };
    Ok((scale_keys(&scale, map.as_ref(), key), description, scale.len(), map.map(|_| kbm)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cents(r: f64) -> f64 {
        1200.0 * r.log2()
    }

    #[test]
    fn every_circle_closes() {
        for (i, name) in NAMES.iter().enumerate() {
            let s: f64 = temperings(i).iter().sum();
            assert!((s - PC).abs() < 1e-9, "{name}: {s}");
        }
    }

    #[test]
    fn equal_is_exactly_zero() {
        for key in 0..12 {
            assert_eq!(offsets(0, key), [0.0; 12]);
        }
        assert_eq!(offsets(99, 0), [0.0; 12]);
    }

    /// Aeolus (source/scales.cc) ratios re C: equal to its rounding for the exact tables
    #[test]
    fn matches_aeolus() {
        let aeolus: [(usize, [f64; 12], f64); 4] = [
            (1, [1.0, 1.0449067, 1.1180340, 1.1962790, 1.25, 1.3374806, 1.3975425, 1.4953488, 1.5625, 1.6718508, 1.7888544, 1.8691860], 1e-3),
            (3, [1.0, 1.05349794, 1.11740331, 1.18518519, 1.25282725, 1.33333333, 1.40466392, 1.49492696, 1.58024691, 1.67043633, 1.77777778, 1.87924088], 1e-3),
            // Aeolus's Kirnberger III table is rounded (its four tempered fifths 4.8…6.2 cents)
            (4, [1.0, 1.05349794, 1.11848107, 1.18518519, 1.25000021, 1.33333333, 1.40625, 1.49542183, 1.58024691, 1.67176840, 1.77777778, 1.875], 0.75),
            (7, [1.0, 1.05826737, 1.11992982, 1.18786496, 1.25424281, 1.33634808, 1.41102316, 1.49661606, 1.58560949, 1.67610496, 1.77978647, 1.88136421], 1e-3),
        ];
        for (i, ratios, tol) in aeolus {
            let p = positions(&temperings(i));
            for pc in 0..12 {
                assert!((p[pc] - cents(ratios[pc])).abs() < tol, "{} pc {pc}: {} vs {}", NAMES[i], p[pc], cents(ratios[pc]));
            }
        }
    }

    /// Vallotti's published cents (re C); Aeolus's "Vallotti" is a syntonic-comma variant
    #[test]
    fn vallotti_published_values() {
        let v = [0.0, 94.135, 196.090, 298.045, 392.180, 501.955, 592.180, 698.045, 796.090, 894.135, 1000.0, 1090.225];
        let p = positions(&temperings(5));
        for pc in 0..12 {
            assert!((p[pc] - v[pc]).abs() < 2e-3, "pc {pc}: {} vs {}", p[pc], v[pc]);
        }
    }

    #[test]
    fn young_is_vallotti_a_fifth_up() {
        let (y, v) = (temperings(6), temperings(5));
        for i in 0..12 {
            assert!((y[i] - v[(i + 11) % 12]).abs() < 1e-12);
        }
    }

    #[test]
    fn key_rotates_and_a_stays() {
        let c = offsets(1, 0);
        let e_flat = offsets(1, 3);
        for key in 0..12 {
            assert!(offsets(1, key)[9].abs() < 1e-9);
        }
        // the same shape three semitones up, re-referenced to A
        let shift = c[6] - e_flat[9];
        for pc in 0..12 {
            assert!((e_flat[(pc + 3) % 12] - (c[pc] - shift)).abs() < 1e-9, "pc {pc}");
        }
        // quarter-comma meantone in C: E is 13.7 cents below equal re C (pure third)
        assert!(((c[4] - c[0]) - (386.3137 - 400.0)).abs() < 1e-3);
    }

    const EQUAL_SCL: &str = "! 12-tet.scl\n!\n12 note equal temperament\n 12\n!\n 100.0\n 200.0 cents\n 300.\n 400.\n 500.\n 600.\n 700.\n 800.\n 900.\n 1000.\n 1100.\n 2/1\n";

    #[test]
    fn scl_equal_gives_zeros() {
        let p = parse_scl(EQUAL_SCL).unwrap();
        for key in 0..12 {
            for o in offsets_of(&p, key) {
                assert!(o.abs() < 1e-9);
            }
        }
    }

    /// Ratios and cents, against a built-in temperament: quarter-comma meantone as ratios
    /// (the Aeolus table of the test above) must come out as `offsets(1, key)`.
    #[test]
    fn scl_ratios_reproduce_a_builtin() {
        let ratios: [f64; 12] = [1.0, 1.0449067, 1.1180340, 1.1962790, 1.25, 1.3374806, 1.3975425, 1.4953488, 1.5625, 1.6718508, 1.7888544, 1.8691860];
        let mut text = String::from("meantone\n12\n");
        for r in &ratios[1..] {
            text += &format!("{:.9}\n", 1200.0 * r.log2()); // cents with a decimal point
        }
        text += "2\n"; // the octave as a bare integer ratio
        let p = parse_scl(&text).unwrap();
        for key in [0, 3, 7] {
            let (a, b) = (offsets_of(&p, key), offsets(1, key));
            for pc in 0..12 {
                assert!((a[pc] - b[pc]).abs() < 1e-3, "key {key} pc {pc}: {} vs {}", a[pc], b[pc]);
            }
        }
    }

    #[test]
    fn scl_rejects_what_cannot_be_mapped() {
        assert!(parse_scl("").is_err());
        assert!(parse_scl("x\n7\n9/8\n5/4\n4/3\n3/2\n5/3\n15/8\n2/1\n").unwrap_err().contains("only twelve"));
        assert!(parse_scale("x\n7\n1/1\n").unwrap_err().contains("1 pitches for 7"));
        let open = EQUAL_SCL.replace("2/1", "3/1");
        assert!(parse_scl(&open).unwrap_err().contains("octave"));
        assert!(parse_scl(&EQUAL_SCL.replace(" 300.", " abc")).is_err());
        assert!(parse_scl("x\n12\n100.\n").is_err());
    }

    #[test]
    fn a_twelve_note_scale_on_the_keys_is_its_temperament() {
        // Werckmeister III as a Scala file, rotated: each key its pitch class's offset
        let p = positions(&temperings(3));
        let scale: Vec<f64> = p[1..].iter().copied().chain([1200.0]).collect();
        for key in [0, 2, 7] {
            let o = offsets_of(&p, key);
            let k = scale_keys(&scale, None, key);
            for (i, c) in k.iter().enumerate() {
                assert!((c - o[(21 + i) % 12]).abs() < 1e-9, "key {key}, note {}: {c} vs {}", 21 + i, o[(21 + i) % 12]);
            }
        }
    }

    #[test]
    fn another_note_count_runs_one_key_per_degree() {
        // 19 equal: A4 stays, each key a 19th of an octave above the one below
        let scale: Vec<f64> = (1..=19).map(|i| 1200.0 * i as f64 / 19.0).collect();
        let k = scale_keys(&scale, None, 0);
        assert!(k[69 - 21].abs() < 1e-9);
        assert!((k[70 - 21] - (1200.0 / 19.0 - 100.0)).abs() < 1e-9);
        assert!((k[50 - 21] - (-19.0 * 1200.0 / 19.0 + 1900.0)).abs() < 1e-9); // 19 keys down: an octave
        assert!(k.iter().all(|c| c.abs() <= MAX_SCALE_CENTS));
        let (_, parsed) = parse_scale("19-EDO\n19\n63.15789\n").map(|(d, c)| (d, c.len())).unwrap_or_default();
        assert_eq!(parsed, 0, "short files are refused");
    }

    #[test]
    fn a_key_map_lays_the_scale_on_chosen_keys() {
        // a 7-note just major scale on the white keys, middle C degree 0, A4 = 440, black keys unmapped
        let scale: Vec<f64> = ["9/8", "5/4", "4/3", "3/2", "5/3", "15/8", "2/1"].iter().map(|r| scala_pitch(r).unwrap()).collect();
        let kbm = "! white keys\n12\n0\n127\n60\n69\n440.0\n7\n0\nx\n1\nx\n2\n3\nx\n4\nx\n5\nx\n6\n";
        let map = parse_kbm(kbm).unwrap();
        assert_eq!(map.map.len(), 12);
        let k = scale_keys(&scale, Some(&map), 0);
        assert!(k[69 - 21].abs() < 1e-9); // A4 the reference
        assert!(k[61 - 21] == 0.0); // C#4 unmapped: equal
        // E4 a just third above C4 where A4 is 5/3 above C4: 386.3 - 884.4 below A, re equal's -500
        let e = 1200.0 * (5.0f64 / 4.0).log2() - 1200.0 * (5.0f64 / 3.0).log2() + 500.0;
        assert!((k[64 - 21] - e).abs() < 1e-9, "{} vs {e}", k[64 - 21]);
        assert!(parse_kbm("12\n0\n").is_err());
    }

    #[test]
    fn a_key_map_beside_the_file_is_found() {
        let dir = std::env::temp_dir().join(format!("pe-scale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let scl = dir.join("major.scl");
        std::fs::write(&scl, "just major\n7\n9/8\n5/4\n4/3\n3/2\n5/3\n15/8\n2/1\n").unwrap();
        let (_, d, n, kbm) = load_scale(&scl, 0).unwrap();
        assert_eq!((d.as_str(), n, kbm), ("just major", 7, None));
        std::fs::write(dir.join("major.kbm"), "12\n0\n127\n60\n69\n440\n7\n0\nx\n1\nx\n2\n3\nx\n4\nx\n5\nx\n6\n").unwrap();
        assert!(load_scale(&scl, 0).unwrap().3.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

