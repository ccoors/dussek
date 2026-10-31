//! `render`'s command line (midi2wav's, which the standalone runs as `dussek render`) only grows:
//! engine/tests/cli.lock records its positional arguments, its options and the keys of `--param`
//! and `--tuning`. A recorded one that is gone or no longer parses fails here; a new one fails
//! until it is recorded (PE_WRITE_CLI_LOCK=1 appends it, never rewrites). Removing or renaming one
//! takes a deprecation release in between (docs/compatibility.md).

use std::collections::BTreeSet;

use dussek::spec::{PARAM_KEYS, TUNING_KEYS};
use dussek::{Params, Tuning};

const LOCK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/cli.lock");

/// today's items, in the lock's form: `positional <n> <NAME>`, `option --<long>`, `param <key>`,
/// `tuning <key>`
fn today() -> Vec<String> {
    let cmd = dussek::midi2wav::command();
    let mut out = Vec::new();
    for (n, a) in cmd.get_positionals().enumerate() {
        let name = a.get_value_names().and_then(|v| v.first()).map_or_else(|| a.get_id().to_string(), |v| v.to_string());
        out.push(format!("positional {} {name}", n + 1));
    }
    for a in cmd.get_arguments() {
        if let Some(long) = a.get_long() {
            out.push(format!("option --{long}"));
        }
    }
    out.extend(PARAM_KEYS.iter().map(|(k, _)| format!("param {k}")));
    out.extend(TUNING_KEYS.iter().map(|(k, _)| format!("tuning {k}")));
    out
}

fn locked() -> Vec<String> {
    std::fs::read_to_string(LOCK)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_render_options_only_grow() {
    let now = today();
    if std::env::var_os("PE_WRITE_CLI_LOCK").is_some() {
        let had = locked();
        let mut text = std::fs::read_to_string(LOCK).unwrap_or_else(|_| {
            "# engine/tests/cli.lock: render's (midi2wav's) command line as released. Append only:\n\
             # docs/compatibility.md.\n"
                .to_string()
        });
        for item in now.iter().filter(|i| !had.contains(i)) {
            text += &format!("{item}\n");
        }
        std::fs::write(LOCK, text).unwrap();
    }
    let lock = locked();
    assert!(!lock.is_empty(), "no {LOCK}: PE_WRITE_CLI_LOCK=1 writes it");
    let have: BTreeSet<&String> = now.iter().collect();
    let gone: Vec<&String> = lock.iter().filter(|i| !have.contains(i)).collect();
    assert!(gone.is_empty(), "recorded in cli.lock, gone from the command line: {gone:?}");
    let recorded: BTreeSet<&String> = lock.iter().collect();
    let new: Vec<&String> = now.iter().filter(|i| !recorded.contains(i)).collect();
    assert!(new.is_empty(), "not in cli.lock yet (PE_WRITE_CLI_LOCK=1 appends them): {new:?}");
    // every recorded key still parses
    for item in &lock {
        if let Some(k) = item.strip_prefix("param ") {
            dussek::spec::params(Params::default(), &format!("{k}=1")).unwrap_or_else(|e| panic!("--param {k}: {e}"));
        } else if let Some(k) = item.strip_prefix("tuning ") {
            dussek::spec::tuning(Tuning::default(), &format!("{k}=1")).unwrap_or_else(|e| panic!("--tuning {k}: {e}"));
        }
    }
}
