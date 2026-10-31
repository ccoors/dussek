//! A rolled piano's recipe: its project file's `[menu]` and `[roll]` (every choice written out; a
//! build draws nothing).
use crate::instrument::Error;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, value};

fn err(msg: impl Into<String>) -> Error {
    Error(msg.into())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// the project's name (its folder)
    pub name: String,
    /// the menu's name and group
    pub menu: (String, String),
    /// the pianos the knobs are measured against
    pub references: Vec<String>,
    /// each key's strings and hammers: these pianos' at these weights (the heaviest is the frame)
    pub strings: Vec<(String, f64)>,
    /// the responses' fine structure above 100–200 Hz: these pianos' mixed
    pub fine_structure: Vec<String>,
    /// + warm, wide, rounded; − cold, clear (the references' standard deviations)
    pub warmth: f64,
    /// + airy, a longer ring; − dry
    pub air: f64,
    /// the further directions
    pub shade: Vec<f64>,
    /// the draw that made this roll (a record)
    pub seed: u64,
}

fn num(item: &Item, what: &str) -> Result<f64, Error> {
    item.as_float().or_else(|| item.as_integer().map(|i| i as f64)).ok_or_else(|| err(format!("[roll] {what}: not a number")))
}

fn strings_of(item: &Item, what: &str) -> Result<Vec<String>, Error> {
    item.as_array().ok_or_else(|| err(format!("[roll] {what}: not a list")))?.iter().map(|v| v.as_str().map(String::from).ok_or_else(|| err(format!("[roll] {what}: not names")))).collect()
}

impl Recipe {
    pub fn parse(text: &str, name: &str) -> Result<Recipe, Error> {
        let doc: DocumentMut = text.parse().map_err(|e| err(format!("{name}: project.toml: {e}")))?;
        let roll = doc.get("roll").ok_or_else(|| err(format!("{name}: no [roll]")))?;
        let menu = doc.get("menu");
        let text_of = |k: &str| menu.and_then(|m| m.get(k)).and_then(Item::as_str).map(String::from);
        let get = |k: &str| roll.get(k).ok_or_else(|| err(format!("{name}: [roll] has no {k}")));
        let strings = get("strings")?
            .as_inline_table()
            .map(|t| t.iter().map(|(k, v)| (k.to_string(), v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).unwrap_or(f64::NAN))).collect::<Vec<_>>())
            .or_else(|| get("strings").ok()?.as_table().map(|t| t.iter().map(|(k, v)| (k.to_string(), num(v, "strings").unwrap_or(f64::NAN))).collect()))
            .ok_or_else(|| err(format!("{name}: [roll] strings: not a table")))?;
        if strings.iter().any(|(_, w)| w.is_nan()) {
            return Err(err(format!("{name}: [roll] strings: a weight is not a number")));
        }
        Ok(Recipe {
            name: name.to_string(),
            menu: (text_of("name").unwrap_or_else(|| name.to_string()), text_of("group").unwrap_or_else(|| "Rolled pianos".to_string())),
            references: strings_of(get("references")?, "references")?,
            strings,
            fine_structure: strings_of(get("fine_structure")?, "fine_structure")?,
            warmth: num(get("warmth")?, "warmth")?,
            air: num(get("air")?, "air")?,
            shade: get("shade")?.as_array().ok_or_else(|| err("[roll] shade: not a list"))?.iter().map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).ok_or_else(|| err("[roll] shade: not numbers"))).collect::<Result<_, _>>()?,
            seed: roll.get("seed").and_then(Item::as_integer).unwrap_or(0) as u64,
        })
    }

    /// The whole project file: `[project]`, `[menu]`, `[roll]`, and `[uses]` naming every project
    /// the recipe uses (roles ref1…, strings1…, fine1…), so the ledger sees them change.
    pub fn to_toml(&self) -> String {
        let mut doc = DocumentMut::new();
        let mut project = Table::new();
        project["family"] = value("rolled");
        project["body"] = value("grand");
        doc["project"] = Item::Table(project);
        self.fill(&mut doc);
        doc.to_string()
    }

    /// `text` (a project file) with the recipe's values written in: `[menu]`'s name and group,
    /// `[roll]`'s values, `[uses]` anew; every other line (comments, other tables) kept.
    pub fn update_toml(&self, text: &str) -> Result<String, Error> {
        let mut doc: DocumentMut = text.parse().map_err(|e| err(format!("{}: project.toml: {e}", self.name)))?;
        self.fill(&mut doc);
        Ok(doc.to_string())
    }

    fn fill(&self, doc: &mut DocumentMut) {
        // each table filled in place (its comments and position kept), made when missing
        fn table<'a>(doc: &'a mut DocumentMut, k: &str) -> &'a mut Table {
            if !doc.get(k).is_some_and(Item::is_table) {
                doc[k] = Item::Table(Table::new());
            }
            doc[k].as_table_mut().expect("a table")
        }
        let menu = table(doc, "menu");
        menu["name"] = value(self.menu.0.as_str());
        menu["group"] = value(self.menu.1.as_str());
        let roll = table(doc, "roll");
        roll["references"] = value(self.references.iter().map(String::as_str).collect::<Array>());
        let mut strings = InlineTable::new();
        for (p, w) in &self.strings {
            strings.insert(p, (*w).into());
        }
        roll["strings"] = value(strings);
        roll["fine_structure"] = value(self.fine_structure.iter().map(String::as_str).collect::<Array>());
        roll["warmth"] = value(self.warmth);
        roll["air"] = value(self.air);
        roll["shade"] = value(self.shade.iter().copied().collect::<Array>());
        roll["seed"] = value(self.seed as i64);
        let uses = table(doc, "uses");
        uses.clear();
        for (i, p) in self.references.iter().enumerate() {
            uses[&format!("ref{}", i + 1)] = value(p.as_str());
        }
        for (i, (p, _)) in self.strings.iter().enumerate() {
            uses[&format!("strings{}", i + 1)] = value(p.as_str());
        }
        for (i, p) in self.fine_structure.iter().enumerate() {
            uses[&format!("fine{}", i + 1)] = value(p.as_str());
        }
    }
}
