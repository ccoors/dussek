//! The background jobs: measuring a source's files, and the suggestion (measuring first).
use eframe::egui;
use pe_editor::audio::{self, Measures};
use pe_editor::suggest::{self, Folder, Suggestion};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

pub enum Done {
    Suggested(Suggestion, Measures),
    Measured(Measures),
    Failed(String),
}

pub struct Job {
    progress: Arc<AtomicUsize>,
    total: usize,
    rx: Receiver<Done>,
    what: &'static str,
}

impl Job {
    pub fn start(what: &'static str, paths: Vec<PathBuf>, cache: PathBuf, then: impl FnOnce(Measures) -> Done + Send + 'static) -> Job {
        let progress = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = channel();
        let p = progress.clone();
        let total = paths.len();
        std::thread::spawn(move || {
            let m = audio::measure_all(&paths, &cache, &p);
            let _ = tx.send(then(m));
        });
        Job { progress, total, rx, what }
    }

    /// the suggestion for a folder, read from an SFZ file or from the names
    pub fn suggestion(data: PathBuf, folder: Folder, sfz: Option<String>, cache: PathBuf) -> Job {
        let paths = suggest::files_to_measure(&data, &folder, sfz.as_deref());
        Job::start("suggesting", paths, cache, move |m| match suggest::suggest(&data, &folder, sfz.as_deref(), &m) {
            Ok(s) => Done::Suggested(s, m),
            Err(e) => Done::Failed(e),
        })
    }

    /// the result when the job is done (a thread that ended without one failed: it panicked)
    pub fn poll(&self) -> Option<Done> {
        match self.rx.try_recv() {
            Ok(d) => Some(d),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Done::Failed(format!("{} stopped without a result", self.what))),
        }
    }

    pub fn bar(&self, ui: &mut egui::Ui) {
        let done = self.progress.load(Ordering::Relaxed);
        ui.add(egui::ProgressBar::new(done as f32 / self.total.max(1) as f32).text(format!("{} {done} / {} files", self.what, self.total)));
    }
}
