//! proto/build.py as a subprocess: a project's status, a build's events (build.py --json), and
//! cancelling a build: SIGINT, or SIGTERM when the editor drops a running build (closing during a
//! run); either way build.py restores the running step's files and leaves the ledger as it was.
use crate::project::{Repo, python};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Row {
    pub step: String,
    pub kind: String,
    pub id: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Status {
    pub project: String,
    pub rows: Vec<Row>,
    pub uses: BTreeMap<String, Vec<Row>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum Event {
    Start { project: String, step: String, kind: String, id: String, log: String },
    Done { project: String, step: String, kind: String, id: String, log: String, seconds: f64 },
    Failed { project: String, step: String, kind: String, id: String, log: String, exit: i32, seconds: f64 },
    Finished {
        ok: bool,
        #[serde(default)]
        cancelled: bool,
    },
}

pub fn parse_event(line: &str) -> Result<Event, String> {
    serde_json::from_str(line).map_err(|e| format!("{e}: {line}"))
}

fn command(repo: &Repo) -> Command {
    let mut c = Command::new(python());
    c.arg(repo.root.join("proto/build.py")).current_dir(&repo.root).env("PE_PROJECTS", &repo.projects);
    c
}

pub fn status(repo: &Repo, project: &str) -> Result<Status, String> {
    let out = command(repo).args(["status", "--json", project]).output()
        .map_err(|e| format!("{} proto/build.py: {e}", python()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().last().unwrap_or("build.py status failed").to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.lines().next().unwrap_or_default()).map_err(|e| e.to_string())
}

/// a project's status read on a thread of its own (build.py takes about a second)
pub struct StatusJob {
    pub project: String,
    rx: Receiver<Result<Status, String>>,
}

impl StatusJob {
    pub fn start(repo: &Repo, project: &str) -> StatusJob {
        let (tx, rx) = channel();
        let (repo, name) = (repo.clone(), project.to_string());
        std::thread::spawn(move || {
            let _ = tx.send(status(&repo, &name));
        });
        StatusJob { project: project.into(), rx }
    }

    /// the status once it is read
    pub fn poll(&self) -> Option<Result<Status, String>> {
        match self.rx.try_recv() {
            Ok(s) => Some(s),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("the status was not read".into())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Stale,
    From(String),
    Only(String),
}

pub struct Build {
    child: Child,
    pub events: Receiver<Event>,
    pub started: Instant,
    stderr: Arc<Mutex<String>>,
    cancelled: bool,
}

impl Build {
    pub fn start(repo: &Repo, project: &str, mode: &Mode) -> Result<Build, String> {
        let mut c = command(repo);
        c.args(["build", "--json", project]);
        match mode {
            Mode::Stale => {
                c.arg("--stale");
            }
            Mode::From(s) => {
                c.args(["--from", s]);
            }
            Mode::Only(s) => {
                c.args(["--only", s]);
            }
        }
        // the editor killed (no Drop): the build gets SIGTERM when the thread that started it ends
        // (Linux; the editor starts builds on its UI thread)
        // SAFETY: prctl is async-signal-safe; nothing else runs between fork and exec
        unsafe {
            c.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let mut child = c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
            .map_err(|e| format!("{} proto/build.py: {e}", python()))?;
        let out = child.stdout.take().unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Ok(e) = parse_event(&line) {
                    if tx.send(e).is_err() {
                        break;
                    }
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let mut err = child.stderr.take().unwrap();
        let sink = stderr.clone();
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = err.read_to_string(&mut s);
            *sink.lock().unwrap() = s;
        });
        Ok(Build { child, events: rx, started: Instant::now(), stderr, cancelled: false })
    }

    fn signal(&self, sig: i32) {
        // SAFETY: kill(2) on our own child's pid
        unsafe {
            libc::kill(self.child.id() as i32, sig);
        }
    }

    /// SIGINT (once): build.py kills the running step, restores its files, leaves the ledger as
    /// it was; it ignores the signals after the first (a second would interrupt the restore)
    pub fn cancel(&mut self) {
        if !self.cancelled {
            self.cancelled = true;
            self.signal(libc::SIGINT);
        }
    }

    /// a cancel was sent (the build is restoring and ending)
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }

    /// the exit status once the build has ended
    pub fn exit(&mut self) -> Option<i32> {
        self.child.try_wait().ok().flatten().map(|s| s.code().unwrap_or(-1))
    }

    /// what build.py wrote to stderr (complete once it has ended)
    pub fn stderr(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }
}

impl Drop for Build {
    fn drop(&mut self) {
        if self.exit().is_none() {
            if !self.cancelled {
                self.signal(libc::SIGTERM);
            }
            let _ = self.child.wait();
        }
    }
}

/// the last `lines` lines of a log file ("" when it cannot be read)
pub fn log_tail(path: &str, lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else { return String::new() };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}
