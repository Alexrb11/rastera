//! Notifications are hints. Only a complete, bounded scan can replace a library.
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, SendTimeoutError, Sender, bounded};
use rastera_core::{
    CancellationToken, ErrorKind, FolderBackend, FolderEntry, MAX_FOLDER_ENTRIES,
    MAX_FOLDER_PATH_BYTES,
};

const QUIET: Duration = Duration::from_millis(350);
const MAX_DELAY: Duration = Duration::from_secs(2);
const REVIEW: Duration = Duration::from_secs(15);
const TICK: Duration = Duration::from_millis(100);

type Prepare = Arc<dyn Fn(&LibraryUpdate, &CancellationToken) + Send + Sync>;

pub struct LibraryUpdate {
    pub generation: u64,
    /// None means a failed scan: retain the last usable library.
    pub entries: Option<Vec<FolderEntry>>,
    pub changed: HashSet<PathBuf>,
    pub polling: bool,
    pub error: Option<String>,
}

struct Request {
    generation: u64,
    path: PathBuf,
    baseline: Vec<FolderEntry>,
    cancel: CancellationToken,
}

#[derive(Default)]
struct State {
    pending: Option<Request>,
    active: Option<CancellationToken>,
    stopping: bool,
}

pub struct FolderMonitor {
    state: Arc<(Mutex<State>, Condvar)>,
    results: Receiver<LibraryUpdate>,
    worker: Option<JoinHandle<()>>,
}

fn changed_paths(
    old: &[FolderEntry],
    new: &[FolderEntry],
    hints: &HashSet<PathBuf>,
) -> HashSet<PathBuf> {
    let old: HashMap<_, _> = old
        .iter()
        .map(|entry| (&entry.path, &entry.stamp))
        .collect();
    new.iter()
        .filter(|entry| {
            old.get(&entry.path)
                .is_some_and(|stamp| *stamp != &entry.stamp)
                || hints.contains(&entry.path)
        })
        .map(|entry| entry.path.clone())
        .collect()
}

fn publish(
    tx: &Sender<LibraryUpdate>,
    mut event: LibraryUpdate,
    cancel: &CancellationToken,
    wake: &dyn Fn(),
) -> bool {
    loop {
        if cancel.check().is_err() {
            return false;
        }
        match tx.send_timeout(event, TICK) {
            Ok(()) => {
                wake();
                return true;
            }
            Err(SendTimeoutError::Timeout(returned)) => event = returned,
            Err(SendTimeoutError::Disconnected(_)) => return false,
        }
    }
}

fn monitor(
    backend: &dyn FolderBackend,
    request: Request,
    tx: &Sender<LibraryUpdate>,
    wake: &dyn Fn(),
    state: &Arc<(Mutex<State>, Condvar)>,
    review: Duration,
    prepare: &Prepare,
) {
    let mut watch = backend.watch(&request.path).ok();
    let mut baseline = request.baseline;
    let mut hints = HashSet::new();
    let mut hint_bytes = 0;
    let mut first = Some(Instant::now() - QUIET);
    let mut last = Instant::now() - QUIET;
    let mut reviewed = Instant::now();
    let mut previous_status = None;
    loop {
        if request.cancel.check().is_err() {
            break;
        }
        if let Some(observer) = &watch {
            match observer.take_changes() {
                Ok(changes) if changes.rescan || !changes.paths.is_empty() => {
                    first.get_or_insert_with(Instant::now);
                    last = Instant::now();
                    for path in changes.paths {
                        let bytes = path.as_os_str().as_encoded_bytes().len();
                        if hints.len() < MAX_FOLDER_ENTRIES
                            && hint_bytes + bytes <= MAX_FOLDER_PATH_BYTES
                            && hints.insert(path)
                        {
                            hint_bytes += bytes;
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    watch = None;
                    first.get_or_insert_with(Instant::now);
                }
            }
        }
        let periodic = reviewed.elapsed() >= review;
        if periodic
            || first.is_some_and(|first| last.elapsed() >= QUIET || first.elapsed() >= MAX_DELAY)
        {
            if periodic && watch.is_none() {
                watch = backend.watch(&request.path).ok();
            }
            let mut entries = Vec::new();
            let scan = backend.scan(&request.path, &request.cancel, &mut |page| {
                entries.extend(page);
                Ok(())
            });
            reviewed = Instant::now();
            first = None;
            if request.cancel.check().is_err() {
                break;
            }
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            if scan
                .as_ref()
                .is_err_and(|error| error.kind == ErrorKind::Io)
            {
                watch = None;
            }
            let polling = watch.is_none();
            let error = scan
                .as_ref()
                .err()
                .filter(|error| error.kind != ErrorKind::Cancelled)
                .map(ToString::to_string);
            let status = (polling, error.clone());
            let changed = changed_paths(&baseline, &entries, &hints);
            let replace = scan.is_ok() && (entries != baseline || !changed.is_empty());
            let event_entries = if replace {
                baseline = entries.clone();
                Some(entries)
            } else {
                None
            };
            if replace || previous_status.as_ref() != Some(&status) {
                previous_status = Some(status);
                let event = LibraryUpdate {
                    generation: request.generation,
                    entries: event_entries,
                    changed,
                    polling,
                    error,
                };
                prepare(&event, &request.cancel);
                if !publish(tx, event, &request.cancel, wake) {
                    break;
                }
            }
            // Keep hints after an incomplete scan, so a later successful scan refreshes pixels.
            if scan.is_ok() {
                hints.clear();
                hint_bytes = 0;
            }
        }
        let (lock, ready) = &**state;
        let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        if request.cancel.check().is_err() {
            break;
        }
        let _ = ready
            .wait_timeout(guard, TICK)
            .unwrap_or_else(|e| e.into_inner());
    }
}

impl FolderMonitor {
    pub fn new(backend: Arc<dyn FolderBackend>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::with_review(backend, wake, REVIEW)
    }
    /// Runs cache preparation on the monitor worker, before waking the UI.
    pub fn with_prepare(
        backend: Arc<dyn FolderBackend>,
        wake: Arc<dyn Fn() + Send + Sync>,
        prepare: Prepare,
    ) -> Self {
        Self::build(backend, wake, REVIEW, prepare)
    }
    fn with_review(
        backend: Arc<dyn FolderBackend>,
        wake: Arc<dyn Fn() + Send + Sync>,
        review: Duration,
    ) -> Self {
        Self::build(backend, wake, review, Arc::new(|_, _| {}))
    }
    fn build(
        backend: Arc<dyn FolderBackend>,
        wake: Arc<dyn Fn() + Send + Sync>,
        review: Duration,
        prepare: Prepare,
    ) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker_state = state.clone();
        let (tx, results) = bounded(1);
        let worker = thread::spawn(move || {
            loop {
                let request = {
                    let (lock, ready) = &*worker_state;
                    let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
                    while state.pending.is_none() && !state.stopping {
                        state = ready.wait(state).unwrap_or_else(|e| e.into_inner());
                    }
                    if state.stopping {
                        break;
                    }
                    let request = state.pending.take().expect("pending monitor");
                    state.active = Some(request.cancel.clone());
                    request
                };
                monitor(
                    &*backend,
                    request,
                    &tx,
                    &*wake,
                    &worker_state,
                    review,
                    &prepare,
                );
            }
        });
        Self {
            state,
            results,
            worker: Some(worker),
        }
    }
    pub fn cancel(&self) {
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(active) = state.active.take() {
            active.cancel();
        }
        state.pending = None;
        while self.results.try_recv().is_ok() {}
        ready.notify_one();
    }
    pub fn request(&self, generation: u64, path: PathBuf, baseline: Vec<FolderEntry>) {
        self.cancel();
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        state.pending = Some(Request {
            generation,
            path,
            baseline,
            cancel: CancellationToken::default(),
        });
        ready.notify_one();
    }
    pub fn try_recv(&self) -> Option<LibraryUpdate> {
        self.results.try_recv().ok()
    }
}

impl Drop for FolderMonitor {
    fn drop(&mut self) {
        self.cancel();
        let (lock, ready) = &*self.state;
        lock.lock().unwrap_or_else(|e| e.into_inner()).stopping = true;
        ready.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rastera_core::{FileStamp, RasteraError, Result};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestBackend {
        entries: Mutex<Vec<FolderEntry>>,
        fail: Mutex<bool>,
        scans: AtomicUsize,
    }
    impl FolderBackend for TestBackend {
        fn is_directory(&self, _: &std::path::Path) -> Result<bool> {
            Ok(true)
        }
        fn scan(
            &self,
            _: &std::path::Path,
            cancel: &CancellationToken,
            emit: &mut dyn FnMut(Vec<FolderEntry>) -> Result<()>,
        ) -> Result<()> {
            cancel.check()?;
            self.scans.fetch_add(1, Ordering::SeqCst);
            emit(self.entries.lock().unwrap().clone())?;
            if *self.fail.lock().unwrap() {
                return Err(RasteraError::new(ErrorKind::Io, "Lectura parcial"));
            }
            Ok(())
        }
    }
    fn entry(name: &str, length: u64) -> FolderEntry {
        FolderEntry {
            path: name.into(),
            stamp: Some(FileStamp {
                length,
                modified: None,
            }),
        }
    }
    fn backend() -> Arc<TestBackend> {
        Arc::new(TestBackend {
            entries: Mutex::new(vec![entry("b.png", 1), entry("a.png", 1)]),
            fail: Mutex::new(false),
            scans: AtomicUsize::new(0),
        })
    }
    fn recv(monitor: &FolderMonitor) -> LibraryUpdate {
        monitor
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
    }

    #[test]
    fn missed_events_are_reconciled_and_unchanged_scans_do_not_wake_ui() {
        let backend = backend();
        let monitor = FolderMonitor::with_review(
            backend.clone(),
            Arc::new(|| {}),
            Duration::from_millis(150),
        );
        monitor.request(
            1,
            "folder".into(),
            vec![entry("a.png", 1), entry("b.png", 1)],
        );
        let initial = recv(&monitor);
        assert!(initial.polling);
        assert!(initial.entries.is_none()); // Sorting alone is not a change.
        while backend.scans.load(Ordering::SeqCst) < 3 {
            thread::sleep(TICK);
        }
        assert!(monitor.try_recv().is_none());
        *backend.entries.lock().unwrap() = vec![entry("b.png", 2), entry("c.png", 1)];
        let updated = recv(&monitor);
        assert_eq!(
            updated.entries.unwrap(),
            [entry("b.png", 2), entry("c.png", 1)]
        );
        assert!(updated.changed.contains(Path::new("b.png")));
    }

    #[test]
    fn incomplete_scan_keeps_baseline_until_success_and_empty_folder_is_valid() {
        let backend = backend();
        let monitor = FolderMonitor::with_review(
            backend.clone(),
            Arc::new(|| {}),
            Duration::from_millis(150),
        );
        monitor.request(
            2,
            "folder".into(),
            vec![entry("a.png", 1), entry("b.png", 1)],
        );
        recv(&monitor);
        *backend.fail.lock().unwrap() = true;
        *backend.entries.lock().unwrap() = vec![entry("a.png", 2)];
        let failed = recv(&monitor);
        assert!(failed.entries.is_none());
        assert!(failed.error.is_some());
        *backend.fail.lock().unwrap() = false;
        let recovered = recv(&monitor);
        assert!(recovered.error.is_none());
        assert!(recovered.changed.contains(Path::new("a.png")));
        backend.entries.lock().unwrap().clear();
        assert!(recv(&monitor).entries.unwrap().is_empty());
    }

    #[test]
    fn replacing_folder_cancels_old_generation_even_with_full_queue() {
        let backend = backend();
        let monitor = FolderMonitor::new(backend.clone(), Arc::new(|| {}));
        monitor.request(3, "old".into(), vec![]);
        let deadline = Instant::now() + Duration::from_secs(5);
        while monitor.results.is_empty() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        monitor.request(4, "new".into(), vec![]);
        loop {
            if recv(&monitor).generation == 4 {
                break;
            }
        }
        // Drop also cancels waits and does not require consuming a result queue.
        monitor.request(5, "last".into(), vec![]);
        drop(monitor);
    }

    use std::path::Path;
}
