//! A rolled piano's export from its project (the `roll` step of the `rolled` chain).
//!
//! Usage: roll <project dir> [--instruments DIR] [--out DIR]
//!
//!   --instruments DIR  the exports the recipe names (default: <repo>/instruments, the repo being
//!                      the project dir's grandparent)
//!   --out DIR          where the export goes (default: <instruments>/<project name>)
//!
//! Prints "warning: <what>" per statistic the realism check still names (build.py shows those).

use dussek::roll::generate::{generate, write_export};
use dussek::roll::recipe::Recipe;
use std::path::PathBuf;

fn main() {
    if let Err(e) = run() {
        eprintln!("roll: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let project = PathBuf::from(args.next().ok_or("usage: roll <project dir> [--instruments DIR] [--out DIR]")?);
    let (mut instruments, mut out) = (None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--instruments" => instruments = Some(PathBuf::from(args.next().ok_or("--instruments needs a folder")?)),
            "--out" => out = Some(PathBuf::from(args.next().ok_or("--out needs a folder")?)),
            other => return Err(format!("unknown option {other}")),
        }
    }
    let project = std::fs::canonicalize(&project).map_err(|e| format!("{}: {e}", project.display()))?;
    let name = project.file_name().ok_or("the project folder has no name")?.to_string_lossy().into_owned();
    let instruments = instruments.unwrap_or_else(|| project.join("../../instruments"));
    let out = out.unwrap_or_else(|| instruments.join(&name));
    let text = std::fs::read_to_string(project.join("project.toml")).map_err(|e| format!("project.toml: {e}"))?;
    let recipe = Recipe::parse(&text, &name).map_err(|e| e.0)?;
    let (export, report, license) = generate(&recipe, &instruments).map_err(|e| e.0)?;
    write_export(&out, &export, &license).map_err(|e| e.0)?;
    for f in &report.failures {
        println!("warning: {f}");
    }
    println!(
        "{name}: knobs {:?}, pulled back {}×, {:.1} s → {}",
        report.knobs.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>(),
        report.pulled_back,
        report.seconds,
        out.display()
    );
    Ok(())
}
