//! The projects' files (projects/<name>/project.toml) as the editor reads and writes them:
//! through toml_edit, so comments, key order and formatting outside an edited key survive.
use serde_json::Value as Json;
use std::fmt;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Value};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use toml_edit::TableLike;

/// Where the repository's parts are: the projects ($PE_PROJECTS overrides it, as for the
/// Python side: the tests' own) and the sample sets.
#[derive(Clone, Debug)]
pub struct Repo {
    pub root: PathBuf,
    pub projects: PathBuf,
    pub data: PathBuf,
}

impl Repo {
    pub fn new(root: PathBuf) -> Repo {
        let projects = std::env::var_os("PE_PROJECTS").map(PathBuf::from).unwrap_or_else(|| root.join("projects"));
        let data = root.join("data");
        Repo { root, projects, data }
    }

    /// $PE_REPO, else the directory above this crate
    pub fn from_env() -> Repo {
        let root = std::env::var_os("PE_REPO")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf());
        Repo::new(root.canonicalize().unwrap_or(root))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "line {l}, column {c}: {}", self.message),
            _ => write!(f, "{}", self.message),
        }
    }
}

#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    /// the file changed on disk since it was loaded or saved
    Conflict,
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "{e}"),
            SaveError::Conflict => write!(f, "the file changed on disk"),
        }
    }
}

/// One project file, as loaded; `saved` is the text on disk at the last load or save.
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    doc: DocumentMut,
    saved: String,
}

pub fn parse(text: &str) -> Result<DocumentMut, LoadError> {
    text.parse::<DocumentMut>().map_err(|e| {
        let (line, column) = match e.span() {
            Some(span) => {
                let before = &text[..span.start.min(text.len())];
                let line = before.matches('\n').count() + 1;
                let column = before.len() - before.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
                (Some(line), Some(column))
            }
            None => (None, None),
        };
        LoadError { message: e.message().to_string(), line, column }
    })
}

impl Project {
    pub fn load(repo: &Repo, name: &str) -> Result<Project, LoadError> {
        let path = repo.projects.join(name).join("project.toml");
        let saved = std::fs::read_to_string(&path)
            .map_err(|e| LoadError { message: format!("{}: {e}", path.display()), line: None, column: None })?;
        let doc = parse(&saved)?;
        Ok(Project { name: name.to_string(), path, doc, saved })
    }

    pub fn text(&self) -> String {
        self.doc.to_string()
    }

    /// edits not saved yet
    pub fn dirty(&self) -> bool {
        self.text() != self.saved
    }

    pub fn save(&mut self) -> Result<(), SaveError> {
        if self.disk() == Disk::Changed {
            return Err(SaveError::Conflict);
        }
        self.save_over()
    }

    /// the whole file as JSON (what Python's tomllib gives: the mapper's input)
    pub fn to_json(&self) -> Json {
        toml_to_json(self.doc.as_item())
    }
}

/// the text written to a temporary file next to `path`, renamed into place
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.tmp{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// the projects' names (directories with a project.toml), sorted
pub fn project_names(repo: &Repo) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(&repo.projects)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("project.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

pub fn toml_to_json(item: &Item) -> Json {
    match item {
        Item::None => Json::Null,
        Item::Value(v) => value_to_json(v),
        Item::Table(t) => Json::Object(
            t.iter().filter(|(_, i)| !i.is_none()).map(|(k, i)| (k.to_string(), toml_to_json(i))).collect(),
        ),
        Item::ArrayOfTables(a) => Json::Array(a.iter().map(|t| toml_to_json(&Item::Table(t.clone()))).collect()),
    }
}

pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::String(s) => Json::String(s.value().clone()),
        Value::Integer(i) => Json::from(*i.value()),
        Value::Float(f) => Json::from(*f.value()),
        Value::Boolean(b) => Json::Bool(*b.value()),
        Value::Datetime(d) => Json::String(d.value().to_string()),
        Value::Array(a) => Json::Array(a.iter().map(value_to_json).collect()),
        Value::InlineTable(t) => Json::Object(t.iter().map(|(k, v)| (k.to_string(), value_to_json(v))).collect()),
    }
}

/// a JSON value as a TOML value (objects as inline tables; null has no TOML form: an empty string)
pub fn json_to_toml(v: &Json) -> Value {
    match v {
        Json::Null => Value::from(""),
        Json::Bool(b) => Value::from(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::from(i),
            None => Value::from(n.as_f64().unwrap_or(0.0)),
        },
        Json::String(s) => Value::from(s.as_str()),
        Json::Array(a) => Value::Array(a.iter().map(json_to_toml).collect()),
        Json::Object(o) => {
            let mut t = toml_edit::InlineTable::new();
            for (k, v) in o {
                t.insert(k, json_to_toml(v));
            }
            Value::InlineTable(t)
        }
    }
}

/// the Python interpreter for the pipeline's scripts ($PE_PYTHON, else python3)
pub fn python() -> String {
    std::env::var("PE_PYTHON").unwrap_or_else(|_| "python3".into())
}

/// every source of every project (project name, source table as JSON), in file order
pub fn all_sources(repo: &Repo) -> Vec<(String, Json)> {
    let mut out = Vec::new();
    for name in project_names(repo) {
        let Ok(p) = Project::load(repo, &name) else { continue };
        let j = p.to_json();
        for part in j.get("part").and_then(Json::as_array).into_iter().flatten() {
            for s in part.get("source").and_then(Json::as_array).into_iter().flatten() {
                out.push((name.clone(), s.clone()));
            }
        }
    }
    out
}

/// the roles a source can have (the ones analysis/ reads)
pub const ROLES: [&str; 5] = ["main", "mallet", "damped", "check", "buff"];

/// a table the editor edits: a top-level section, a source (by id) or a source's map
#[derive(Clone, Debug, PartialEq)]
pub enum Table {
    Top(String),
    Source(String),
    Map(String),
}

impl Table {
    pub fn top(name: &str) -> Table {
        Table::Top(name.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Disk {
    Same,
    Changed,
    Missing,
}

static HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(\[\[|\[)\s*([A-Za-z0-9_.\- ]+?)\s*(\]\]|\])\s*(#.*)?$").unwrap());
static ID_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^\s*id\s*=\s*"([^"]*)""#).unwrap());
static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9_]+$").unwrap());
static DATA_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]*$").unwrap());

/// a header line: (array of tables, its dotted name)
fn header(line: &str) -> Option<(bool, String)> {
    let c = HEADER.captures(line)?;
    Some((&c[1] == "[[", c[2].replace(' ', "")))
}

/// where a header's block starts: the comment lines directly above it belong to it
fn with_comments(lines: &[&str], i: usize) -> usize {
    let mut s = i;
    while s > 0 && lines[s - 1].trim_start().starts_with('#') {
        s -= 1;
    }
    s
}

/// a block of lines [start, end)
#[derive(Clone, Copy, Debug)]
struct Block {
    part: usize,
    start: usize,
    end: usize,
}

/// the blocks of the headers `is_head` picks, each up to the next header `belongs` does not keep
fn blocks(lines: &[&str], is_head: impl Fn(bool, &str) -> bool, belongs: impl Fn(bool, &str) -> bool) -> Vec<Block> {
    let heads: Vec<(usize, bool, String)> =
        lines.iter().enumerate().filter_map(|(i, l)| header(l).map(|(a, n)| (i, a, n))).collect();
    let mut out = Vec::new();
    let mut part = 0usize;
    let mut parts_seen = 0usize;
    for (k, (i, arr, name)) in heads.iter().enumerate() {
        if *arr && name == "part" {
            parts_seen += 1;
            part = parts_seen - 1;
        }
        if !is_head(*arr, name) {
            continue;
        }
        let next = heads[k + 1..].iter().find(|(_, a, n)| !belongs(*a, n)).map(|(j, _, _)| with_comments(lines, *j));
        out.push(Block { part, start: with_comments(lines, *i), end: next.unwrap_or(lines.len()) });
    }
    out
}

fn source_blocks(lines: &[&str]) -> Vec<Block> {
    blocks(lines, |a, n| a && n == "part.source", |a, n| !a && n.starts_with("part.source."))
}

fn part_blocks(lines: &[&str]) -> Vec<Block> {
    blocks(lines, |a, n| a && n == "part", |a, n| !(a && n == "part") && (n.starts_with("part.")))
}

/// the lines without the blank ones at the end
fn trim_blank<'a>(r: &[&'a str]) -> Vec<&'a str> {
    let mut v = r.to_vec();
    while v.last().is_some_and(|l| l.trim().is_empty()) {
        v.pop();
    }
    v
}

fn block_id(lines: &[&str], b: &Block) -> Option<String> {
    lines[b.start..b.end].iter().find_map(|l| ID_LINE.captures(l).map(|c| c[1].to_string()))
}

fn json_get<'a>(j: &'a Json, k: &str) -> Option<&'a Json> {
    j.get(k)
}

impl Project {
    fn replace_text(&mut self, text: String) -> Result<(), String> {
        self.doc = parse(&text).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn table_like(&self, t: &Table) -> Option<&dyn TableLike> {
        match t {
            Table::Top(name) => self.doc.get(name)?.as_table_like(),
            Table::Source(id) => self.source_table(id).map(|t| t as &dyn TableLike),
            Table::Map(id) => self.source_table(id)?.get("map")?.as_table_like(),
        }
    }

    fn source_table(&self, id: &str) -> Option<&toml_edit::Table> {
        let parts = self.doc.get("part")?.as_array_of_tables()?;
        parts.iter().flat_map(|p| p.get("source").and_then(Item::as_array_of_tables).into_iter().flat_map(|a| a.iter()))
            .find(|s| s.get("id").and_then(Item::as_str) == Some(id))
    }

    fn table_like_mut(&mut self, t: &Table) -> Result<&mut dyn TableLike, String> {
        match t {
            Table::Top(name) => {
                if !self.doc.contains_key(name) {
                    self.doc.insert(name, toml_edit::table());
                }
                self.doc[name.as_str()].as_table_like_mut().ok_or_else(|| format!("{name} is not a table"))
            }
            Table::Source(id) | Table::Map(id) => {
                let parts = self.doc.get_mut("part").and_then(Item::as_array_of_tables_mut).ok_or("no [[part]]")?;
                let src = parts.iter_mut()
                    .flat_map(|p| p.get_mut("source").and_then(Item::as_array_of_tables_mut).into_iter().flat_map(|a| a.iter_mut()))
                    .find(|s| s.get("id").and_then(Item::as_str) == Some(id.as_str()))
                    .ok_or_else(|| format!("no source {id}"))?;
                if let Table::Map(_) = t {
                    if !src.contains_key("map") {
                        src.insert("map", toml_edit::table());
                    }
                    src["map"].as_table_like_mut().ok_or_else(|| "the map is not a table".to_string())
                } else {
                    Ok(src as &mut dyn TableLike)
                }
            }
        }
    }

    pub fn get(&self, t: &Table, key: &str) -> Option<Json> {
        self.table_like(t)?.get(key).map(toml_to_json).filter(|j| !j.is_null())
    }

    /// (key, the value as TOML text without its comment) of a table's values, in file order
    pub fn keys(&self, t: &Table) -> Vec<(String, String)> {
        let Some(tl) = self.table_like(t) else { return Vec::new() };
        tl.iter()
            .filter_map(|(k, item)| {
                let mut v = item.as_value()?.clone();
                v.decor_mut().clear();
                Some((k.to_string(), v.to_string()))
            })
            .collect()
    }

    /// a key's value; an existing key keeps its comment and spacing
    pub fn set(&mut self, t: &Table, key: &str, value: Value) -> Result<(), String> {
        let tl = self.table_like_mut(t)?;
        match tl.get_mut(key) {
            Some(Item::Value(old)) => {
                let decor = old.decor().clone();
                *old = value;
                *old.decor_mut() = decor;
            }
            Some(item) => *item = Item::Value(value),
            None => {
                tl.insert(key, Item::Value(value));
            }
        }
        Ok(())
    }

    /// a key's value with a comment line of its own above it (the old key and its comment replaced)
    pub fn set_commented(&mut self, t: &Table, key: &str, value: Value, comment: &str) -> Result<(), String> {
        let tl = self.table_like_mut(t)?;
        tl.remove(key);
        tl.insert(key, Item::Value(value));
        if let Some(mut k) = tl.key_mut(key) {
            k.leaf_decor_mut().set_prefix(format!("# {comment}\n"));
        }
        Ok(())
    }

    /// a top-level table removed whole (none: nothing to do)
    pub fn remove_table(&mut self, name: &str) {
        self.doc.remove(name);
    }

    pub fn remove(&mut self, t: &Table, key: &str) -> Result<(), String> {
        self.table_like_mut(t)?.remove(key);
        Ok(())
    }

    /// what stops the chain before it runs: an instrument's source without dynamics or levels, a
    /// string project's part without a source of its
    /// name (the string fits address a part and its main source by one id: fit_pluck.py's
    /// --profile {part}; every committed string project names them alike)
    pub fn build_problems(&self) -> Vec<String> {
        let j = self.to_json();
        let parts: Vec<&Json> = j.get("part").and_then(Json::as_array).into_iter().flatten().collect();
        let mut out = Vec::new();
        // an instrument's sources (a project with a body exports one) carry dynamics and levels
        if j["project"].get("body").is_some() {
            for s in parts.iter().flat_map(|p| p.get("source").and_then(Json::as_array).into_iter().flatten()) {
                let missing: Vec<&str> = ["dynamics", "levels"].into_iter().filter(|k| s.get(*k).is_none()).collect();
                if !missing.is_empty() {
                    out.push(format!("the source {} has no {} (the fits read them: Suggest proposes both)",
                        s["id"].as_str().unwrap_or_default(), missing.join(" and ")));
                }
            }
        }
        if j.get("strings").is_none() {
            return out;
        }
        out.extend(parts.into_iter().filter_map(|part| {
            let id = part["id"].as_str()?;
            let sources: Vec<&str> = part.get("source").and_then(Json::as_array).into_iter().flatten()
                .filter_map(|s| s["id"].as_str()).collect();
            (!sources.is_empty() && !sources.contains(&id)).then(|| format!(
                "the part {id} has no source named {id}: a string project's fits take the part's id for its main source \
                 (rename the main source to {id})"))
        }));
        out
    }

    pub fn part_count(&self) -> usize {
        self.doc.get("part").and_then(Item::as_array_of_tables).map(|a| a.len()).unwrap_or(0)
    }

    pub fn source_ids(&self) -> Vec<String> {
        let j = self.to_json();
        json_get(&j, "part").and_then(Json::as_array).into_iter().flatten()
            .flat_map(|p| p.get("source").and_then(Json::as_array).into_iter().flatten())
            .filter_map(|s| s["id"].as_str().map(String::from))
            .collect()
    }

    pub fn source_json(&self, id: &str) -> Option<Json> {
        self.source_table(id).map(|t| toml_to_json(&Item::Table(t.clone())))
    }

    /// a new source at the end of the first part: id, role, root and its map
    pub fn add_source(&mut self, repo: &Repo, id: &str, role: &str, root: &str, map: &Json) -> Result<(), String> {
        check_source_id(repo, self, id)?;
        let text = self.text();
        let lines: Vec<&str> = text.lines().collect();
        let part = part_blocks(&lines).first().copied().ok_or("the project has no [[part]]")?;
        let q = |s: &str| Value::from(s).to_string().trim().to_string();
        let mut block = format!("\n[[part.source]]\nid = {}\nrole = {}\nroot = {}\n\n[part.source.map]\n", q(id), q(role), q(root));
        for (k, v) in map.as_object().ok_or("the map is not a table")? {
            block.push_str(&format!("{k} = {}\n", json_to_toml(v).to_string().trim()));
        }
        let mut end = part.end;
        while end > part.start && lines[end - 1].trim().is_empty() {
            end -= 1;
        }
        let mut out: Vec<String> = lines[..end].iter().map(|s| s.to_string()).collect();
        out.extend(block.lines().map(String::from));
        if part.end < lines.len() {
            out.push(String::new());
            out.extend(lines[part.end..].iter().map(|s| s.to_string()));
        }
        self.replace_text(out.join("\n") + "\n")
    }

    /// a source's id changed (the new one checked as a new source's: ids are global)
    pub fn rename_source(&mut self, repo: &Repo, id: &str, new: &str) -> Result<(), String> {
        if self.source_table(id).is_none() {
            return Err(format!("no source {id}"));
        }
        check_source_id(repo, self, new)?;
        self.set(&Table::Source(id.into()), "id", Value::from(new))
    }

    /// a source's keys and its map's set to `new` (the source as JSON): changed values set,
    /// keys `new` lacks removed, equal values left as they are (formatting and comments kept);
    /// the id stays. Returns whether anything changed.
    /// a source's sub-table (`[part.source.<name>]`, after its map) set from a JSON object, a
    /// comment line above it
    pub fn set_source_table(&mut self, id: &str, name: &str, table: &Json, comment: &str) -> Result<(), String> {
        let obj = table.as_object().ok_or("not a table")?;
        let mut t = toml_edit::Table::new();
        for (k, v) in obj {
            t.insert(k, Item::Value(json_to_toml(v)));
        }
        t.decor_mut().set_prefix(format!("\n# {comment}\n"));
        let src = self.table_like_mut(&Table::Source(id.into()))?;
        // after the map: a table's place in the text is its position
        let after = src.get("map").and_then(Item::as_table).and_then(toml_edit::Table::position);
        if let Some(pos) = after {
            t.set_position(Some(pos + 1));
        }
        src.insert(name, Item::Table(t));
        Ok(())
    }

    /// a key of a source removed (a sub-table too)
    pub fn remove_source_key(&mut self, id: &str, name: &str) -> Result<(), String> {
        self.table_like_mut(&Table::Source(id.into()))?.remove(name);
        Ok(())
    }

    pub fn update_source(&mut self, id: &str, new: &Json) -> Result<bool, String> {
        let old = self.source_json(id).ok_or_else(|| format!("no source {id}"))?;
        let mut changed = false;
        let tables = [(Table::Source(id.into()), &old, new), (Table::Map(id.into()), &old["map"], &new["map"])];
        for (t, o, n) in tables {
            let o = o.as_object().cloned().unwrap_or_default();
            let n = n.as_object().ok_or("the source or its map is not a table")?;
            for (k, v) in n {
                if k != "id" && k != "map" && o.get(k) != Some(v) {
                    self.set(&t, k, json_to_toml(v))?;
                    changed = true;
                }
            }
            for k in o.keys() {
                if k != "id" && k != "map" && !n.contains_key(k) {
                    self.remove(&t, k)?;
                    changed = true;
                }
            }
        }
        Ok(changed)
    }

    pub fn remove_source(&mut self, id: &str) -> Result<(), String> {
        let text = self.text();
        let lines: Vec<&str> = text.lines().collect();
        let b = source_blocks(&lines).into_iter().find(|b| block_id(&lines, b).as_deref() == Some(id))
            .ok_or_else(|| format!("no source {id}"))?;
        let mut out: Vec<&str> = lines[..b.start].to_vec();
        out.extend_from_slice(&lines[b.end..]);
        self.replace_text(out.join("\n") + "\n")
    }

    /// a source one place up or down within its part
    pub fn move_source(&mut self, id: &str, up: bool) -> Result<(), String> {
        let text = self.text();
        let lines: Vec<&str> = text.lines().collect();
        let bs = source_blocks(&lines);
        let i = bs.iter().position(|b| block_id(&lines, b).as_deref() == Some(id)).ok_or_else(|| format!("no source {id}"))?;
        let j = if up { i.checked_sub(1) } else { Some(i + 1).filter(|&j| j < bs.len()) };
        let Some(j) = j.filter(|&j| bs[j].part == bs[i].part) else { return Ok(()) };
        let (a, b) = if i < j { (bs[i], bs[j]) } else { (bs[j], bs[i]) };
        let first = trim_blank(&lines[a.start..b.start]);
        let second = trim_blank(&lines[b.start..b.end]);
        let tail_blank = lines[b.start..b.end].iter().rev().take_while(|l| l.trim().is_empty()).count();
        let mut out: Vec<&str> = lines[..a.start].to_vec();
        out.extend(second);
        out.push("");
        out.extend(first);
        out.extend(std::iter::repeat_n("", tail_blank));
        out.extend_from_slice(&lines[b.end..]);
        self.replace_text(out.join("\n") + "\n")
    }

    /// a top-level section as written (its comments and subtables included)
    pub fn section_text(&self, name: &str) -> Option<String> {
        let text = self.text();
        let lines: Vec<&str> = text.lines().collect();
        let prefix = format!("{name}.");
        let b = blocks(&lines, |a, n| !a && n == name, |a, n| !a && n.starts_with(&prefix)).into_iter().next()?;
        Some(lines[b.start..b.end].join("\n").trim_end().to_string())
    }

    pub fn disk(&self) -> Disk {
        match std::fs::read_to_string(&self.path) {
            Ok(t) if t == self.saved => Disk::Same,
            Ok(_) => Disk::Changed,
            Err(_) => Disk::Missing,
        }
    }

    /// the file as it is on disk now (unsaved edits dropped)
    pub fn reload(&mut self) -> Result<(), LoadError> {
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| LoadError { message: e.to_string(), line: None, column: None })?;
        self.doc = parse(&text)?;
        self.saved = text;
        Ok(())
    }

    /// the edits written over a file changed on disk (the user chose to keep theirs)
    pub fn save_over(&mut self) -> Result<(), SaveError> {
        let text = self.text();
        write_atomic(&self.path, &text).map_err(SaveError::Io)?;
        self.saved = text;
        Ok(())
    }
}

/// every id in use (project names, part ids, source ids) → the project it belongs to
pub fn ids(repo: &Repo) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for name in project_names(repo) {
        out.insert(name.clone(), name.clone());
        let Ok(p) = Project::load(repo, &name) else { continue };
        let j = p.to_json();
        for part in j.get("part").and_then(Json::as_array).into_iter().flatten() {
            if let Some(id) = part["id"].as_str() {
                out.insert(id.to_string(), name.clone());
            }
            for s in part.get("source").and_then(Json::as_array).into_iter().flatten() {
                if let Some(id) = s["id"].as_str() {
                    out.insert(id.to_string(), name.clone());
                }
            }
        }
    }
    out
}

/// a new project's name: lower case, digits and underscores; no id in use anywhere
pub fn check_project_name(repo: &Repo, name: &str) -> Result<(), String> {
    check_project_name_in(&ids(repo), name)
}

/// check_project_name against the ids read once (project::ids)
pub fn check_project_name_in(ids: &HashMap<String, String>, name: &str) -> Result<(), String> {
    if !NAME.is_match(name) {
        return Err(format!("{name:?}: use lower case letters, digits and underscores"));
    }
    if let Some(owner) = ids.get(name) {
        return Err(format!("{name:?} is in use (project {owner})"));
    }
    Ok(())
}

/// a new source's id: as a project name; ids are global (analysis/project.py), so no other
/// project's id and no other source of this one
pub fn check_source_id(repo: &Repo, project: &Project, id: &str) -> Result<(), String> {
    check_source_id_in(&ids(repo), Some(project), id)
}

/// check_source_id against the ids read once; `project` None: a project not written yet
pub fn check_source_id_in(ids: &HashMap<String, String>, project: Option<&Project>, id: &str) -> Result<(), String> {
    if !NAME.is_match(id) {
        return Err(format!("{id:?}: use lower case letters, digits and underscores"));
    }
    if project.is_some_and(|p| p.source_ids().iter().any(|s| s == id)) {
        return Err(format!("{id:?} is a source of this project already"));
    }
    match ids.get(id) {
        Some(owner) if project.is_none_or(|p| owner != &p.name) => Err(format!("{id:?} is in use (project {owner})")),
        _ => Ok(()),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub family: String,
    pub name: String,
    pub menu_name: String,
    /// the [menu] group ("" without one)
    pub group: String,
}

/// the projects a new one can start from: those with one part (by family, then name)
pub fn templates(repo: &Repo) -> Vec<Template> {
    let mut out: Vec<Template> = project_names(repo)
        .into_iter()
        .filter_map(|n| {
            let p = Project::load(repo, &n).ok()?;
            if p.part_count() != 1 {
                return None;
            }
            let family = p.get(&Table::top("project"), "family")?.as_str()?.to_string();
            let menu_name = p.get(&Table::top("menu"), "name").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| n.clone());
            let group = p.get(&Table::top("menu"), "group").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            Some(Template { family, name: n, menu_name, group })
        })
        .collect();
    out.sort_by(|a, b| (&a.family, &a.name).cmp(&(&b.family, &b.name)));
    out
}

/// an answer to the wizard's "How is it played?" (projects/kinds.toml)
#[derive(Clone, Debug, PartialEq)]
pub struct Kind {
    pub id: String,
    pub label: String,
    /// the donor on a tie
    pub fallback: String,
    /// the committed instruments a new one of this kind starts from (those present)
    pub donors: Vec<String>,
    /// pre-selected when the recordings' overtones are a bar's
    pub bar_like: bool,
}

/// the answers in the file's order: the donors whose project is missing left out, an answer with
/// none left out (a fallback missing: the first donor)
pub fn kinds(repo: &Repo) -> Vec<Kind> {
    let Ok(text) = std::fs::read_to_string(repo.projects.join("kinds.toml")) else { return Vec::new() };
    let Ok(doc) = text.parse::<DocumentMut>() else { return Vec::new() };
    let names = project_names(repo);
    let s = |t: &toml_edit::Table, k: &str| t.get(k).and_then(Item::as_str).unwrap_or_default().to_string();
    let mut out = Vec::new();
    for t in doc.get("kind").and_then(Item::as_array_of_tables).into_iter().flatten() {
        let donors: Vec<String> = t.get("donors").and_then(Item::as_array).into_iter().flatten()
            .filter_map(|v| v.as_str().map(String::from)).filter(|d| names.contains(d)).collect();
        let Some(first) = donors.first().cloned() else { continue };
        let fallback = Some(s(t, "fallback")).filter(|f| donors.contains(f)).unwrap_or(first);
        out.push(Kind { id: s(t, "id"), label: s(t, "label"), fallback, donors,
            bar_like: t.get("bar_like").and_then(Item::as_bool).unwrap_or(false) });
    }
    out
}

/// a project's [project] family
pub fn family(repo: &Repo, project: &str) -> Option<String> {
    Project::load(repo, project).ok()?.get(&Table::top("project"), "family")?.as_str().map(String::from)
}

/// the menu groups the projects use, sorted
pub fn menu_groups(repo: &Repo) -> Vec<String> {
    let set: std::collections::BTreeSet<String> = project_names(repo).into_iter()
        .filter_map(|n| Project::load(repo, &n).ok()?.get(&Table::top("menu"), "group")?.as_str().map(String::from))
        .collect();
    set.into_iter().collect()
}

/// (project, family) of every project with a family
pub fn families(repo: &Repo) -> Vec<(String, String)> {
    project_names(repo).into_iter()
        .filter_map(|n| {
            let f = Project::load(repo, &n).ok()?.get(&Table::top("project"), "family")?.as_str()?.to_string();
            Some((n, f))
        })
        .collect()
}

/// a template's first source (its role and mallet guide a new source's)
pub fn template_source(repo: &Repo, template: &str) -> Option<Json> {
    let p = Project::load(repo, template).ok()?;
    p.source_json(p.source_ids().first()?)
}

/// a family's chain of steps (projects/steps.toml), each step once ("polarizations#2" is polarizations)
pub fn chain(repo: &Repo, family: &str) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(repo.projects.join("steps.toml")) else { return Vec::new() };
    let Ok(doc) = text.parse::<DocumentMut>() else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for s in doc.get("chain").and_then(|c| c.get(family)).and_then(Item::as_array).into_iter().flatten() {
        if let Some(name) = s.as_str().map(|x| x.split('#').next().unwrap_or(x).to_string()) {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// [project] skip after a step of the chain is ticked (`on`) or unticked: the chain's steps in its
/// order, then the entries it does not list (kept as they are)
pub fn skip_toggled(skip: &[String], chain: &[String], step: &str, on: bool) -> Vec<String> {
    let mut out: Vec<String> = chain.iter().filter(|c| if *c == step { on } else { skip.contains(c) }).cloned().collect();
    out.extend(skip.iter().filter(|s| !chain.contains(s)).cloned());
    out
}

/// new_from_donor without a reason
pub fn new_from_template(repo: &Repo, name: &str, template: &str) -> Result<Project, String> {
    new_from_donor(repo, name, template, "")
}

/// a new project: the donor's file with its comments, its first line saying which and why, the
/// menu's name set, one part of the new name without sources (no ledger, fits, cache or
/// `[license]` copied)
pub fn new_from_donor(repo: &Repo, name: &str, template: &str, why: &str) -> Result<Project, String> {
    check_project_name(repo, name)?;
    let t = Project::load(repo, template).map_err(|e| e.to_string())?;
    if t.part_count() != 1 {
        return Err(format!("{template} has {} parts: not a template", t.part_count()));
    }
    let text = t.text();
    let lines: Vec<&str> = text.lines().collect();
    let parts = part_blocks(&lines);
    let mut kept: Vec<&str> = Vec::new();
    let mut at = 0;
    for b in &parts {
        kept.extend_from_slice(&lines[at..b.start]);
        at = b.end;
    }
    kept.extend_from_slice(&lines[at..]);
    let body = kept.join("\n");
    let because = if why.is_empty() { String::new() } else { format!(" ({why})") };
    let text = format!(
        "# {name}: made from {template}'s project file{because} (the comments are {template}'s)\n{}\n\n[[part]]\nid = \"{name}\"\n",
        body.trim_end()
    );
    let dir = repo.projects.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("project.toml");
    write_atomic(&path, &text).map_err(|e| e.to_string())?;
    let mut p = Project::load(repo, name).map_err(|e| e.to_string())?;
    p.set(&Table::top("menu"), "name", Value::from(name))?;
    // the donor's credit names the donor's recordings, not the new set's
    p.remove_table("license");
    p.save().map_err(|e| e.to_string())?;
    Ok(p)
}

/// a typed folder: relative to data/ (as it is) or absolute (under data/: made relative; elsewhere:
/// None). A path through a set linked into data/ is under data/: it is compared as written first,
/// then with its links resolved.
pub fn data_relative(data: &Path, typed: &str) -> Option<String> {
    let p = Path::new(typed.trim());
    if p.is_relative() {
        return Some(typed.trim().trim_matches('/').to_string());
    }
    let written = std::path::absolute(p).ok()?;
    for d in [std::path::absolute(data).ok(), data.canonicalize().ok()].into_iter().flatten() {
        if let Ok(r) = written.strip_prefix(&d) {
            return Some(r.to_string_lossy().trim_matches('/').to_string());
        }
    }
    let d = data.canonicalize().ok()?;
    let c = p.canonicalize().ok()?;
    c.strip_prefix(&d).ok().map(|r| r.to_string_lossy().into_owned())
}

/// a folder outside data/ made available as data/<name> (a symlink), with a stub entry in
/// data/SOURCES.md for its origin and license
pub fn link_folder(repo: &Repo, folder: &Path, name: &str) -> Result<PathBuf, String> {
    if !DATA_NAME.is_match(name) {
        return Err(format!("{name:?}: use letters, digits, '.', '_' and '-'"));
    }
    let dest = repo.data.join(name);
    if dest.symlink_metadata().is_ok() {
        return Err(format!("data/{name} exists"));
    }
    let src = folder.canonicalize().map_err(|e| format!("{}: {e}", folder.display()))?;
    let abs = std::path::absolute(folder).map_err(|e| e.to_string())?;
    let under = |p: &Path| p.starts_with(&repo.data) || repo.data.canonicalize().is_ok_and(|d| p.starts_with(d));
    if under(&abs) || under(&src) {
        return Err(format!("{} is under data/ already", folder.display()));
    }
    std::os::unix::fs::symlink(&src, &dest).map_err(|e| e.to_string())?;
    let sources = repo.data.join("SOURCES.md");
    let old = std::fs::read_to_string(&sources).unwrap_or_default();
    write_atomic(&sources, &format!("{old}{}", link_stub(name, &src))).map_err(|e| e.to_string())?;
    Ok(dest)
}

fn link_stub(name: &str, target: &Path) -> String {
    format!(
        "\n## {name}\n\n- Location: `{name}/` (a link to `{}`)\n- Source: (where the set comes from: the URL)\n- License: (the set's license)\n",
        target.display()
    )
}

/// a link link_folder made, undone: the link and its stub in data/SOURCES.md (when unedited)
/// removed; the folder it points to stays
pub fn unlink_folder(repo: &Repo, name: &str) -> Result<(), String> {
    let dest = repo.data.join(name);
    let target = std::fs::read_link(&dest).map_err(|_| format!("data/{name} is not a link"))?;
    let sources = repo.data.join("SOURCES.md");
    if let Ok(text) = std::fs::read_to_string(&sources) {
        let stub = link_stub(name, &target);
        if text.contains(&stub) {
            write_atomic(&sources, &text.replacen(&stub, "", 1)).map_err(|e| e.to_string())?;
        }
    }
    std::fs::remove_file(&dest).map_err(|e| e.to_string())
}
