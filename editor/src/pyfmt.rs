//! The Python behaviors analysis/mapping.py relies on, so the port gives the same items:
//! note names, os.path's normpath/join/basename, str.format with "{}" and "{layer}", and
//! Python's str/int/float of a value.
use regex::Regex;
use serde_json::Value as Json;
use std::sync::LazyLock;

/// 'A0', 'D#1', 'Bb0', 'Cs4' (a sharp spelled s), 'g3' → MIDI (C4 = 60)
pub fn note_name(text: &str) -> Result<i64, String> {
    static RX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Ga-g])(#|s|b)?(-?[0-9])$").unwrap());
    let c = RX.captures(text).ok_or_else(|| format!("not a note name: {text:?}"))?;
    let base = match c[1].to_ascii_uppercase().as_str() {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        _ => 11,
    };
    let acc = match c.get(2).map(|m| m.as_str()) {
        Some("#") | Some("s") => 1,
        Some("b") => -1,
        _ => 0,
    };
    let octave: i64 = c[3].parse().map_err(|_| format!("not a note name: {text:?}"))?;
    Ok(base + acc + 12 * (octave + 1))
}

/// os.path.normpath (POSIX)
pub fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".into();
    }
    let initial = if path.starts_with("//") && !path.starts_with("///") {
        "//"
    } else if path.starts_with('/') {
        "/"
    } else {
        ""
    };
    let mut comps: Vec<&str> = Vec::new();
    for c in path.split('/') {
        if c.is_empty() || c == "." {
            continue;
        }
        if c != ".." || (initial.is_empty() && comps.is_empty()) || comps.last() == Some(&"..") {
            comps.push(c);
        } else if !comps.is_empty() {
            comps.pop();
        }
    }
    let s = format!("{initial}{}", comps.join("/"));
    if s.is_empty() { ".".into() } else { s }
}

/// os.path.join(a, b)
pub fn join(a: &str, b: &str) -> String {
    if b.starts_with('/') || a.is_empty() {
        b.to_string()
    } else if a.ends_with('/') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

pub fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// str.format with the two forms the maps use: "{}" (one positional value) and "{layer}"
pub fn format(fmt: &str, positional: Option<&str>, layer: Option<i64>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = fmt;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let r = &rest[i..];
        if let Some(after) = r.strip_prefix("{}") {
            out.push_str(positional.ok_or_else(|| format!("format {fmt:?}: {{}} without a value"))?);
            rest = after;
        } else if let Some(after) = r.strip_prefix("{layer}") {
            out.push_str(&layer.ok_or_else(|| format!("format {fmt:?}: {{layer}} without a layer"))?.to_string());
            rest = after;
        } else {
            return Err(format!("format {fmt:?}: only {{}} and {{layer}} are supported"));
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// Python's str() of a value
pub fn py_str(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Bool(b) => if *b { "True" } else { "False" }.into(),
        Json::Null => "None".into(),
        Json::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap();
            if f.fract() == 0.0 && f.abs() < 1e16 { format!("{f:.1}") } else { format!("{f}") }
        }
        other => other.to_string(),
    }
}

/// Python's int() of a value (a string of an integer, an integer, a float truncated)
pub fn py_int(v: &Json) -> Result<i64, String> {
    match v {
        Json::String(s) => s.trim().parse().map_err(|_| format!("int({s:?})")),
        Json::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)).ok_or_else(|| format!("int({n})")),
        Json::Bool(b) => Ok(*b as i64),
        other => Err(format!("int({other})")),
    }
}

/// Python's float() of a value
pub fn py_float(v: &Json) -> Result<f64, String> {
    match v {
        Json::String(s) => s.trim().parse().map_err(|_| format!("float({s:?})")),
        Json::Number(n) => n.as_f64().ok_or_else(|| format!("float({n})")),
        Json::Bool(b) => Ok(*b as i64 as f64),
        other => Err(format!("float({other})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn note_names_as_python_reads_them() {
        let got: Vec<i64> = ["A0", "C4", "D#1", "Bb0", "Cs4", "g3"].iter().map(|n| note_name(n).unwrap()).collect();
        assert_eq!(got, [21, 60, 27, 22, 61, 55]);
        assert!(note_name("H4").is_err());
        assert!(note_name("C10").is_err());
    }

    #[test]
    fn normpath_as_posix() {
        for (a, b) in [("a/./b", "a/b"), ("a//b/", "a/b"), ("a/../b", "b"), ("../a", "../a"), ("a/../../b", "../b"),
                       ("", "."), ("/a/../..", "/"), ("//a", "//a"), ("///a", "/a"), ("./", ".")] {
            assert_eq!(normpath(a), b, "{a}");
        }
    }

    #[test]
    fn join_and_basename() {
        assert_eq!(join("/r", "a/b.wav"), "/r/a/b.wav");
        assert_eq!(join("/r/", "a"), "/r/a");
        assert_eq!(join("/r", "/abs"), "/abs");
        assert_eq!(join("", "a"), "a");
        assert_eq!(basename("x/y/z.wav"), "z.wav");
        assert_eq!(basename("z.wav"), "z.wav");
    }

    #[test]
    fn format_supports_the_maps_strings_only() {
        assert_eq!(format("v{}", Some("3"), None).unwrap(), "v3");
        assert_eq!(format("vl{layer}", None, Some(2)).unwrap(), "vl2");
        assert_eq!(format("plain", Some("x"), None).unwrap(), "plain");
        assert!(format("{0}", Some("x"), None).is_err());
        assert!(format("{}", None, Some(1)).is_err());
    }

    #[test]
    fn values_as_python_converts_them() {
        assert_eq!(py_str(&json!("attack")), "attack");
        assert_eq!(py_str(&json!(0)), "0");
        assert_eq!(py_str(&json!(0.0)), "0.0");
        assert_eq!(py_int(&json!(" 64 ")).unwrap(), 64);
        assert_eq!(py_int(&json!(7)).unwrap(), 7);
        assert!(py_int(&json!("6.5")).is_err());
        assert_eq!(py_float(&json!("-3.5")).unwrap(), -3.5);
        assert_eq!(py_float(&json!(64)).unwrap(), 64.0);
    }
}
