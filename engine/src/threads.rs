//! How many threads a build takes (an instrument's compile, its responses, a felt's measurement):
//! every core by default (the offline tools), fewer where an audio thread shares the machine (the
//! plugin, `pe_set_build_threads`): a build on every core slowed the audio thread's blocks 2–3×
//! at their tail (the shared cache, the cores' siblings, the clock).
use std::sync::atomic::{AtomicUsize, Ordering};

/// 0: every core (up to 16)
static BUILD_THREADS: AtomicUsize = AtomicUsize::new(0);

/// The threads a build spreads its work over.
pub fn build_threads() -> usize {
    match BUILD_THREADS.load(Ordering::Relaxed) {
        0 => std::thread::available_parallelism().map_or(4, |n| n.get()).min(16),
        n => n,
    }
}

/// At most `n` threads per build from now on (0: every core), for the whole process.
pub fn set_build_threads(n: usize) {
    BUILD_THREADS.store(n.min(16), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_core_by_default_and_a_limit_when_set() {
        let all = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        assert_eq!(build_threads(), all);
        set_build_threads(3);
        assert_eq!(build_threads(), 3);
        set_build_threads(0);
        assert_eq!(build_threads(), all);
    }
}
