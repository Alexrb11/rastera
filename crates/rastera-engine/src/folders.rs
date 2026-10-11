use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::{Receiver, SendTimeoutError, Sender, bounded};
use rastera_core::{
    CancellationToken, ErrorKind, FolderBackend, FolderEntry, RasteraError, Result,
};

pub enum FolderUpdate {
    File(PathBuf),
    Page(Vec<FolderEntry>),
    Sorted(Vec<FolderEntry>),
    Finished(Result<()>),
}

pub struct FolderEvent {
    pub generation: u64,
    pub update: FolderUpdate,
}

struct Request {
    generation: u64,
    path: PathBuf,
    cancel: CancellationToken,
}

#[derive(Default)]
struct State {
    pending: Option<Request>,
    active: Option<CancellationToken>,
    stopping: bool,
}

pub struct FolderScanner {
    state: Arc<(Mutex<State>, Condvar)>,
    results: Receiver<FolderEvent>,
    worker: Option<JoinHandle<()>>,
}

fn send(
    tx: &Sender<FolderEvent>,
    request: &Request,
    update: FolderUpdate,
    wake: &dyn Fn(),
) -> Result<()> {
    let mut event = FolderEvent {
        generation: request.generation,
        update,
    };
    loop {
        request.cancel.check()?;
        match tx.send_timeout(event, Duration::from_millis(25)) {
            Ok(()) => {
                wake();
                return Ok(());
            }
            Err(SendTimeoutError::Timeout(returned)) => {
                event = returned;
            }
            Err(SendTimeoutError::Disconnected(_)) => {
                return Err(RasteraError::new(ErrorKind::Cancelled, "La vista se cerró"));
            }
        }
    }
}

impl FolderScanner {
    pub fn new(backend: Arc<dyn FolderBackend>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker_state = state.clone();
        let (tx, results) = bounded(2);
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
                    let request = state.pending.take().expect("pending scan");
                    state.active = Some(request.cancel.clone());
                    request
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    request.cancel.check()?;
                    if !backend.is_directory(&request.path)? {
                        return send(
                            &tx,
                            &request,
                            FolderUpdate::File(request.path.clone()),
                            &*wake,
                        );
                    }
                    let mut entries = Vec::new();
                    backend.scan(&request.path, &request.cancel, &mut |page| {
                        entries.extend(page.iter().cloned());
                        send(&tx, &request, FolderUpdate::Page(page), &*wake)
                    })?;
                    entries.sort_by(|left, right| left.path.cmp(&right.path));
                    send(&tx, &request, FolderUpdate::Sorted(entries), &*wake)
                }));
                let result = result.unwrap_or_else(|_| {
                    Err(RasteraError::new(
                        ErrorKind::Internal,
                        "Falló la lectura de la carpeta",
                    ))
                });
                let _ = send(&tx, &request, FolderUpdate::Finished(result), &*wake);
            }
        });
        Self {
            state,
            results,
            worker: Some(worker),
        }
    }

    pub fn request(&self, generation: u64, path: PathBuf) {
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(active) = &state.active {
            active.cancel();
        }
        state.pending = Some(Request {
            generation,
            path,
            cancel: CancellationToken::default(),
        });
        while self.results.try_recv().is_ok() {}
        ready.notify_one();
    }

    pub fn try_recv(&self) -> Option<FolderEvent> {
        self.results.try_recv().ok()
    }
}

impl Drop for FolderScanner {
    fn drop(&mut self) {
        let (lock, ready) = &*self.state;
        {
            let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
            state.stopping = true;
            state.pending = None;
            if let Some(active) = &state.active {
                active.cancel();
            }
        }
        ready.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Backend;
    impl FolderBackend for Backend {
        fn is_directory(&self, path: &std::path::Path) -> Result<bool> {
            Ok(path == std::path::Path::new("folder"))
        }
        fn scan(
            &self,
            _: &std::path::Path,
            _: &CancellationToken,
            emit: &mut dyn FnMut(Vec<FolderEntry>) -> Result<()>,
        ) -> Result<()> {
            emit(vec![
                FolderEntry {
                    path: "z.png".into(),
                    stamp: None,
                },
                FolderEntry {
                    path: "a.png".into(),
                    stamp: None,
                },
            ])
        }
    }

    #[test]
    fn final_order_is_stable_and_file_input_is_routed_to_viewer() {
        let scanner = FolderScanner::new(Arc::new(Backend), Arc::new(|| {}));
        scanner.request(7, "folder".into());
        let first = scanner
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(matches!(first.update, FolderUpdate::Page(_)));
        let second = scanner
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let FolderUpdate::Sorted(entries) = second.update else {
            panic!("expected sorted result");
        };
        assert_eq!(entries[0].path, PathBuf::from("a.png"));
        let _ = scanner
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        scanner.request(8, "image.png".into());
        let event = scanner
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(event.generation, 8);
        assert!(matches!(event.update, FolderUpdate::File(_)));
    }

    #[test]
    fn closing_while_result_queue_is_full_does_not_deadlock() {
        let scanner = FolderScanner::new(Arc::new(Backend), Arc::new(|| {}));
        scanner.request(1, "folder".into());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while scanner.results.len() < 2 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        drop(scanner);
    }
}
