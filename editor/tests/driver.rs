//! The driver client on a toy project: one slow step that writes its fit and sleeps.
use pe_editor::driver::{self, Event, Mode};
use pe_editor::project::{Repo, python};
use std::time::{Duration, Instant};

fn toy() -> (tempfile::TempDir, Repo) {
    let real = Repo::from_env();
    let d = tempfile::tempdir().unwrap();
    let projects = d.path().join("projects");
    std::fs::create_dir_all(projects.join("p/fits")).unwrap();
    let script = d.path().join("slow.py");
    std::fs::write(&script, format!(
        "import sys, time\nsys.path.insert(0, {:?})\nimport project as P\nP.update_fit(sys.argv[1], {{\"s\": \"half\"}})\ntime.sleep(60)\n",
        real.root.join("analysis").to_str().unwrap())).unwrap();
    std::fs::write(projects.join("steps.toml"), format!(
        "[step.slow]\nscope = \"part\"\nrun = [{:?}, {:?}, \"{{part}}\"]\nowns = [\"fit.s\"]\n\n[chain]\nslowy = [\"slow\"]\n",
        python(), script.to_str().unwrap())).unwrap();
    std::fs::write(projects.join("p/project.toml"), "[project]\nfamily = \"slowy\"\nexport = false\n\n[[part]]\nid = \"p\"\n").unwrap();
    std::fs::write(projects.join("p/fits/p.json"), "{\"a\": 1}").unwrap();
    let repo = Repo { root: real.root.clone(), projects, data: real.data.clone() };
    (d, repo)
}

fn wait_for_the_half_written_fit(repo: &Repo, build: &driver::Build) {
    let t = Instant::now();
    loop {
        if let Ok(Event::Start { step, .. }) = build.events.recv_timeout(Duration::from_secs(20)) {
            assert_eq!(step, "slow");
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(20), "no start event");
    }
    while !std::fs::read_to_string(repo.projects.join("p/fits/p.json")).unwrap().contains("half") {
        assert!(t.elapsed() < Duration::from_secs(20), "the step never wrote");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn events_parse() {
    let e = driver::parse_event(r#"{"event": "start", "project": "p", "step": "felt", "kind": "part", "id": "p", "log": "/l"}"#).unwrap();
    assert_eq!(e, Event::Start { project: "p".into(), step: "felt".into(), kind: "part".into(), id: "p".into(), log: "/l".into() });
    let e = driver::parse_event(r#"{"event": "failed", "project": "p", "step": "s", "kind": "part", "id": "p", "log": "/l", "exit": 1, "seconds": 0.5}"#).unwrap();
    assert!(matches!(e, Event::Failed { exit: 1, .. }));
    assert_eq!(driver::parse_event(r#"{"event": "finished", "ok": true}"#).unwrap(), Event::Finished { ok: true, cancelled: false });
    assert!(driver::parse_event("not json").is_err());
}

#[test]
fn status_of_a_toy_project() {
    let (_d, repo) = toy();
    let s = driver::status(&repo, "p").unwrap();
    assert_eq!(s.project, "p");
    assert_eq!(s.rows.len(), 1);
    assert_eq!((s.rows[0].step.as_str(), s.rows[0].reason.as_deref()), ("slow", Some("never run")));
    assert!(driver::status(&repo, "nope").is_err());
}

#[test]
fn the_status_is_read_in_the_background() {
    let (_d, repo) = toy();
    let t = Instant::now();
    let job = driver::StatusJob::start(&repo, "p");
    assert!(t.elapsed() < Duration::from_millis(100), "start returns at once ({:?})", t.elapsed());
    assert_eq!(job.project, "p");
    let got = loop {
        if let Some(s) = job.poll() {
            break s;
        }
        assert!(t.elapsed() < Duration::from_secs(30), "no status");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(got, driver::status(&repo, "p"));
}

#[test]
fn a_cancelled_build_restores_the_running_step() {
    let (_d, repo) = toy();
    let mut b = driver::Build::start(&repo, "p", &Mode::Stale).unwrap();
    wait_for_the_half_written_fit(&repo, &b);
    b.cancel();
    let last = std::iter::from_fn(|| b.events.recv_timeout(Duration::from_secs(30)).ok()).last();
    assert_eq!(last, Some(Event::Finished { ok: false, cancelled: true }));
    let t = Instant::now();
    while b.exit().is_none() {
        assert!(t.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(b.exit(), Some(130));
    assert_eq!(std::fs::read_to_string(repo.projects.join("p/fits/p.json")).unwrap().replace(' ', ""), "{\"a\":1}");
    assert!(!repo.projects.join("p/ledger.json").exists());
}

#[test]
fn dropping_a_build_restores_the_running_step() {
    let (_d, repo) = toy();
    let b = driver::Build::start(&repo, "p", &Mode::Stale).unwrap();
    wait_for_the_half_written_fit(&repo, &b);
    drop(b);
    assert_eq!(std::fs::read_to_string(repo.projects.join("p/fits/p.json")).unwrap().replace(' ', ""), "{\"a\":1}");
    assert!(!repo.projects.join("p/ledger.json").exists());
}

#[test]
fn the_log_tail() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("x.log");
    std::fs::write(&p, "1\n2\n3\n4\n").unwrap();
    assert_eq!(driver::log_tail(p.to_str().unwrap(), 2), "3\n4");
    assert_eq!(driver::log_tail("/nonexistent", 2), "");
}

#[test]
fn a_build_outliving_its_editor_is_cancelled() {
    // a killed editor runs no Drop: the kernel sends the build SIGTERM when the thread that started it
    // ends (PR_SET_PDEATHSIG), and build.py restores the running step's files
    let (_d, repo) = toy();
    let r = repo.clone();
    std::thread::spawn(move || {
        let b = driver::Build::start(&r, "p", &Mode::Stale).unwrap();
        wait_for_the_half_written_fit(&r, &b);
        std::mem::forget(b);
    })
    .join()
    .unwrap();
    let fit = repo.projects.join("p/fits/p.json");
    let t = Instant::now();
    while std::fs::read_to_string(&fit).unwrap().contains("half") {
        assert!(t.elapsed() < Duration::from_secs(30), "the orphaned build kept running");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(std::fs::read_to_string(&fit).unwrap().replace(' ', ""), "{\"a\":1}");
    assert!(!repo.projects.join("p/ledger.json").exists());
}
