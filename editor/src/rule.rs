//! A source's rule as the pages show it: how its files are read (the parts of their names, an
//! SFZ file, or a custom map only Advanced edits), its layers and which of them the fits use as
//! pp, mf and ff, and its levels. Read from a source and applied back onto it: keys the rule does
//! not model pass through, and reading then applying gives the source back unchanged.
use crate::mapping;
use crate::audio::Measures;
use crate::pyfmt::{basename, py_str};
use regex::Regex;
use serde_json::{Map, Value as Json, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;


/// what a part of the files' names is
#[derive(Clone, Debug, PartialEq)]
pub enum PartRole {
    Fixed,
    NoteName,
    Midi,
    Counted,
    Dynamic,
    Take,
    Group,
    AnyDigits,
    AnyLetters,
    /// a fragment written by hand (kept as it is)
    Other,
    /// a named group the map's `fields` read (kept as it is)
    Field(String),
}

/// a part: its text in an example file, its role, its regex fragment (as written, or from the role)
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub text: String,
    pub role: PartRole,
    pub pattern: String,
}

/// the separators the suggestion splits names at
const SEPARATORS: &str = "_-. /";

pub fn is_separator(p: &Part) -> bool {
    p.role == PartRole::Fixed && !p.text.is_empty() && p.text.chars().all(|c| SEPARATORS.contains(c))
}

/// one element of a regex at its top level
enum Atom {
    /// a literal character and its text in the regex ("." for `\.`)
    Lit(char, String),
    /// a group (its name, if named) with any quantifier
    Group(Option<String>, String),
    /// a class, `\d`, `.` or a quantified literal
    Class(String),
}

static GROUP_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\(\?P?<([A-Za-z_][A-Za-z0-9_]*)>").unwrap());
static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\\d)+(\+|\{\d+(,\d*)?\})?$").unwrap());
const LETTERS: [&str; 4] = [r"[^/_\-. 0-9]+", "[A-Za-z]+", "[a-z]+", "[A-Z]+"];

pub fn group_name(pattern: &str) -> Option<String> {
    GROUP_NAME.captures(pattern).map(|c| c[1].to_string())
}

fn close_bracket(c: &[char], open: usize) -> Option<usize> {
    let mut i = open + 1;
    if c.get(i) == Some(&'^') {
        i += 1;
    }
    if c.get(i) == Some(&']') {
        i += 1;
    }
    let mut depth = 1;
    while i < c.len() {
        match c[i] {
            '\\' => i += 1,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn close_paren(c: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut i = open;
    while i < c.len() {
        match c[i] {
            '\\' => i += 1,
            '[' => i = close_bracket(c, i)?,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// the length of a quantifier at `i` (0: none)
fn quantifier(c: &[char], i: usize) -> usize {
    let n = match c.get(i) {
        Some('*' | '+' | '?') => 1,
        Some('{') => match c[i..].iter().position(|&x| x == '}') {
            Some(e) if c[i + 1..i + e].iter().all(|x| x.is_ascii_digit() || *x == ',') => e + 1,
            _ => return 0,
        },
        _ => return 0,
    };
    if matches!(c.get(i + n), Some('?' | '+')) { n + 1 } else { n }
}

fn atoms(rx: &str) -> Option<Vec<Atom>> {
    let c: Vec<char> = rx.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let start = i;
        let atom = match c[i] {
            '(' => {
                let end = close_paren(&c, i)?;
                i = end + 1;
                let body: String = c[start..=end].iter().collect();
                Atom::Group(group_name(&body), body)
            }
            '[' => {
                let end = close_bracket(&c, i)?;
                i = end + 1;
                Atom::Class(c[start..=end].iter().collect())
            }
            '\\' => {
                let n = *c.get(i + 1)?;
                i += 2;
                if "dDwWsS".contains(n) {
                    Atom::Class(format!("\\{n}"))
                } else if n.is_ascii_alphanumeric() {
                    // anchors (\b, \A) and characters by code (\x41): not a plain sequence
                    return None;
                } else {
                    Atom::Lit(n, format!("\\{n}"))
                }
            }
            '.' => {
                i += 1;
                Atom::Class(".".into())
            }
            '|' | '^' | '$' | ')' | '*' | '+' | '?' | '{' => return None,
            ch => {
                i += 1;
                Atom::Lit(ch, ch.to_string())
            }
        };
        let q = quantifier(&c, i);
        let qs: String = c[i..i + q].iter().collect();
        i += q;
        out.push(match atom {
            Atom::Lit(_, s) if q > 0 => Atom::Class(s + &qs),
            Atom::Class(s) => Atom::Class(s + &qs),
            Atom::Group(n, s) => Atom::Group(n, s + &qs),
            lit => lit,
        });
    }
    Some(out)
}

fn char_class(ch: char) -> u8 {
    if SEPARATORS.contains(ch) {
        0
    } else if ch.is_ascii_digit() {
        1
    } else {
        2
    }
}

fn named_role(name: &str, note: &Json) -> PartRole {
    let group = note.get("group").and_then(Json::as_str).unwrap_or("note");
    if name == group {
        return match note.get("style").and_then(Json::as_str).unwrap_or("name") {
            "midi" => PartRole::Midi,
            "index" => PartRole::Counted,
            _ => PartRole::NoteName,
        };
    }
    match name {
        "dynamic" => PartRole::Dynamic,
        "take" => PartRole::Take,
        n => PartRole::Field(n.to_string()),
    }
}

fn any_role(pattern: &str) -> PartRole {
    if DIGITS.is_match(pattern) {
        PartRole::AnyDigits
    } else if LETTERS.contains(&pattern) {
        PartRole::AnyLetters
    } else {
        PartRole::Other
    }
}

/// a regex as parts: literal runs split into separators (one each) and runs of letters or of
/// digits; every group and class a part of its own, its role from its name and the note spec
pub fn parts_of(regex: &str, note: &Json) -> Option<Vec<Part>> {
    let mut parts: Vec<Part> = Vec::new();
    let mut lit: Option<(u8, Part)> = None;
    for a in atoms(regex)? {
        match a {
            Atom::Lit(ch, src) => {
                let k = char_class(ch);
                match &mut lit {
                    Some((kk, p)) if *kk == k && k != 0 => {
                        p.text.push(ch);
                        p.pattern.push_str(&src);
                    }
                    _ => {
                        if let Some((_, p)) = lit.take() {
                            parts.push(p);
                        }
                        lit = Some((k, Part { text: ch.to_string(), role: PartRole::Fixed, pattern: src }));
                    }
                }
            }
            other => {
                if let Some((_, p)) = lit.take() {
                    parts.push(p);
                }
                let (role, pattern) = match other {
                    Atom::Group(Some(n), s) => (named_role(&n, note), s),
                    Atom::Group(None, s) | Atom::Class(s) => (any_role(&s), s),
                    Atom::Lit(..) => unreachable!(),
                };
                parts.push(Part { text: String::new(), role, pattern });
            }
        }
    }
    if let Some((_, p)) = lit.take() {
        parts.push(p);
    }
    Some(parts)
}

pub fn regex_of(parts: &[Part]) -> String {
    parts.iter().map(|p| p.pattern.as_str()).collect()
}

/// the parts' regex with each part in a group of its own, to read each part's text in a name
pub fn probe(parts: &[Part]) -> Option<Regex> {
    let body: String = parts.iter().enumerate().map(|(i, p)| format!("(?P<pe_part{i}>{})", p.pattern)).collect();
    Regex::new(&format!("^(?:{body})$")).ok()
}

pub fn part_texts(rx: &Regex, n: usize, target: &str) -> Option<Vec<String>> {
    let c = rx.captures(target)?;
    Some((0..n).map(|i| c.name(&format!("pe_part{i}")).map(|m| m.as_str().to_string()).unwrap_or_default()).collect())
}
/// a part's fragment for a role chosen on the page: a dynamic's from its values in the files
pub fn default_pattern(role: &PartRole, text: &str, values: &BTreeSet<String>) -> String {
    match role {
        PartRole::Group if !values.is_empty() => group_pattern(values.iter()),
        PartRole::Fixed | PartRole::Group | PartRole::Other | PartRole::Field(_) => regex::escape(text),
        PartRole::NoteName => r"(?P<note>[A-Ga-g][#bs]?\d)".into(),
        PartRole::Midi => r"(?P<midi>\d+)".into(),
        PartRole::Counted => r"(?P<index>\d+)".into(),
        PartRole::Dynamic => format!("(?P<dynamic>{})", dynamic_shape(values)),
        PartRole::Take => r"(?P<take>\d+)".into(),
        PartRole::AnyDigits => r"\d+".into(),
        PartRole::AnyLetters => LETTERS[0].into(),
    }
}

/// a group part's values as one pattern (the files of every group; a source takes one value)
fn group_pattern<'a>(values: impl Iterator<Item = &'a String>) -> String {
    let mut alts: Vec<String> = values.map(|v| regex::escape(v)).collect();
    alts.sort_by_key(|a| std::cmp::Reverse(a.len()));
    format!("(?:{})", alts.join("|"))
}

/// a dynamic part's values as a pattern: digits, letters, letters shared then digits (vl1,
/// vl2), else the values as alternatives (the longest first)
fn dynamic_shape(values: &BTreeSet<String>) -> String {
    let v: Vec<&String> = values.iter().collect();
    if v.is_empty() {
        return "[A-Za-z0-9]+".into();
    }
    if v.iter().all(|x| x.chars().all(|c| c.is_ascii_digit())) {
        return r"\d+".into();
    }
    if v.iter().all(|x| x.chars().all(|c| c.is_ascii_alphabetic())) {
        return "[A-Za-z]+".into();
    }
    let pre: String = v[0].chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits_after = |x: &String| x.strip_prefix(pre.as_str()).is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()));
    if !pre.is_empty() && v.iter().all(|x| digits_after(x)) {
        return format!(r"{}\d+", regex::escape(&pre));
    }
    let mut alts: Vec<String> = v.iter().map(|x| regex::escape(x)).collect();
    alts.sort_by_key(|a| std::cmp::Reverse(a.len()));
    alts.join("|")
}

/// the fits' three dynamics
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dyn {
    Pp,
    Mf,
    Ff,
}

impl Dyn {
    pub const ALL: [Dyn; 3] = [Dyn::Pp, Dyn::Mf, Dyn::Ff];

    pub fn key(self) -> &'static str {
        match self {
            Dyn::Pp => "pp",
            Dyn::Mf => "mf",
            Dyn::Ff => "ff",
        }
    }

    fn of(k: &str) -> Option<Dyn> {
        Dyn::ALL.into_iter().find(|d| d.key() == k)
    }
}

/// a layer: its value in the names (None: a map without a dynamic part, an SFZ's layer), its
/// name in the items, whether the map keeps it, which of the fits' dynamics it is
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub raw: Option<String>,
    pub name: Json,
    pub keep: bool,
    pub used_as: Option<Dyn>,
}

/// a pattern map shown as parts
#[derive(Clone, Debug, PartialEq)]
pub struct Names {
    pub glob: String,
    /// the regex reads the path below the root, not the file name
    pub relpath: bool,
    pub parts: Vec<Part>,
    /// note.shift (note names, MIDI numbers)
    pub shift: i64,
    /// note.offset (numbers counted from a key)
    pub offset: i64,
    pub prefer: Vec<String>,
    pub order: Option<String>,
    /// the group part's values (the wizard makes one source per value)
    pub group: Option<Vec<String>>,
}

fn role_kind(r: &PartRole) -> u8 {
    match r {
        PartRole::NoteName | PartRole::Midi | PartRole::Counted => 1,
        PartRole::Dynamic => 2,
        PartRole::Take => 3,
        PartRole::Group => 4,
        _ => 0,
    }
}

impl Names {
    /// the parts' example texts from the first file the rule reads (fixed parts keep theirs)
    pub fn fill_texts(&mut self, p: &Preview) {
        let Some((_, texts)) = p.rows.first() else { return };
        for (part, t) in self.parts.iter_mut().zip(texts) {
            if part.role != PartRole::Fixed {
                part.text = t.clone();
            }
        }
    }

    pub fn regex(&self) -> String {
        regex_of(&self.parts)
    }

    /// the map's note spec from the note part (its group name and role), the shift, the offset
    pub fn note_spec(&self) -> Option<Json> {
        let p = self.parts.iter().find(|p| role_kind(&p.role) == 1)?;
        let mut s = Map::new();
        s.insert("group".into(), json!(group_name(&p.pattern)?));
        match p.role {
            PartRole::Midi => {
                s.insert("style".into(), json!("midi"));
            }
            PartRole::Counted => {
                s.insert("style".into(), json!("index"));
            }
            _ => {}
        }
        if p.role == PartRole::Counted {
            if self.offset != 0 {
                s.insert("offset".into(), json!(self.offset));
            }
        } else if self.shift != 0 {
            s.insert("shift".into(), json!(self.shift));
        }
        Some(Json::Object(s))
    }

    /// a part's role chosen: its fragment follows the role (`values`: its texts in the files);
    /// the note, the dynamic, the take and the group are one part each, so the part that had the
    /// role becomes "any"; a note name takes in the octave digit right after it
    pub fn set_role(&mut self, i: usize, role: PartRole, values: &BTreeSet<String>) {
        let k = role_kind(&role);
        for j in 0..self.parts.len() {
            if j != i && k > 0 && role_kind(&self.parts[j].role) == k {
                let p = &mut self.parts[j];
                let digits = !p.text.is_empty() && p.text.chars().all(|c| c.is_ascii_digit());
                p.role = match p.role {
                    PartRole::Midi | PartRole::Counted | PartRole::Take => PartRole::AnyDigits,
                    _ if digits => PartRole::AnyDigits,
                    _ => PartRole::AnyLetters,
                };
                p.pattern = default_pattern(&p.role, &p.text, &BTreeSet::new());
            }
        }
        let digit_next = self.parts.get(i + 1).is_some_and(|n| {
            n.text.len() == 1 && n.text.chars().all(|c| c.is_ascii_digit()) && matches!(n.role, PartRole::Fixed | PartRole::AnyDigits)
        });
        if role == PartRole::NoteName && digit_next {
            let n = self.parts.remove(i + 1);
            self.parts[i].text.push_str(&n.text);
        }
        let p = &mut self.parts[i];
        p.pattern = default_pattern(&role, &p.text, values);
        p.role = role.clone();
        let has = |r: &PartRole| self.parts.iter().any(|p| p.role == *r);
        let (take, group) = (has(&PartRole::Take), has(&PartRole::Group));
        if take && !self.prefer.iter().any(|x| x == "take") {
            self.prefer.push("take".into());
        } else if !take {
            self.prefer.retain(|x| x != "take");
        }
        if role == PartRole::Group {
            self.group = Some(values.iter().cloned().collect());
        } else if !group {
            self.group = None;
        }
    }
}

/// an SFZ map's choices the pages show
#[derive(Clone, Debug, PartialEq)]
pub struct Sfz {
    pub files: Vec<String>,
    pub folders: Vec<String>,
    pub prefer: Option<String>,
    pub filter: Vec<Json>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    Names(Names),
    Sfz(Sfz),
    /// a map the parts cannot show: only Advanced edits it
    Custom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub reading: Reading,
    pub layers: Vec<Layer>,
    pub levels: Option<String>,
    /// the source's dynamics name layer numbers (layer_key = "layer")
    pub by_layer: bool,
    /// dynamics entries no layer row shows (another key, a layer not kept)
    pub other_dynamics: Vec<(String, Json)>,
}

fn strs(v: Option<&Json>) -> Vec<String> {
    v.and_then(Json::as_array).into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect()
}

fn put(map: &mut Map<String, Json>, k: &str, v: Option<Json>) {
    match v {
        Some(v) => {
            map.insert(k.into(), v);
        }
        None => {
            map.remove(k);
        }
    }
}

/// the layer rows of a map: a pattern's `layers` (kept, in order) and the other values the files
/// hold (`found`, left out); a pattern without `layers` has its one `dynamic`; an SFZ's (or a
/// custom map's) are the values found, all kept
fn layers_of(map: &Json, found: &[String]) -> Vec<Layer> {
    let pattern = map.get("kind").and_then(Json::as_str) == Some("pattern");
    let row = |r: &String, keep: bool| Layer { raw: Some(r.clone()), name: mapping::dynamic_name(map, r).unwrap_or(json!(r)), keep, used_as: None };
    match map.get("layers") {
        Some(Json::Array(list)) if pattern => {
            let kept: Vec<String> = list.iter().map(py_str).collect();
            let mut out: Vec<Layer> = kept.iter().map(|r| row(r, true)).collect();
            out.extend(found.iter().filter(|f| !kept.contains(f)).map(|f| row(f, false)));
            out
        }
        _ if pattern => vec![Layer { raw: None, name: map.get("dynamic").cloned().unwrap_or(json!("mf")), keep: true, used_as: None }],
        _ => found.iter().map(|f| Layer { raw: None, name: json!(f), keep: true, used_as: None }).collect(),
    }
}

impl Rule {
    /// a source as a rule; `found`: the layer values its files hold (rule::layer_stats)
    pub fn read(src: &Json, found: &[String]) -> Rule {
        let map = src.get("map").cloned().unwrap_or(Json::Null);
        let note = map.get("note").cloned().unwrap_or(Json::Null);
        let reading = match map.get("kind").and_then(Json::as_str) {
            Some("sfz") => Reading::Sfz(Sfz {
                files: strs(map.get("files")),
                folders: strs(map.get("folders")),
                prefer: map.get("prefer_substring").and_then(Json::as_str).map(String::from),
                filter: map.get("filter").and_then(Json::as_array).cloned().unwrap_or_default(),
            }),
            Some("pattern") => {
                let parts = map.get("regex").and_then(Json::as_str).and_then(|rx| parts_of(rx, &note));
                match (parts, map.get("glob").and_then(Json::as_str)) {
                    (Some(parts), Some(glob)) => Reading::Names(Names {
                        glob: glob.into(),
                        relpath: map.get("match").and_then(Json::as_str) == Some("relpath"),
                        parts,
                        shift: note.get("shift").and_then(Json::as_i64).unwrap_or(0),
                        offset: note.get("offset").and_then(Json::as_i64).unwrap_or(0),
                        prefer: strs(map.get("prefer")),
                        order: map.get("order").and_then(Json::as_str).map(String::from),
                        group: None,
                    }),
                    _ => Reading::Custom,
                }
            }
            _ => Reading::Custom,
        };
        let mut rule = Rule {
            reading,
            layers: layers_of(&map, found),
            levels: src.get("levels").and_then(Json::as_str).map(String::from),
            by_layer: src.get("layer_key").and_then(Json::as_str) == Some("layer"),
            other_dynamics: Vec::new(),
        };
        for (k, v) in src.get("dynamics").and_then(Json::as_object).into_iter().flatten() {
            match (Dyn::of(k), rule.find_layer(v)) {
                (Some(d), Some(i)) if rule.layers[i].used_as.is_none() => rule.layers[i].used_as = Some(d),
                _ => rule.other_dynamics.push((k.clone(), v.clone())),
            }
        }
        rule
    }

    /// the kept layer a dynamics value names: by its name, or by its number among the kept ones
    fn find_layer(&self, v: &Json) -> Option<usize> {
        if self.by_layer {
            let n = usize::try_from(v.as_i64()? - 1).ok()?;
            return (0..self.layers.len()).filter(|&i| self.layers[i].keep).nth(n);
        }
        self.layers.iter().position(|l| l.keep && l.name == *v)
    }

    fn dynamics_value(&self, i: usize) -> Json {
        if self.by_layer {
            json!(self.layers[..i].iter().filter(|l| l.keep).count() + 1)
        } else {
            self.layers[i].name.clone()
        }
    }

    /// the source with the rule's keys set: levels, dynamics, the map's keys the reading shows;
    /// every other key as it was
    pub fn apply(&self, src: &Json) -> Json {
        let mut out = src.clone();
        let Some(o) = out.as_object_mut() else { return out };
        put(o, "levels", self.levels.as_ref().map(|l| json!(l)));
        let mut d = Map::new();
        for k in Dyn::ALL {
            if let Some(i) = self.layers.iter().position(|l| l.used_as == Some(k)) {
                d.insert(k.key().into(), self.dynamics_value(i));
            }
        }
        for (k, v) in &self.other_dynamics {
            d.entry(k.clone()).or_insert(v.clone());
        }
        put(o, "dynamics", (!d.is_empty()).then_some(Json::Object(d)));
        let Some(map) = o.get_mut("map").and_then(Json::as_object_mut) else { return out };
        let named = self.layers.iter().any(|l| l.raw.is_some());
        if named {
            let kept: Vec<Json> = self.layers.iter().filter(|l| l.keep).filter_map(|l| l.raw.clone().map(Json::String)).collect();
            map.insert("layers".into(), Json::Array(kept));
            map.remove("dynamic");
        } else if map.get("kind").and_then(Json::as_str) == Some("pattern") {
            let had_list = map.get("layers").is_some_and(Json::is_array);
            if had_list {
                map.remove("layers");
            }
            if let Some(l) = self.layers.first().filter(|_| had_list || map.contains_key("dynamic")) {
                map.insert("dynamic".into(), l.name.clone());
            }
        }
        match &self.reading {
            Reading::Names(n) => {
                map.insert("glob".into(), json!(n.glob));
                map.insert("regex".into(), json!(n.regex()));
                if n.relpath {
                    map.insert("match".into(), json!("relpath"));
                } else if map.get("match").and_then(Json::as_str) == Some("relpath") {
                    map.remove("match");
                }
                if let Some(s) = n.note_spec() {
                    map.insert("note".into(), s);
                }
                put(map, "prefer", (!n.prefer.is_empty()).then(|| json!(n.prefer)));
                if let Some(ord) = &n.order {
                    map.insert("order".into(), json!(ord));
                }
            }
            Reading::Sfz(s) => {
                map.insert("files".into(), json!(s.files));
                put(map, "folders", (!s.folders.is_empty()).then(|| json!(s.folders)));
                put(map, "prefer_substring", s.prefer.as_ref().map(|p| json!(p)));
                put(map, "filter", (!s.filter.is_empty()).then(|| json!(s.filter)));
            }
            Reading::Custom => {}
        }
        out
    }

    /// the layer rows after the parts changed: a dynamic part's values as found (rows keep their
    /// choices; new values are kept; a value gone is dropped unless kept); no dynamic part: the
    /// one layer, mf. Rows that changed with no layer used as pp, mf or ff: they are proposed.
    pub fn sync_layers(&mut self, map: &Json, found: &[String]) {
        let Reading::Names(n) = &self.reading else { return };
        let before: Vec<Option<String>> = self.layers.iter().map(|l| l.raw.clone()).collect();
        if n.parts.iter().any(|p| p.role == PartRole::Dynamic) {
            let mut rows: Vec<Layer> = self.layers.iter()
                .filter(|l| l.raw.as_ref().is_some_and(|r| l.keep || found.contains(r))).cloned().collect();
            for f in found {
                if !rows.iter().any(|l| l.raw.as_ref() == Some(f)) {
                    rows.push(Layer { raw: Some(f.clone()), name: json!(f), keep: true, used_as: None });
                }
            }
            for l in &mut rows {
                if let Some(r) = &l.raw {
                    l.name = mapping::dynamic_name(map, r).unwrap_or(json!(r));
                }
            }
            self.layers = rows;
        } else if self.layers.is_empty() || self.layers.iter().any(|l| l.raw.is_some()) {
            self.layers = vec![Layer { raw: None, name: json!("mf"), keep: true, used_as: Some(Dyn::Mf) }];
        }
        // the same rows: a dynamic cleared by hand stays cleared
        let same = self.layers.iter().map(|l| l.raw.clone()).eq(before);
        if !same && !self.layers.iter().any(|l| l.used_as.is_some()) {
            self.propose_dynamics();
        }
    }

    /// pp, mf and ff on the softest, middle and loudest kept layers (one: mf; two: mf and ff)
    pub fn propose_dynamics(&mut self) {
        let kept: Vec<usize> = (0..self.layers.len()).filter(|&i| self.layers[i].keep).collect();
        for l in &mut self.layers {
            l.used_as = None;
        }
        let picks: Vec<(usize, Dyn)> = match kept.len() {
            0 => vec![],
            1 => vec![(kept[0], Dyn::Mf)],
            2 => vec![(kept[0], Dyn::Mf), (kept[1], Dyn::Ff)],
            n => vec![(kept[0], Dyn::Pp), (kept[(n - 1) / 2], Dyn::Mf), (kept[n - 1], Dyn::Ff)],
        };
        for (i, d) in picks {
            self.layers[i].used_as = Some(d);
        }
    }

    /// a dynamic chosen for a layer: taken from the layer that had it; a used layer is kept
    pub fn use_as(&mut self, i: usize, d: Option<Dyn>) {
        if let Some(d) = d {
            for l in &mut self.layers {
                if l.used_as == Some(d) {
                    l.used_as = None;
                }
            }
            self.layers[i].keep = true;
        }
        self.layers[i].used_as = d;
    }

    /// a layer kept or left out (one left out is no dynamic); only a value of the names can be
    pub fn set_keep(&mut self, i: usize, keep: bool) {
        if self.layers[i].raw.is_none() {
            return;
        }
        self.layers[i].keep = keep;
        if !keep {
            self.layers[i].used_as = None;
        }
    }

    /// the kept layers softest first by their levels (unmeasured ones after them), then the rest
    pub fn order_by_level(&mut self, level: &dyn Fn(&Layer) -> Option<f32>) {
        self.layers.sort_by(|a, b| {
            b.keep.cmp(&a.keep).then_with(|| match (level(a), level(b)) {
                (Some(x), Some(y)) => x.total_cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
        });
    }

    pub fn group_values(&self) -> Option<Vec<String>> {
        match &self.reading {
            Reading::Names(n) => n.group.clone(),
            _ => None,
        }
    }

    /// the rule for one value of the group part
    pub fn for_group(&self, value: &str) -> Rule {
        let mut r = self.clone();
        if let Reading::Names(n) = &mut r.reading {
            for p in n.parts.iter_mut().filter(|p| p.role == PartRole::Group) {
                p.text = value.into();
                p.pattern = regex::escape(value);
            }
        }
        r
    }
}

/// a layer value the files hold: how many files, their median level (dB re the loudest value's)
#[derive(Clone, Debug, PartialEq)]
pub struct LayerStat {
    pub value: String,
    pub files: usize,
    pub level_db: Option<f32>,
}

/// the layer values a source's files hold: a pattern map's dynamic group (every value, also those
/// its layers leave out; in the order the files show them), else the items' dynamics (their layer
/// numbers with layer_key = "layer"), softest first
pub fn layer_stats(src: &Json, root: &str, m: &Measures) -> Vec<LayerStat> {
    let map = &src["map"];
    let by_layer = src.get("layer_key").and_then(Json::as_str) == Some("layer");
    let mut order: Vec<String> = Vec::new();
    let mut paths: HashMap<String, Vec<String>> = HashMap::new();
    let mut add = |v: String, p: String| {
        if !paths.contains_key(&v) {
            order.push(v.clone());
        }
        paths.entry(v).or_default().push(p);
    };
    let has_group = map.get("kind").and_then(Json::as_str) == Some("pattern")
        && map.get("regex").and_then(Json::as_str).is_some_and(|r| r.contains("(?P<dynamic>"));
    if has_group {
        if let Ok((matches, _)) = mapping::pattern_matches(root, map) {
            for x in matches {
                if let Some(Some(d)) = x.groups.get("dynamic") {
                    add(d.clone(), x.path);
                }
            }
        }
    } else if let Ok(r) = mapping::items(src, root) {
        // softest first: by the regions' velocities, else by the items' places among their key's layers
        let mut place: HashMap<String, (f64, usize)> = HashMap::new();
        for it in &r.items {
            let v = if by_layer { py_str(&it["layer"]) } else { py_str(&it["dynamic"]) };
            let at = match it.get("velocity").and_then(Json::as_array) {
                Some(lh) => lh.iter().filter_map(Json::as_f64).sum::<f64>() / 254.0,
                None => it["layer"].as_f64().unwrap_or(1.0) / it["layers"].as_f64().unwrap_or(1.0).max(1.0),
            };
            let e = place.entry(v.clone()).or_default();
            e.0 += at;
            e.1 += 1;
            add(v, it["path"].as_str().unwrap_or_default().to_string());
        }
        let mean = |v: &String| place.get(v).map_or(0.0, |(s, n)| s / *n as f64);
        order.sort_by(|a, b| mean(a).total_cmp(&mean(b)));
    }
    let median = |ps: &[String]| {
        let mut l: Vec<f32> = ps.iter().filter_map(|p| m.ok.get(p).map(|x| x.level_db)).collect();
        l.sort_by(f32::total_cmp);
        (!l.is_empty()).then(|| if l.len() % 2 == 1 { l[l.len() / 2] } else { (l[l.len() / 2 - 1] + l[l.len() / 2]) / 2.0 })
    };
    let med: HashMap<&String, f32> = order.iter().filter_map(|v| Some((v, median(&paths[v])?))).collect();
    let top = med.values().copied().fold(f32::MIN, f32::max);
    order.iter().map(|v| LayerStat { value: v.clone(), files: paths[v].len(), level_db: med.get(v).map(|l| l - top) }).collect()
}

/// what a names rule reads: each matched file (its relpath) with its parts' texts, the files of
/// another shape, each part's values
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    pub rows: Vec<(String, Vec<String>)>,
    pub other: Vec<String>,
    pub values: Vec<BTreeSet<String>>,
}

pub fn preview(n: &Names, root: &str) -> Result<Preview, String> {
    let mut map = json!({"kind": "pattern", "glob": n.glob, "regex": n.regex()});
    if n.relpath {
        map["match"] = json!("relpath");
    }
    let (matches, other) = mapping::pattern_matches(root, &map)?;
    let rx = probe(&n.parts).ok_or("the parts do not make a regex")?;
    let mut out = Preview { rows: Vec::new(), other, values: vec![BTreeSet::new(); n.parts.len()] };
    for x in matches {
        let target = if n.relpath { x.rel.clone() } else { basename(&x.rel).to_string() };
        if let Some(t) = part_texts(&rx, n.parts.len(), &target) {
            for (i, s) in t.iter().enumerate() {
                out.values[i].insert(s.clone());
            }
            out.rows.push((x.rel, t));
        }
    }
    Ok(out)
}

/// the folder names an SFZ map's region samples sit in, with the regions' counts, before the
/// map's folders and prefer_substring choose among them
pub fn sfz_folders(src: &Json, root: &str) -> Vec<(String, usize)> {
    let mut probe_src = src.clone();
    if let Some(m) = probe_src.get_mut("map").and_then(Json::as_object_mut) {
        m.remove("folders");
        m.remove("prefer_substring");
    }
    let mut count: std::collections::BTreeMap<String, usize> = Default::default();
    for it in mapping::items(&probe_src, root).map(|r| r.items).unwrap_or_default() {
        let rel = it["relpath"].as_str().unwrap_or_default();
        let dirs: Vec<&str> = rel.split('/').collect();
        for d in dirs[..dirs.len().saturating_sub(1)].iter().collect::<BTreeSet<_>>() {
            *count.entry(d.to_string()).or_default() += 1;
        }
    }
    count.into_iter().collect()
}

/// the suggestion as a rule: its source read, and the group part (a word the files fall into
/// groups by: the suggestion's first group value is in the regex) marked with the groups' values
pub fn from_suggestion(s: &crate::suggest::Suggestion, found: &[String]) -> Rule {
    let mut r = Rule::read(&s.source, found);
    let values: Vec<String> = s.groups.iter().map(|g| g.value.clone()).collect();
    if let (Reading::Names(n), Some(first)) = (&mut r.reading, values.first()) {
        if let Some(i) = n.parts.iter().position(|p| p.role == PartRole::Fixed && p.text == *first) {
            n.parts[i].role = PartRole::Group;
            n.parts[i].pattern = group_pattern(values.iter());
            n.group = Some(values);
        }
    }
    r
}

/// the keys a suggestion writes: switching how a source is read stashes and restores them
pub const SUGGESTED: [&str; 5] = ["root", "levels", "dynamics", "release", "map"];

/// the SFZ file a source's map reads (None: the file names)
pub fn reading_of(src: &Json) -> Option<String> {
    (src["map"]["kind"] == "sfz").then(|| src["map"]["files"][0].as_str().unwrap_or_default().to_string())
}

/// the ways of reading a source was switched away from, by choice (an SFZ file, "" the names):
/// switching back restores their keys; a new folder forgets them (they read the old one)
#[derive(Clone, Debug, Default)]
pub struct Stash(HashMap<String, Vec<(String, Option<Json>)>>);

impl Stash {
    /// the source's way of reading, kept before another is chosen
    pub fn put(&mut self, src: &Json) {
        let kept = SUGGESTED.iter().map(|k| (k.to_string(), src.get(*k).cloned())).collect();
        self.0.insert(reading_of(src).unwrap_or_default(), kept);
    }

    /// the source with a way kept earlier put back (None: none kept for that choice)
    pub fn restore(&self, src: &Json, choice: Option<&str>) -> Option<Json> {
        let kept = self.0.get(choice.unwrap_or(""))?;
        let mut new = src.clone();
        for (k, v) in kept {
            match v {
                Some(x) => new[k.as_str()] = x.clone(),
                None => {
                    new.as_object_mut().map(|o| o.remove(k));
                }
            }
        }
        Some(new)
    }

    pub fn folder_changed(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// every regex the committed projects use (2026-10-14)
    const COMMITTED: [&str; 25] = [
        r"BowedPsaltery_(?P<note>[A-G]#?\d)_Main_Pluck_rr\d+\.wav",
        r"chimes_(?P<note>[A-G]#?\d)\.ogg",
        r"EthnicXylo_[a-z]+M_(?P<note>[A-G]#?\d)_(?P<dynamic>vl\d)_rr(?P<take>\d+)_Mid\.wav",
        r"HEADROOM PIANO LEVEL(?P<dynamic>\d) CLOSE (?P<midi>\d+)\.flac",
        r"KSHarp_(?P<note>[A-G]#?\d)_(?P<dynamic>[a-z]+)(?P<take>\d+)\.wav",
        r"MBira3_pluck_Main_(?P<note>[A-G]#?\d)_k\d+(?:alt)?_50_100_rr\d+\.wav",
        r"notes/bells\.brass/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/bells\.plastic/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/crotale/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/guitar\.s(?P<string>\d)/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/marimba\.cord/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/marimba\.rubber/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/marimba\.yarn/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/vibraphone\.dampen/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/vibraphone\.sustain/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/xylophone\.hardrubber/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"notes/xylophone\.rosewood/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
        r"Piano\.(?P<dynamic>pp|mf|ff)\.(?P<note>[A-G]b?\d)\.aiff",
        r"(?P<index>\d\d)-PedalOff(?P<dynamic>[A-Za-z]+)1Close\.flac",
        r"(?P<note>[A-Ga-g]#?\d)_(?P<dynamic>mf|f|ff)_(?P<take>\d+)\.wav",
        r"(?P<note>[A-G]#?\d)v(?P<dynamic>\d+)\.wav",
        r"(?P<note>[A-G]s?\d)_.*",
        r"Strumstick_Finger_Str(?P<string>\d)_Main_(?P<note>[A-G]#?\d)_(?P<dynamic>vl\d)_rr(?P<take>\d+)\.wav",
        r"sus_(?P<note>[A-G]#?\d)_r\d+_main\.wav",
        r"TB_hit_(?P<note>[A-G]#?\d)_(?P<dynamic>v\d)_(?P<take>\d+)\.wav",
    ];

    fn roles(parts: &[Part]) -> Vec<PartRole> {
        parts.iter().filter(|p| !is_separator(p)).map(|p| p.role.clone()).collect()
    }

    #[test]
    fn every_committed_regex_splits_and_joins_back() {
        for rx in COMMITTED {
            let parts = parts_of(rx, &json!({"group": "note"})).unwrap_or_else(|| panic!("{rx}: no parts"));
            assert_eq!(regex_of(&parts), rx);
        }
    }

    #[test]
    fn parts_take_their_roles_from_the_groups_and_the_note_spec() {
        use PartRole::*;
        let p = parts_of(COMMITTED[2], &json!({"group": "note", "shift": 12})).unwrap();
        assert_eq!(roles(&p), [Fixed, AnyLetters, Fixed, NoteName, Dynamic, Fixed, Take, Fixed, Fixed]);
        // EthnicXylo _ [a-z]+ M _ note _ dynamic _ rr take _ Mid . wav: "M" is a fixed part of its own
        let p = parts_of(COMMITTED[3], &json!({"group": "midi", "style": "midi"})).unwrap();
        assert_eq!(roles(&p), [Fixed, Fixed, Fixed, Dynamic, Fixed, Midi, Fixed]);
        let p = parts_of(COMMITTED[18], &json!({"group": "index", "style": "index", "offset": 20})).unwrap();
        assert_eq!(roles(&p)[0], Counted);
        let p = parts_of(COMMITTED[9], &json!({"group": "midi", "style": "midi"})).unwrap();
        assert!(roles(&p).contains(&Field("string".into())));
        let p = parts_of(COMMITTED[5], &json!({"group": "note"})).unwrap();
        assert!(p.iter().any(|x| x.role == Other && x.pattern == "(?:alt)?"));
        assert!(p.iter().any(|x| x.role == AnyDigits && x.pattern == r"\d+"));
    }

    #[test]
    fn a_regex_that_is_not_a_sequence_has_no_parts() {
        for rx in [r"a|b", r"^C4\.wav$", r"(?P<note>[A-G]\d", r"\bC4"] {
            assert!(parts_of(rx, &json!({})).is_none(), "{rx}");
        }
    }

    #[test]
    fn separators_stand_alone_and_letters_and_digits_split() {
        let p = parts_of(r"rr12_Mid\.wav", &json!({})).unwrap();
        let texts: Vec<&str> = p.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, ["rr", "12", "_", "Mid", ".", "wav"]);
        assert_eq!(p[4].pattern, r"\.");
        assert!(is_separator(&p[2]) && is_separator(&p[4]) && !is_separator(&p[0]));
    }

    #[test]
    fn the_probe_gives_each_parts_text() {
        let p = parts_of(COMMITTED[2], &json!({"group": "note"})).unwrap();
        let rx = probe(&p).unwrap();
        let t = part_texts(&rx, p.len(), "EthnicXylo_tradM_C4_vl2_rr1_Mid.wav").unwrap();
        assert_eq!(t.join("|"), "EthnicXylo|_|trad|M|_|C4|_|vl2|_|rr|1|_|Mid|.|wav");
        assert!(part_texts(&rx, p.len(), "other.wav").is_none());
    }

    #[test]
    fn group_names_are_read_from_a_fragment() {
        assert_eq!(group_name(r"(?P<midi>\d+)").as_deref(), Some("midi"));
        assert_eq!(group_name(r"(?<note>[A-G]\d)").as_deref(), Some("note"));
        assert_eq!(group_name(r"(?:alt)?"), None);
    }

    fn balafon_like() -> Json {
        json!({"id": "s", "role": "mallet", "root": "r", "dynamics": {"pp": "pp", "mf": "mf", "ff": "ff"}, "levels": "normalized",
            "extra": {"strike": true},
            "map": {"kind": "pattern", "glob": "a/*.wav",
                "regex": r"X_(?P<note>[A-G]#?\d)_(?P<dynamic>vl\d)_rr(?P<take>\d+)\.wav", "note": {"group": "note", "shift": 12},
                "layers": ["vl1", "vl2", "vl3"], "dynamic_names": {"vl1": "pp", "vl2": "mf", "vl3": "ff"},
                "prefer": ["take"], "order": "note"}})
    }

    fn names(r: &Rule) -> &Names {
        match &r.reading { Reading::Names(n) => n, _ => panic!("not names") }
    }

    #[test]
    fn a_source_reads_and_applies_back_unchanged() {
        let src = balafon_like();
        let r = Rule::read(&src, &["vl1".into(), "vl2".into(), "vl3".into(), "vl4".into()]);
        assert_eq!(r.layers.len(), 4);
        assert_eq!((r.layers[0].name.clone(), r.layers[0].used_as), (json!("pp"), Some(Dyn::Pp)));
        assert!(!r.layers[3].keep, "a value the layers leave out is a row, not kept");
        assert_eq!(names(&r).shift, 12);
        assert_eq!(r.apply(&src), src);
        let mut r2 = r.clone();
        r2.sync_layers(&src["map"], &["vl1".into(), "vl2".into(), "vl3".into(), "vl4".into()]);
        assert_eq!(r2.apply(&src), src, "a sync with nothing changed changes nothing");
    }

    #[test]
    fn a_layer_left_out_and_taken_back() {
        let src = balafon_like();
        let found: Vec<String> = ["vl1", "vl2", "vl3", "vl4"].map(String::from).to_vec();
        let mut r = Rule::read(&src, &found);
        r.set_keep(1, false);
        assert_eq!(r.layers[1].used_as, None, "a layer left out is no dynamic");
        let out = r.apply(&src);
        assert_eq!(out["map"]["layers"], json!(["vl1", "vl3"]));
        assert_eq!(out["dynamics"], json!({"pp": "pp", "ff": "ff"}));
        r.set_keep(1, true);
        r.use_as(1, Some(Dyn::Mf));
        r.set_keep(3, true);
        let out = r.apply(&src);
        assert_eq!(out["map"]["layers"], json!(["vl1", "vl2", "vl3", "vl4"]));
        assert_eq!(out["dynamics"], json!({"pp": "pp", "mf": "mf", "ff": "ff"}));
    }

    #[test]
    fn a_dynamic_chosen_twice_moves() {
        let src = balafon_like();
        let mut r = Rule::read(&src, &["vl1".into(), "vl2".into(), "vl3".into()]);
        r.use_as(1, Some(Dyn::Pp));
        assert_eq!(r.layers[0].used_as, None);
        assert_eq!(r.layers[1].used_as, Some(Dyn::Pp));
    }

    #[test]
    fn layer_numbers_name_the_kept_layers() {
        let src = json!({"id": "s", "dynamics": {"pp": 1, "mf": 2, "ff": 3}, "layer_key": "layer",
            "map": {"kind": "pattern", "glob": "*.wav", "regex": r"(?P<note>[A-G]#?\d)v(?P<dynamic>\d+)\.wav", "note": {"group": "note"},
                "layers": ["1", "2", "3"], "dynamic_format": "v{}", "order": "path"}});
        let mut r = Rule::read(&src, &["1".into(), "2".into(), "3".into()]);
        assert_eq!(r.layers[1].name, json!("v2"));
        assert_eq!(r.layers[1].used_as, Some(Dyn::Mf));
        assert_eq!(r.apply(&src), src);
        r.set_keep(0, false);
        assert_eq!(r.apply(&src)["dynamics"], json!({"mf": 1, "ff": 2}), "the numbers follow the kept layers");
    }

    #[test]
    fn an_sfz_rule_keeps_its_other_keys() {
        let src = json!({"id": "s", "dynamics": {"mf": "2/3"}, "map": {"kind": "sfz", "files": ["a.sfz"], "folders": ["Close"],
            "prefix": "x/", "layers": "rank"}});
        let mut r = Rule::read(&src, &["1/3".into(), "2/3".into(), "3/3".into()]);
        assert!(r.layers.iter().all(|l| l.keep && l.raw.is_none()));
        assert_eq!(r.apply(&src), src);
        if let Reading::Sfz(s) = &mut r.reading {
            s.folders.clear();
            s.prefer = Some("/Close/".into());
        }
        let out = r.apply(&src);
        assert!(out["map"].get("folders").is_none());
        assert_eq!(out["map"]["prefer_substring"], json!("/Close/"));
        assert_eq!(out["map"]["prefix"], json!("x/"), "a key the rule does not model stays");
    }

    #[test]
    fn a_custom_map_is_left_alone() {
        let src = json!({"id": "s", "dynamics": {"mf": "mf"}, "map": {"kind": "pattern", "glob": "*", "regex": "a|b",
            "note": {"group": "note"}, "dynamic": "mf"}});
        let r = Rule::read(&src, &[]);
        assert_eq!(r.reading, Reading::Custom);
        assert_eq!(r.apply(&src), src);
    }

    #[test]
    fn set_role_on_a_fixed_part_and_back() {
        let src = json!({"id": "s", "dynamics": {"mf": "mf"}, "map": {"kind": "pattern", "glob": "*.wav",
            "regex": r"(?P<note>[A-G]#?\d)_loud\.wav", "note": {"group": "note"}, "dynamic": "mf", "order": "note"}});
        let mut r = Rule::read(&src, &[]);
        let i = names(&r).parts.iter().position(|p| p.text == "loud").unwrap();
        let values: BTreeSet<String> = ["loud", "soft"].map(String::from).into();
        if let Reading::Names(n) = &mut r.reading {
            n.set_role(i, PartRole::Dynamic, &values);
        }
        assert_eq!(names(&r).regex(), r"(?P<note>[A-G]#?\d)_(?P<dynamic>[A-Za-z]+)\.wav");
        r.sync_layers(&src["map"], &["loud".into(), "soft".into()]);
        let out = r.apply(&src);
        assert_eq!(out["map"]["layers"], json!(["loud", "soft"]));
        assert!(out["map"].get("dynamic").is_none());
        assert_eq!(out["dynamics"], json!({"mf": "loud", "ff": "soft"}), "proposed: two layers are mf and ff");
        if let Reading::Names(n) = &mut r.reading {
            n.set_role(i, PartRole::Fixed, &BTreeSet::new());
        }
        r.sync_layers(&src["map"], &[]);
        let out = r.apply(&src);
        assert!(out["map"].get("layers").is_none());
        assert_eq!(out["map"]["dynamic"], json!("mf"));
        assert_eq!(out["dynamics"], json!({"mf": "mf"}));
    }

    #[test]
    fn one_note_part_and_the_octave_digit_joins_a_note_name() {
        let mut n = Names { glob: "*.wav".into(), relpath: false, parts: parts_of(r"C4_(?P<midi>\d+)\.wav", &json!({"group": "midi", "style": "midi"})).unwrap(),
            shift: 0, offset: 0, prefer: vec![], order: None, group: None };
        n.set_role(0, PartRole::NoteName, &BTreeSet::new());
        assert_eq!(n.parts[0].text, "C4");
        assert_eq!(n.regex(), r"(?P<note>[A-Ga-g][#bs]?\d)_\d+\.wav", "the midi part is no longer the note: any digits");
        assert_eq!(n.note_spec(), Some(json!({"group": "note"})));
    }

    #[test]
    fn take_and_group_roles_follow_prefer_and_the_group() {
        let mut n = Names { glob: "*.wav".into(), relpath: false, parts: parts_of(r"(?P<note>[A-G]\d)_hard_rr1\.wav", &json!({"group": "note"})).unwrap(),
            shift: 0, offset: 0, prefer: vec![], order: None, group: None };
        let take = n.parts.iter().position(|p| p.text == "1").unwrap();
        n.set_role(take, PartRole::Take, &BTreeSet::new());
        assert_eq!(n.prefer, ["take"]);
        let g = n.parts.iter().position(|p| p.text == "hard").unwrap();
        n.set_role(g, PartRole::Group, &["hard", "soft"].map(String::from).into());
        assert_eq!(n.group, Some(vec!["hard".to_string(), "soft".to_string()]));
        let r = Rule { reading: Reading::Names(n), layers: vec![], levels: None, by_layer: false, other_dynamics: vec![] };
        assert_eq!(r.group_values().unwrap().len(), 2);
        assert_eq!(names(&r.for_group("soft")).regex(), r"(?P<note>[A-G]\d)_soft_rr(?P<take>\d+)\.wav");
    }

    #[test]
    fn dynamic_shapes() {
        let s = |v: &[&str]| default_pattern(&PartRole::Dynamic, "", &v.iter().map(|x| x.to_string()).collect());
        assert_eq!(s(&["vl1", "vl2"]), r"(?P<dynamic>vl\d+)");
        assert_eq!(s(&["pp", "ff"]), "(?P<dynamic>[A-Za-z]+)");
        assert_eq!(s(&["1", "2"]), r"(?P<dynamic>\d+)");
        assert_eq!(s(&["p1x", "f"]), "(?P<dynamic>p1x|f)");
    }

    fn touch(root: &std::path::Path, rels: &[&str]) {
        for r in rels {
            let p = root.join(r);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
    }

    #[test]
    fn layer_stats_count_every_value_and_rank_the_levels() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["a/X_C4_vl1.wav", "a/X_C4_vl2.wav", "a/X_D4_vl1.wav", "a/X_D4_vl3.wav"]);
        let root = d.path().to_string_lossy().to_string();
        let src = json!({"map": {"kind": "pattern", "glob": "a/*.wav", "regex": r"X_(?P<note>[A-G]\d)_(?P<dynamic>vl\d)\.wav",
            "note": {"group": "note"}, "layers": ["vl1"]}});
        let mut m = Measures::default();
        for (f, l) in [("a/X_C4_vl1.wav", -20.0), ("a/X_D4_vl1.wav", -22.0), ("a/X_C4_vl2.wav", -10.0)] {
            m.ok.insert(format!("{root}/{f}"), crate::audio::Measure { onset_s: 0.0, level_db: l, nyquist: 24000.0, peaks: vec![], decays: vec![], env_db: vec![], bands_db: vec![] });
        }
        let s = layer_stats(&src, &root, &m);
        let v: Vec<(&str, usize)> = s.iter().map(|x| (x.value.as_str(), x.files)).collect();
        assert_eq!(v, [("vl1", 2), ("vl2", 1), ("vl3", 1)], "every value, also those the layers leave out");
        assert_eq!(s[1].level_db, Some(0.0), "the loudest value is the reference");
        assert_eq!(s[0].level_db, Some(-11.0), "the median of -20 and -22 dB (-21) re -10 dB");
        assert_eq!(s[2].level_db, None);
    }

    #[test]
    fn the_preview_reads_each_file_into_its_parts() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["a/X_C4_vl1.wav", "a/X_D4_vl2.wav", "a/readme.wav"]);
        let root = d.path().to_string_lossy().to_string();
        let src = json!({"map": {"kind": "pattern", "glob": "a/*.wav", "regex": r"X_(?P<note>[A-G]\d)_(?P<dynamic>vl\d)\.wav",
            "note": {"group": "note"}, "dynamic": "mf"}});
        let Reading::Names(mut n) = Rule::read(&src, &[]).reading else { panic!() };
        let p = preview(&n, &root).unwrap();
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.other, ["a/readme.wav"]);
        let note = n.parts.iter().position(|x| x.role == PartRole::NoteName).unwrap();
        assert_eq!(p.values[note], ["C4", "D4"].map(String::from).into());
        n.fill_texts(&p);
        assert_eq!(n.parts[note].text, "C4");
    }

    #[test]
    fn sfz_folders_count_the_regions_by_folder() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::write(root.join("a.sfz"), "<region> sample=Close/C4.wav pitch_keycenter=60\n\
            <region> sample=Room/C4.wav pitch_keycenter=60\n<region> sample=Close/D4.wav pitch_keycenter=62\n").unwrap();
        let src = json!({"map": {"kind": "sfz", "files": ["a.sfz"], "folders": ["Close"]}});
        let f = sfz_folders(&src, &root.to_string_lossy());
        assert_eq!(f, [("Close".to_string(), 2), ("Room".to_string(), 1)], "counted before the map's folders choose");
    }

    #[test]
    fn skip_files_round_trip() {
        // the source's left-out takes are not the rule's: read and applied back, they stay as written
        let src = json!({"id": "s", "root": "r1", "levels": "recorded", "dynamics": {"mf": "mf"},
            "skip_files": ["b.wav", "a.wav"],
            "map": {"kind": "pattern", "glob": "a/*.wav", "regex": "x", "note": {"group": "note"}, "dynamic": "mf"}});
        assert_eq!(Rule::read(&src, &[]).apply(&src), src);
    }

    #[test]
    fn a_stash_restores_a_way_of_reading_until_the_folder_changes() {
        let names = json!({"id": "s", "root": "r1", "levels": "recorded", "dynamics": {"mf": "mf"},
            "map": {"kind": "pattern", "glob": "a/*.wav", "regex": "x", "note": {"group": "note"}, "dynamic": "mf"}});
        let sfz = json!({"id": "s", "root": "r1", "levels": "normalized", "dynamics": {"mf": "1/1"},
            "map": {"kind": "sfz", "files": ["a.sfz"]}});
        let mut st = Stash::default();
        st.put(&names);
        assert_eq!(st.restore(&sfz, None), Some(names.clone()), "back to the names: their keys again");
        assert_eq!(st.restore(&sfz, Some("b.sfz")), None, "a way never left: suggested instead");
        st.folder_changed();
        assert_eq!(st.restore(&sfz, None), None, "another folder: the old folder's way is not brought back");
    }

    #[test]
    fn a_group_part_matches_every_value_until_a_source_is_made() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["a/Bar_cord_C4.wav", "a/Bar_yarn_C4.wav"]);
        let root = d.path().to_string_lossy().to_string();
        let src = json!({"id": "s", "dynamics": {"mf": "mf"}, "map": {"kind": "pattern", "glob": "a/*.wav",
            "regex": r"Bar_cord_(?P<note>[A-G]\d)\.wav", "note": {"group": "note"}, "dynamic": "mf", "order": "note"}});
        let s = crate::suggest::Suggestion { source: src, findings: vec![], groups: ["cord", "yarn"].iter()
            .map(|v| crate::suggest::Group { value: v.to_string(), map: json!({}) }).collect() };
        let mut r = from_suggestion(&s, &[]);
        let p = preview(names(&r), &root).unwrap();
        assert_eq!(p.rows.len(), 2, "the preview reads both groups' files");
        let g = names(&r).parts.iter().position(|x| x.role == PartRole::Group).unwrap();
        if let Reading::Names(n) = &mut r.reading {
            n.set_role(g, PartRole::Fixed, &p.values[g]);
            n.set_role(g, PartRole::Group, &p.values[g]);
        }
        assert_eq!(r.group_values(), Some(vec!["cord".to_string(), "yarn".to_string()]), "a role changed and back keeps both values");
        assert_eq!(names(&r.for_group("yarn")).regex(), r"Bar_yarn_(?P<note>[A-G]\d)\.wav");
    }

    #[test]
    fn the_last_dynamic_can_be_cleared() {
        let src = json!({"id": "s", "dynamics": {"mf": "loud"}, "map": {"kind": "pattern", "glob": "*.wav",
            "regex": r"(?P<note>[A-G]#?\d)_(?P<dynamic>[a-z]+)\.wav", "note": {"group": "note"}, "layers": ["loud", "soft"], "order": "note"}});
        let found = ["loud".to_string(), "soft".to_string()];
        let mut r = Rule::read(&src, &found);
        let i = r.layers.iter().position(|l| l.used_as.is_some()).unwrap();
        r.use_as(i, None);
        r.sync_layers(&src["map"], &found);
        assert!(r.layers.iter().all(|l| l.used_as.is_none()), "the same layers: nothing proposed again");
        r.sync_layers(&src["map"], &["loud".into(), "soft".into(), "mid".into()]);
        assert!(r.layers.iter().any(|l| l.used_as.is_some()), "another layer found: proposed");
    }

    #[test]
    fn an_sfz_sets_layers_come_softest_first() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        // the first key has only the loud layer: the items show it first
        std::fs::write(root.join("a.sfz"), "<region> sample=C4_v2_.wav pitch_keycenter=60 lovel=64 hivel=127\n\
            <region> sample=D4_v1_.wav pitch_keycenter=62 lovel=1 hivel=63\n\
            <region> sample=D4_v2_.wav pitch_keycenter=62 lovel=64 hivel=127\n").unwrap();
        touch(root, &["C4_v2_.wav", "D4_v1_.wav", "D4_v2_.wav"]);
        let src = json!({"map": {"kind": "sfz", "files": ["a.sfz"], "layers": "rank",
            "dynamic_from": {"regex": r"_v(\d+)_", "format": "v{}"}}});
        let s = layer_stats(&src, &root.to_string_lossy(), &Measures::default());
        let v: Vec<&str> = s.iter().map(|x| x.value.as_str()).collect();
        assert_eq!(v, ["v1", "v2"]);
    }

    #[test]
    fn a_rule_that_reads_nothing_still_applies() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["a/X_loud_C4.wav", "a/X_loud_D4.wav"]);
        let root = d.path().to_string_lossy().to_string();
        let src = json!({"id": "s", "dynamics": {"mf": "mf"}, "map": {"kind": "pattern", "glob": "a/*.wav",
            "regex": r"X_loud_(?P<note>[A-G]\d)\.wav", "note": {"group": "note"}, "dynamic": "mf", "order": "note"}});
        let mut r = Rule::read(&src, &[]);
        let i = names(&r).parts.iter().position(|p| p.text == "loud").unwrap();
        if let Reading::Names(n) = &mut r.reading {
            // a part the same in every file made the dynamic, then letters made the note name
            n.set_role(i, PartRole::Dynamic, &["loud".to_string()].into());
            n.set_role(i, PartRole::NoteName, &BTreeSet::new());
        }
        let out = r.apply(&src);
        let found: Vec<String> = layer_stats(&out, &root, &Measures::default()).into_iter().map(|s| s.value).collect();
        r.sync_layers(&out["map"], &found);
        let out = r.apply(&src);
        let rep = crate::mapping::items(&out, &root).unwrap();
        assert!(rep.items.is_empty());
        assert_eq!(rep.unmatched.len(), 2, "the mapper says which files it rejects");
        assert!(out["dynamics"].is_object(), "the one layer stays the mf");
    }
}
