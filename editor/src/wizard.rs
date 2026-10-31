//! The wizard for a new instrument (or new sources in an open project): its answers in memory,
//! its steps and what keeps Next from going on, and Finish: the project and its sources written
//! at once (one source per value of a group part), nothing before.
use crate::project::{self, Project, ROLES, Repo, Table};
use crate::rule::Rule;
use crate::suggest::{Finding, Folder, Suggestion};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use toml_edit::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Folder,
    Reading,
    Names,
    Dynamics,
    Played,
    Check,
    Menu,
}

impl Step {
    pub fn title(self) -> &'static str {
        match self {
            Step::Folder => "Folder",
            Step::Reading => "How to read the files",
            Step::Names => "The file names",
            Step::Dynamics => "Dynamics",
            Step::Played => "How is it played?",
            Step::Check => "Check and listen",
            Step::Menu => "Name and menu",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Wizard {
    /// Some(project): new sources for an open project (the steps from the folder to the check)
    pub add_to: Option<String>,
    pub template: String,
    pub name: String,
    /// the source's id (the first one's, when a group makes several)
    pub id: String,
    pub role: String,
    pub mallet: Option<String>,
    pub folder: Option<Folder>,
    pub sfz_files: Vec<String>,
    /// the SFZ file read (None: the file names)
    pub sfz: Option<String>,
    /// the suggestion's source (role, root, levels, dynamics, release, map) the rule applies onto
    pub source: Json,
    pub rule: Option<Rule>,
    pub findings: Vec<Finding>,
    pub menu_name: String,
    pub menu_group: String,
    pub build_now: bool,
    /// the group value the pages show (None: the first)
    pub shown: Option<String>,
    /// each group value's own skip and relabel (set on the check page)
    pub own: BTreeMap<String, Json>,
    /// why the last suggestion failed (no rule then)
    pub suggest_error: Option<String>,
    /// the answer to "How is it played?" (a projects/kinds.toml id; "" none yet)
    pub kind: String,
    /// why the donor (`template`) was chosen, for the new file's first line
    pub donor_why: String,
    /// the take checks' thresholds (an added source: the project's), written at Finish
    pub thresholds: crate::takes::Thresholds,
}

/// where the wizard keeps its measurements before the project exists (copied in at Finish)
pub fn measures_cache() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("pianoengine-editor/audio.json")
}

impl Wizard {
    pub fn new_instrument() -> Wizard {
        Wizard { role: ROLES[0].into(), build_now: true, ..Default::default() }
    }

    /// new sources for `p`: the id proposed is the project's name (as an id: lower case, digits,
    /// underscores), else the first free name_N (none free among a thousand: left to the user)
    pub fn add_to(repo: &Repo, p: &Project) -> Wizard {
        let base = slug(&p.name).trim_matches('_').to_string();
        let base = if base.is_empty() { "source".to_string() } else { base };
        let id = std::iter::once(base.clone()).chain((2..1000).map(|n| format!("{base}_{n}")))
            .find(|id| project::check_source_id(repo, p, id).is_ok())
            .unwrap_or_default();
        let role = p.source_ids().first().and_then(|s| p.source_json(s)).and_then(|s| s["role"].as_str().map(String::from));
        let thresholds = crate::takes::Thresholds::from_project(p.to_json().get("take_checks"));
        Wizard { add_to: Some(p.name.clone()), id, role: role.unwrap_or_else(|| ROLES[0].into()), thresholds, ..Default::default() }
    }

    /// the template chosen: the new source's role and mallet name follow its first source's, the
    /// menu's group the template's
    pub fn set_template(&mut self, repo: &Repo, template: &str) {
        self.template = template.into();
        self.menu_group = Project::load(repo, template).ok()
            .and_then(|p| p.get(&Table::top("menu"), "group")?.as_str().map(String::from))
            .unwrap_or_default();
        let src = project::template_source(repo, template);
        self.role = src.as_ref().and_then(|s| s["role"].as_str().map(String::from)).unwrap_or_else(|| ROLES[0].into());
        self.mallet = src.as_ref().and_then(|s| s["mallet"].as_str().map(String::from));
    }

    /// an answer and its donor: the new source's role and mallet follow the donor's first source
    /// (a single mallet is named "mallet": the donor's name is its own mallet's), the menu's group
    /// the donor's
    pub fn choose(&mut self, repo: &Repo, kind: &str, donor: &str, why: &str) {
        self.kind = kind.into();
        self.donor_why = why.into();
        self.set_template(repo, donor);
        if self.mallet.is_some() {
            self.mallet = Some("mallet".into());
        }
    }

    /// the name typed: the id follows it, and the menu's name while it was the name's (one typed
    /// by hand stays)
    pub fn set_name(&mut self, name: &str) {
        if self.menu_name.is_empty() || self.menu_name == self.name {
            self.menu_name = name.into();
        }
        self.name = name.into();
        self.id = name.into();
    }

    pub fn steps(&self) -> Vec<Step> {
        let mut s = vec![Step::Folder, Step::Reading, Step::Names];
        if self.rule.as_ref().is_none_or(|r| r.layers.len() > 1) {
            s.push(Step::Dynamics);
        }
        if self.add_to.is_none() {
            s.push(Step::Played);
        }
        s.push(Step::Check);
        if self.add_to.is_none() {
            s.push(Step::Menu);
        }
        s
    }

    /// why Next cannot go on from `step` (ids: project::ids, read once; project: the open project
    /// sources are added to; items: how many items the rule reads, when known)
    pub fn blocker(&self, ids: &HashMap<String, String>, project: Option<&Project>, step: Step, items: Option<usize>) -> Option<String> {
        let no_rule = || self.rule.is_none().then(|| match &self.suggest_error {
            Some(e) => format!("the suggestion failed: {e}"),
            None => "the suggestion is still running".to_string(),
        });
        let no_items = || no_rule().or_else(|| (items.unwrap_or(0) == 0).then(|| "the rule reads no files".into()));
        match step {
            Step::Folder if self.folder.is_none() => Some("choose the folder".into()),
            // (a new instrument's id follows its name, typed on the last step)
            Step::Folder if self.add_to.is_some() && self.id.trim().is_empty() => Some("the source needs an id".into()),
            Step::Folder if self.add_to.is_some() => project::check_source_id_in(ids, project, self.id.trim()).err(),
            Step::Folder => None,
            Step::Reading => no_rule(),
            Step::Names => no_items(),
            Step::Played if self.kind.is_empty() || self.template.is_empty() => Some("choose how it is played".into()),
            Step::Played => None,
            Step::Check => no_items().or_else(|| if self.add_to.is_some() { self.check_ids(ids, project).err() } else { None }),
            Step::Dynamics => no_rule().or_else(|| {
                let used = self.rule.as_ref().is_some_and(|r| r.layers.iter().any(|l| l.used_as.is_some()));
                (!used).then(|| "choose which layer is pp, mf or ff (at least one)".into())
            }),
            Step::Menu if self.name.trim().is_empty() => Some("the instrument needs a name".into()),
            Step::Menu => project::check_project_name_in(ids, &self.name).err()
                .or_else(|| self.check_ids(ids, project).err())
                .or_else(|| self.menu_name.trim().is_empty().then(|| "the menu needs a name".into())),
        }
    }

    /// the suggestion's proposal taken: its source, its rule (found: the layer values the files hold)
    pub fn apply_suggestion(&mut self, s: Suggestion, found: &[String]) {
        // new recordings: the answer and its donor were chosen from the old ones
        if self.add_to.is_none() {
            self.kind.clear();
            self.template.clear();
            self.donor_why.clear();
        }
        self.rule = Some(crate::rule::from_suggestion(&s, found));
        self.findings = s.findings.clone();
        self.source = s.source;
        if self.menu_name.is_empty() {
            self.menu_name = self.name.clone();
        }
    }

    /// the ids Finish writes: free, and each once (the new project's sources are not in `ids` yet)
    pub fn check_ids(&self, ids: &HashMap<String, String>, project: Option<&Project>) -> Result<(), String> {
        let srcs = self.sources()?;
        for (k, (id, _)) in srcs.iter().enumerate() {
            project::check_source_id_in(ids, project, id)?;
            if srcs[..k].iter().any(|(o, _)| o == id) {
                return Err(format!("the id {id} twice"));
            }
        }
        Ok(())
    }

    /// the ids of the sources Finish would write that read no file (`root`: the folder's data root)
    pub fn sources_reading_nothing(&self, root: &str) -> Vec<String> {
        self.sources().unwrap_or_default().into_iter()
            .filter(|(_, src)| !crate::mapping::items(src, root).is_ok_and(|r| !r.items.is_empty()))
            .map(|(id, _)| id)
            .collect()
    }

    /// the group value the pages show: the one chosen, else the first (None: no group part)
    pub fn shown_value(&self) -> Option<String> {
        let values = self.rule.as_ref()?.group_values()?;
        self.shown.clone().filter(|s| values.contains(s)).or_else(|| values.first().cloned())
    }

    /// the source for a group value (None: no group part): the rule's, with the value's own keys
    fn source_for(&self, rule: &Rule, value: Option<&String>) -> Json {
        let mut src = match value {
            Some(v) => rule.for_group(v).apply(&self.source),
            None => rule.apply(&self.source),
        };
        if let Some(own) = value.and_then(|v| self.own.get(v)).and_then(Json::as_object) {
            for (k, x) in own {
                src[k.as_str()] = x.clone();
            }
        }
        src
    }

    /// the source as the pages see it now (the shown group value's, with a group part)
    pub fn current(&self) -> Option<Json> {
        Some(self.source_for(self.rule.as_ref()?, self.shown_value().as_ref()))
    }

    /// the keys the check page sets: the shown group value's own, else the source's
    fn checked_mut(&mut self) -> &mut Json {
        match self.shown_value() {
            Some(v) => self.own.entry(v).or_insert_with(|| json!({})),
            None => &mut self.source,
        }
    }

    /// a key left out (the check page's Skip)
    pub fn skip(&mut self, note: i64) {
        let c = self.checked_mut();
        let mut skip: Vec<i64> = c["skip"].as_array().into_iter().flatten().filter_map(Json::as_i64).collect();
        skip.push(note);
        skip.sort();
        skip.dedup();
        c["skip"] = json!(skip);
    }

    /// a take left out (the check page's "Leave this take out" and "Use the spare take")
    pub fn skip_file(&mut self, relpath: &str) {
        let c = self.checked_mut();
        let mut v: Vec<String> = c["skip_files"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
        if !v.iter().any(|x| x == relpath) {
            v.push(relpath.to_string());
        }
        c["skip_files"] = json!(v);
    }

    /// a left-out take read again (the check page's "Put this take back"); the key goes with its last file
    pub fn unskip_file(&mut self, relpath: &str) {
        let c = self.checked_mut();
        let v: Vec<String> = c["skip_files"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from))
            .filter(|x| x != relpath).collect();
        if v.is_empty() {
            if let Some(o) = c.as_object_mut() {
                o.remove("skip_files");
            }
        } else {
            c["skip_files"] = json!(v);
        }
    }

    /// a file's key changed (the check page's Relabel)
    pub fn relabel(&mut self, from: i64, to: i64) {
        let c = self.checked_mut();
        if !c["relabel"].is_object() {
            c["relabel"] = json!({});
        }
        c["relabel"][from.to_string()] = json!(to);
    }

    /// the sources Finish writes: one per group value (the first takes the id, the others
    /// id_value), with the role and the mallet name
    pub fn sources(&self) -> Result<Vec<(String, Json)>, String> {
        let rule = self.rule.as_ref().ok_or("no rule yet")?;
        let values: Vec<Option<String>> = match rule.group_values() {
            Some(v) => v.into_iter().map(Some).collect(),
            None => vec![None],
        };
        let mut out = Vec::new();
        for (i, v) in values.iter().enumerate() {
            let mut src = self.source_for(rule, v.as_ref());
            src["role"] = json!(self.role);
            match (v, &self.mallet) {
                (Some(v), _) if self.role == ROLES[1] => src["mallet"] = json!(v.to_lowercase()),
                (None, Some(m)) if self.role == ROLES[1] => src["mallet"] = json!(m),
                _ => {}
            }
            let id = match (i, v) {
                (0, _) | (_, None) => self.id.trim().to_string(),
                (_, Some(v)) => format!("{}_{}", self.id.trim(), slug(v)),
            };
            src["id"] = json!(id);
            out.push((id, src));
        }
        Ok(out)
    }
}

fn slug(v: &str) -> String {
    v.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// the sources into a project: every id checked first (none written when one is taken)
fn add_sources(p: &mut Project, repo: &Repo, srcs: &[(String, Json)]) -> Result<Vec<String>, String> {
    for (k, (id, _)) in srcs.iter().enumerate() {
        project::check_source_id(repo, p, id)?;
        if srcs[..k].iter().any(|(o, _)| o == id) {
            return Err(format!("the id {id} twice"));
        }
    }
    for (id, src) in srcs {
        let root = src["root"].as_str().ok_or("the source has no root")?;
        p.add_source(repo, id, src["role"].as_str().unwrap_or(ROLES[0]), root, &src["map"])?;
        p.update_source(id, src)?;
    }
    Ok(srcs.iter().map(|(id, _)| id.clone()).collect())
}

/// a mallet's name in the menu: its source's mallet, capitalized
fn menu_mallet(src: &Json) -> String {
    let m = src["mallet"].as_str().or(src["id"].as_str()).unwrap_or_default();
    let mut c = m.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// the menu's mallet names once sources are added: one per mallet source, none for a single
/// mallet (the plugin lists them); the names listed already stay (they may be edited by hand)
/// and the added sources' follow
fn update_mallet_names(p: &mut Project, added: &[String]) -> Result<(), String> {
    let menu = Table::top("menu");
    let all: Vec<Json> = p.source_ids().iter().filter_map(|id| p.source_json(id)).filter(|s| s["role"] == ROLES[1]).collect();
    if all.len() < 2 {
        return p.remove(&menu, "mallets");
    }
    let is_new = |s: &Json| added.iter().any(|id| s["id"] == json!(id));
    let old: Vec<&Json> = all.iter().filter(|s| !is_new(s)).collect();
    let listed: Vec<String> = p.get(&menu, "mallets").and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter().filter_map(|v| v.as_str().map(String::from)).collect();
    let mut names = if listed.len() == old.len() { listed } else { old.iter().map(|s| menu_mallet(s)).collect() };
    names.extend(all.iter().filter(|s| is_new(s)).map(menu_mallet));
    p.set(&menu, "mallets", project::json_to_toml(&json!(names)))
}

/// the donor a role is borrowed from: the project the donor borrows it from, else the donor
fn lender(p: &Project, donor: &str, role: &str) -> Json {
    p.get(&Table::top("uses"), role).unwrap_or_else(|| json!(donor))
}

/// the values the recordings decide, set in a project made from a donor (its [project], [uses]
/// and [bars] are the donor's until here): bars' tracks = "auto" and the mallet or pp_mirror the
/// recorded dynamics need; strings' release analysis and borrowings by what the set has
pub fn own_values(p: &mut Project, repo: &Repo, donor: &str, srcs: &[(String, Json)]) -> Result<(), String> {
    let proj = Table::top("project");
    let family = p.get(&proj, "family").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    // (no dynamics named: one layer recorded)
    let dynamics = srcs.first().and_then(|(_, s)| s["dynamics"].as_object().map(|o| o.len())).unwrap_or(1).max(1);
    if family == "bars" {
        let bars = Table::top("bars");
        p.set_commented(&bars, "tracks", Value::from("auto"),
            "the modes' tracks derived from the recordings by the bars fit (numbers here pin them by hand)")?;
        p.remove(&bars, "tol")?;
        p.remove(&bars, "tied")?;
        p.remove(&bars, "pp_mirror")?;
        match dynamics {
            1 if p.get(&bars, "mallet").is_none() => {
                let assumed = project::kinds(repo).iter().flat_map(|k| k.donors.clone())
                    .filter_map(|d| Project::load(repo, &d).ok()?.get(&bars, "mallet")).next();
                if let Some(m) = assumed {
                    p.set_commented(&bars, "mallet", project::json_to_toml(&m),
                        "one dynamic recorded: the mallet's speeds and lowpass assumed")?;
                }
            }
            1 => {}
            2 => {
                p.remove(&bars, "mallet")?;
                p.set(&bars, "pp_mirror", Value::from(true))?;
            }
            _ => p.remove(&bars, "mallet")?,
        }
        return Ok(());
    }
    let chain = project::chain(repo, &family);
    let uses = Table::top("uses");
    let mut skip: Vec<String> = p.get(&proj, "skip").and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter().filter_map(|v| v.as_str().map(String::from)).collect();
    let has_release = srcs.iter().any(|(_, s)| s.get("release").is_some());
    let mut skipped = |step: &str, on: bool| {
        skip.retain(|s| s != step);
        if on {
            skip.push(step.into());
        }
    };
    if chain.iter().any(|s| s == "release_noise") {
        skipped("release_noise", !has_release);
        if has_release {
            p.remove(&uses, "release")?;
        } else {
            let l = lender(p, donor, "release");
            p.set(&uses, "release", project::json_to_toml(&l))?;
        }
    }
    for (step, role) in [("dampers", "dampers"), ("pedal_noise", "pedal")] {
        if chain.iter().any(|s| s == step) {
            skipped(step, true);
            let l = lender(p, donor, role);
            p.set(&uses, role, project::json_to_toml(&l))?;
            if role == "pedal" {
                let d = lender(p, donor, "dampers");
                p.set(&uses, "dampers", project::json_to_toml(&d))?;
            }
        }
    }
    // register fits the recorded levels, or a borrowed reference ([uses] levels)
    if chain.iter().any(|s| s == "register") {
        let recorded = srcs.first().is_some_and(|(_, s)| s["levels"] == "recorded");
        skipped("register", !recorded && p.get(&uses, "levels").is_none());
    }
    if family == "struck" && dynamics < 2 {
        let l = lender(p, donor, "felt");
        p.set(&uses, "felt", project::json_to_toml(&l))?;
    } else if family == "struck" {
        p.remove(&uses, "felt")?;
    }
    let ordered = project::skip_toggled(&skip, &chain, "", false);
    if ordered.is_empty() {
        p.remove(&proj, "skip")
    } else {
        p.set(&proj, "skip", project::json_to_toml(&json!(ordered)))
    }
}

/// the new project: from the template, its sources, the menu, saved; the wizard's measurements
/// of the folder's files copied into its cache (a copy that fails costs a measuring only). On an
/// error what Finish wrote is gone (a project directory that was there before stays).
pub fn finish(repo: &Repo, w: &Wizard, measures_cache: &Path) -> Result<Project, String> {
    let srcs = w.sources()?;
    let dir = repo.projects.join(&w.name);
    let existed = dir.exists();
    let mut p = project::new_from_donor(repo, &w.name, &w.template, &w.donor_why)?;
    let result = (|| -> Result<(), String> {
        let ids = add_sources(&mut p, repo, &srcs)?;
        update_mallet_names(&mut p, &ids)?;
        own_values(&mut p, repo, &w.template, &srcs)?;
        crate::takes::write_thresholds(&mut p, &w.thresholds)?;
        p.set(&Table::top("menu"), "name", Value::from(w.menu_name.trim()))?;
        if !w.menu_group.trim().is_empty() {
            p.set(&Table::top("menu"), "group", Value::from(w.menu_group.trim()))?;
        }
        p.save().map_err(|e| e.to_string())
    })();
    if let Err(e) = result {
        if existed {
            let _ = std::fs::remove_file(dir.join("project.toml"));
        } else {
            let _ = std::fs::remove_dir_all(&dir);
        }
        return Err(e);
    }
    if let (true, Some(f)) = (measures_cache.is_file(), &w.folder) {
        let under = Path::new(&crate::suggest::data_root(&repo.data, &f.root)).to_path_buf();
        let _ = crate::audio::copy_entries(measures_cache, &dir.join("cache/editor/audio.json"), &under);
    }
    Ok(p)
}

/// new sources into an open project (not saved: the user saves)
pub fn add_into(p: &mut Project, repo: &Repo, w: &Wizard) -> Result<Vec<String>, String> {
    let ids = add_sources(p, repo, &w.sources()?)?;
    update_mallet_names(p, &ids)?;
    crate::takes::write_thresholds(p, &w.thresholds)?;
    Ok(ids)
}
