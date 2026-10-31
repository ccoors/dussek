//! SFZ files as analysis/mapping.py's parse_sfz reads them: #include and #define expanded, the
//! regions as opcode tables, each region over its group over the global scope, their keys as
//! `keys` gives them; other headers' opcodes are not kept (but <control>'s key offsets).
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

pub type Region = HashMap<String, String>;

static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"//[^\n]*").unwrap());
static HEADER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<\w+>").unwrap());
static KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\w+)=").unwrap());
static NEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+\w+=").unwrap());

pub fn parse(text: &str) -> Vec<Region> {
    // Python's text mode: \r\n and \r read as \n
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = COMMENT.replace_all(&text, "");
    let mut chunks: Vec<(bool, &str)> = Vec::new();
    let mut last = 0;
    for m in HEADER.find_iter(&text) {
        chunks.push((false, &text[last..m.start()]));
        chunks.push((true, m.as_str()));
        last = m.end();
    }
    chunks.push((false, &text[last..]));

    let (mut control, mut global, mut group) = (Region::new(), Region::new(), Region::new());
    let mut regions: Vec<Region> = Vec::new();
    let mut cur: Option<&str> = None;
    for (is_header, tok) in chunks {
        if is_header {
            let name = &tok[1..tok.len() - 1];
            cur = Some(name);
            if name == "group" {
                group = Region::new();
            } else if name == "region" {
                let mut r = global.clone();
                r.extend(group.clone());
                regions.push(r);
            }
            continue;
        }
        for (k, v) in opcodes(tok.trim()) {
            match cur {
                Some("region") => {
                    regions.last_mut().unwrap().insert(k, v);
                }
                Some("global") => {
                    global.insert(k, v);
                }
                Some("group") => {
                    group.insert(k, v);
                }
                Some("control") => {
                    control.insert(k, v);
                }
                _ => {}
            }
        }
    }
    let num = |k: &str| control.get(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
    let offset = 12 * num("octave_offset") + num("note_offset");
    regions.into_iter().map(|r| keys(r, offset)).collect()
}

const KEY_OPCODES: [&str; 3] = ["pitch_keycenter", "lokey", "hikey"];

/// a region's keys: key= stands for lokey, hikey and pitch_keycenter (where they are not
/// given); a key by note name (c4 = 60) as its number; <control>'s octave_offset and
/// note_offset move each key given as a number
fn keys(mut r: Region, offset: i64) -> Region {
    if let Some(k) = r.get("key").cloned() {
        for o in KEY_OPCODES {
            r.entry(o.to_string()).or_insert_with(|| k.clone());
        }
    }
    for o in KEY_OPCODES.iter().chain(&["key"]) {
        if let Some(n) = r.get(*o).and_then(|v| crate::pyfmt::note_name(v.trim()).ok()) {
            r.insert(o.to_string(), n.to_string());
        }
    }
    if offset != 0 {
        for o in KEY_OPCODES.iter().chain(&["key"]) {
            if let Some(n) = r.get(*o).and_then(|v| v.trim().parse::<i64>().ok()) {
                r.insert(o.to_string(), (n + offset).to_string());
            }
        }
    }
    r
}

/// key=value pairs: a value (a sample path) may hold spaces; it ends where whitespace and the
/// next `key=` begin (Python: `(\w+)=(.*?)(?=\s+\w+=|\s*$)`, the value stripped)
fn opcodes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(c) = KEY.captures_at(s, pos) {
        let start = c.get(0).unwrap().end();
        let end = NEXT.find_at(s, start).map(|m| m.start()).unwrap_or(s.len());
        out.push((c[1].to_string(), s[start..end].trim().to_string()));
        pos = end;
        if pos >= s.len() {
            break;
        }
    }
    out
}

static DIRECTIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"#include\s+"([^"]*)"|#define\s+(\$\w+)\s+(\S+)"#).unwrap());

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n").replace('\r', "\n");
    Ok(COMMENT.replace_all(&text, "").into_owned())
}

/// each $NAME a #define named replaced by its value, the longest name first ($EXT before $EX)
fn substitute(text: &str, defines: &[(String, String)]) -> String {
    let mut names: Vec<&(String, String)> = defines.iter().collect();
    names.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    let mut out = text.to_string();
    for (k, v) in names {
        out = out.replace(k.as_str(), v);
    }
    out
}

/// an SFZ file's text without comments, its #include files in their places (relative to the top
/// file's folder) and each $NAME a #define names replaced by its value in what follows
/// (analysis/mapping.py's expand_sfz)
pub fn expand(path: &Path) -> Result<String, String> {
    fn go(top: &Path, path: &Path, text: &str, defines: &mut Vec<(String, String)>, depth: usize) -> Result<String, String> {
        let mut out = String::new();
        let mut at = 0;
        for c in DIRECTIVE.captures_iter(text) {
            let m = c.get(0).unwrap();
            out.push_str(&substitute(&text[at..m.start()], defines));
            at = m.end();
            if let Some(inc) = c.get(1) {
                let file = top.join(inc.as_str().replace('\\', "/"));
                if depth > 16 || !file.is_file() {
                    return Err(format!("{}: #include {:?}: {}", path.display(), inc.as_str(),
                        if depth > 16 { "too deep" } else { "no such file" }));
                }
                out.push('\n');
                out.push_str(&go(top, path, &read_text(&file)?, defines, depth + 1)?);
                out.push('\n');
            } else {
                let (k, v) = (c[2].to_string(), c[3].to_string());
                defines.retain(|(x, _)| *x != k);
                defines.push((k, v));
            }
        }
        out.push_str(&substitute(&text[at..], defines));
        Ok(out)
    }
    let top = path.parent().unwrap_or(Path::new("."));
    go(top, path, &read_text(path)?, &mut Vec::new(), 0)
}

pub fn parse_file(path: &Path) -> Result<Vec<Region>, String> {
    Ok(parse(&expand(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_inherit_and_values_keep_spaces() {
        let text = "// a comment <region> sample=no.wav\r\n<control> default_path=x/\n<global> volume=-3 trigger=attack\n\
                    <group> lovel=1 hivel=64\n<region> sample=Sustains/C4 v1.wav pitch_keycenter=60\n\
                    <region> sample=a b.wav lokey=61 volume=2\n<group>\n<region>sample=c.wav\n";
        let r = parse(text);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0]["sample"], "Sustains/C4 v1.wav");
        assert_eq!(r[0]["volume"], "-3");
        assert_eq!(r[0]["lovel"], "1");
        assert_eq!(r[1]["sample"], "a b.wav");
        assert_eq!(r[1]["volume"], "2");
        assert!(!r[2].contains_key("lovel"), "a new <group> resets the group scope");
        assert_eq!(r[2]["trigger"], "attack");
        assert!(!r[0].contains_key("default_path"), "<control> opcodes are not kept");
    }

    #[test]
    fn a_value_ends_at_the_next_opcode() {
        let r = parse("<region> sample=x=y.wav  lokey=60\thivel=127");
        assert_eq!(r[0]["sample"], "x=y.wav");
        assert_eq!(r[0]["lokey"], "60");
        assert_eq!(r[0]["hivel"], "127");
    }

    #[test]
    fn a_lone_carriage_return_ends_a_comment() {
        let r = parse("<region> // note\rsample=a.wav");
        assert_eq!(r[0]["sample"], "a.wav");
    }

    #[test]
    fn key_and_the_control_offsets() {
        // key= stands for lokey, hikey and pitch_keycenter; <control>'s octave_offset and
        // note_offset move every key that follows (a set named by its keys two octaves low)
        let text = "<control>\ndefault_path=s/\noctave_offset=2\nnote_offset=-1\n<group> lovel=1 hivel=64\n\
                    <region> sample=24 p.wav key=24\n<group> lovel=65 hivel=127\n<region> sample=24 f.wav key=24\n\
                    <region> sample=30 f.wav pitch_keycenter=30 lokey=29 hikey=31\n";
        let keys: Vec<(String, String, String)> = parse(text).iter()
            .map(|r| (r["pitch_keycenter"].clone(), r["lokey"].clone(), r["hikey"].clone())).collect();
        let want: Vec<(String, String, String)> = [("47", "47", "47"), ("47", "47", "47"), ("53", "52", "54")]
            .iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect();
        assert_eq!(keys, want);
    }

    #[test]
    fn keys_by_note_name() {
        // an SFZ may name its keys (c4 = 60); the offsets move them as numbers
        let r = parse("<control> note_offset=1\n<region> sample=a.wav lokey=a0 hikey=a#0 pitch_keycenter=a0\n<region> sample=b.wav key=c4\n");
        let keys: Vec<(&str, &str, &str)> = r.iter().map(|x| (x["pitch_keycenter"].as_str(), x["lokey"].as_str(), x["hikey"].as_str())).collect();
        assert_eq!(keys, [("22", "22", "23"), ("61", "61", "61")]);
    }

    #[test]
    fn includes_and_defines() {
        // #include pulls in a file (relative to the top file's folder, also from an included one);
        // #define names a value $NAME stands for in what follows (Ethan Winer's sets)
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        std::fs::create_dir_all(p.join("Data")).unwrap();
        std::fs::write(p.join("Data/ext.txt"), "#define $EXT flac").unwrap();
        std::fs::write(p.join("Data/regions.txt"), "<region> key=60 sample=G a.$EXT\n#include \"Data/more.txt\"\n").unwrap();
        std::fs::write(p.join("Data/more.txt"), "<region> key=62 sample=G b.$EXT // a comment\n").unwrap();
        std::fs::write(p.join("set.sfz"), "// #include \"Data/none.txt\"\n#include \"Data/ext.txt\"\n<control>\ndefault_path=s/\n\
            <global> volume=-6\n#include \"Data/regions.txt\"\n").unwrap();
        let r = parse_file(&p.join("set.sfz")).unwrap();
        let got: Vec<(&str, &str, &str)> = r.iter().map(|x| (x["sample"].as_str(), x["pitch_keycenter"].as_str(), x["volume"].as_str())).collect();
        assert_eq!(got, [("G a.flac", "60", "-6"), ("G b.flac", "62", "-6")]);
        std::fs::write(p.join("bad.sfz"), "#include \"Data/none.txt\"\n").unwrap();
        assert!(parse_file(&p.join("bad.sfz")).is_err_and(|e| e.contains("none.txt")));
    }
}
