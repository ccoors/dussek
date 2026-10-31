//! A comparison run: every pair once through the sink, the cache reused and following the
//! export, cancelling keeps what was done (the committed marimba and its set; returns early
//! without them).
use pe_editor::compare::{self, Run};
use pe_editor::project::{Project, Repo};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn setup() -> Option<(Repo, Vec<compare::Pair>, std::path::PathBuf)> {
    let repo = Repo::from_env();
    let p = Project::load(&repo, "marimba").ok()?;
    // returns early only when the set is not in data/
    let root = p.source_json(p.source_ids().first()?)?["root"].as_str()?.to_string();
    if !repo.data.join(root).is_dir() {
        return None;
    }
    let pairs = compare::pairs(&repo, &p).list;
    // a few keys of one mallet and dynamic: a short run
    let some: Vec<_> = pairs.into_iter().filter(|x| x.source == "marimba_yarn" && x.dynamic == "mf").take(4).collect();
    assert_eq!(some.len(), 4);
    let export = compare::export_dir(&repo, "marimba");
    Some((repo, some, export))
}

fn go(pairs: &[compare::Pair], export: &std::path::Path, cache: &std::path::Path, cancel: &AtomicBool) -> (Vec<usize>, usize) {
    let (progress, rendered) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let got = Mutex::new(Vec::new());
    compare::run(&Run { pairs, export, cache, threads: 2, progress: &progress, rendered: &rendered, cancel },
        &|i, r| { r.expect("compared"); got.lock().unwrap().push(i); }).unwrap();
    let mut v = got.into_inner().unwrap();
    v.sort();
    (v, rendered.load(Ordering::Relaxed))
}

#[test]
fn cache_is_reused_and_follows_the_export() {
    let Some((_, pairs, export)) = setup() else { return };
    let dir = tempfile::tempdir().unwrap();
    // a copy of the export, so its stamp can change
    let ex = dir.path().join("export");
    std::fs::create_dir(&ex).unwrap();
    for f in ["instrument.json", "data.bin"] {
        std::fs::copy(export.join(f), ex.join(f)).unwrap();
    }
    let cache = dir.path().join("compare.json");
    let no = AtomicBool::new(false);
    let (got, rendered) = go(&pairs, &ex, &cache, &no);
    assert_eq!(got, (0..pairs.len()).collect::<Vec<_>>());
    assert_eq!(rendered, pairs.len());
    let (got, rendered) = go(&pairs, &ex, &cache, &no);
    assert_eq!(got.len(), pairs.len());
    assert_eq!(rendered, 0, "all from the cache");
    // a re-export (the file written again): everything renders again
    std::thread::sleep(std::time::Duration::from_millis(20));
    let text = std::fs::read(ex.join("instrument.json")).unwrap();
    std::fs::write(ex.join("instrument.json"), text).unwrap();
    let (_, rendered) = go(&pairs, &ex, &cache, &no);
    assert_eq!(rendered, pairs.len());
}

#[test]
fn cancel_keeps_finished_pairs() {
    let Some((_, pairs, export)) = setup() else { return };
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("compare.json");
    let cancel = AtomicBool::new(false);
    let (progress, rendered) = (AtomicUsize::new(0), AtomicUsize::new(0));
    compare::run(&Run { pairs: &pairs, export: &export, cache: &cache, threads: 1, progress: &progress, rendered: &rendered, cancel: &cancel },
        &|_, _| cancel.store(true, Ordering::Relaxed)).unwrap();
    let done = rendered.load(Ordering::Relaxed);
    assert!(done >= 1 && done < pairs.len(), "{done} of {}", pairs.len());
    let again = AtomicBool::new(false);
    let (_, rendered) = go(&pairs, &export, &cache, &again);
    assert_eq!(rendered, pairs.len() - done, "the finished ones came from the cache");
}

#[test]
fn a_pair_is_rendered_as_long_as_its_recording_sounds() {
    let Some((_, pairs, export)) = setup() else { return };
    let mut r = compare::Renderer::new(&export).unwrap();
    let e = compare::compare_one(&pairs[0], &mut r).unwrap();
    assert_eq!(e.render.end_s, e.recording.end_s);
    assert!(e.recording.end_s > 1.0, "{:?}", e.recording);
    let d = compare::diff(&e.recording, &e.render);
    eprintln!("{} {}: {d:?}", pairs[0].note, pairs[0].dynamic);
}

#[test]
fn a_missing_export_fails_the_run() {
    let Some((_, pairs, _)) = setup() else { return };
    let dir = tempfile::tempdir().unwrap();
    let (p, r, c) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicBool::new(false));
    let missing = dir.path().join("none");
    let res = compare::run(&Run { pairs: &pairs, export: &missing, cache: &dir.path().join("c.json"), threads: 1, progress: &p, rendered: &r, cancel: &c }, &|_, _| {});
    assert!(res.is_err());
}



#[test]
fn a_panicking_worker_ends_the_run_with_an_error_and_keeps_the_cache() {
    // a worker that panics (an engine or decoder fault) neither hangs the run nor loses what the
    // others finished
    let Some((_, pairs, export)) = setup() else { return };
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("compare.json");
    let (p, r, c) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicBool::new(false));
    let calls = AtomicUsize::new(0);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compare::run(
        &Run { pairs: &pairs, export: &export, cache: &cache, threads: 1, progress: &p, rendered: &r, cancel: &c },
        &|_, _| if calls.fetch_add(1, Ordering::Relaxed) == 1 { panic!("a fault in the second pair") })));
    let res = res.expect("the run itself does not panic");
    assert!(res.is_err(), "the run reports the fault");
    assert!(cache.is_file(), "the finished pairs are cached");
}
