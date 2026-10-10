use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, TrySendError, bounded};
use rastera_core::{
    CancellationToken, DecodedImage, ErrorKind, ImageBackend, RasteraError, Result,
};

use crate::memory::{MemoryBudget, MemoryLease};

struct Request {
    generation: u64,
    path: PathBuf,
    max_edge: u32,
    cancel: CancellationToken,
}

#[derive(Default)]
struct State {
    pending: Option<Request>,
    active: Option<CancellationToken>,
    stopping: bool,
}

pub struct LoadedImage {
    pub generation: u64,
    pub result: Result<DecodedImage>,
    // Kept while pixels sit in the result queue and while UI uploads them.
    _lease: Option<MemoryLease>,
}

pub struct ImageLoader {
    state: Arc<(Mutex<State>, Condvar)>,
    results: Receiver<LoadedImage>,
    worker: Option<JoinHandle<()>>,
    budget: MemoryBudget,
}

impl ImageLoader {
    pub fn new(
        backend: Arc<dyn ImageBackend>,
        budget: MemoryBudget,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let (tx, results) = bounded(1);
        let worker_state = state.clone();
        let worker_budget = budget.clone();
        let discard = results.clone();
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
                    let request = state.pending.take().expect("pending request");
                    state.active = Some(request.cancel.clone());
                    request
                };
                while discard.try_recv().is_ok() {}
                let operation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    request.cancel.check()?;
                    let info = backend.inspect(&request.path)?;
                    request.cancel.check()?;
                    let mut lease = worker_budget.reserve_wait(
                        info.working_bytes()?,
                        &request.cancel,
                        false,
                    )?;
                    let decoded =
                        backend.decode(&request.path, &info, request.max_edge, &request.cancel)?;
                    request.cancel.check()?;
                    lease.shrink_to(decoded.rgba.len() as u64 * 2);
                    Ok::<_, RasteraError>((decoded, lease))
                }));
                let (result, lease) = match operation {
                    Ok(Ok((decoded, lease))) => (Ok(decoded), Some(lease)),
                    Ok(Err(error)) => (Err(error), None),
                    Err(_) => (
                        Err(RasteraError::new(
                            ErrorKind::Internal,
                            "El decodificador falló",
                        )),
                        None,
                    ),
                };
                let event = LoadedImage {
                    generation: request.generation,
                    result,
                    _lease: lease,
                };
                match tx.try_send(event) {
                    Ok(()) => {}
                    Err(TrySendError::Full(event)) => {
                        let _ = discard.try_recv();
                        let _ = tx.try_send(event);
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                }
                wake();
            }
        });
        Self {
            state,
            results,
            worker: Some(worker),
            budget,
        }
    }

    pub fn request(&self, generation: u64, path: PathBuf, max_edge: u32) {
        // Discard any old buffered preview so its lease cannot starve a new load.
        while self.results.try_recv().is_ok() {}
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(active) = &state.active {
            active.cancel();
        }
        if let Some(pending) = &state.pending {
            pending.cancel.cancel();
        }
        state.pending = Some(Request {
            generation,
            path,
            max_edge,
            cancel: CancellationToken::default(),
        });
        ready.notify_one();
    }

    pub fn try_recv(&self) -> Option<LoadedImage> {
        self.results.try_recv().ok()
    }

    pub fn cancel(&self) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        state.pending = None;
        if let Some(active) = &state.active {
            active.cancel();
        }
        while self.results.try_recv().is_ok() {}
    }

    pub fn reserved_bytes(&self) -> u64 {
        self.budget.used()
    }
}

impl Drop for ImageLoader {
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
    use rastera_core::{FileStamp, ImageInfo, ImageKind};
    use std::time::Duration;

    struct Backend;
    impl ImageBackend for Backend {
        fn inspect(&self, _: &std::path::Path) -> Result<ImageInfo> {
            Ok(ImageInfo {
                width: 1,
                height: 1,
                kind: ImageKind::Png,
                stamp: FileStamp {
                    length: 4,
                    modified: None,
                },
                has_color_profile: false,
            })
        }
        fn decode(
            &self,
            _: &std::path::Path,
            info: &ImageInfo,
            _: u32,
            cancel: &CancellationToken,
        ) -> Result<DecodedImage> {
            cancel.check()?;
            Ok(DecodedImage {
                width: 1,
                height: 1,
                rgba: vec![0; 4],
                info: info.clone(),
            })
        }
    }

    #[test]
    fn latest_request_wins_and_result_owns_its_reservation() {
        let budget = MemoryBudget::new(16);
        let (wake, received) = bounded(10);
        let loader = ImageLoader::new(
            Arc::new(Backend),
            budget.clone(),
            Arc::new(move || {
                let _ = wake.try_send(());
            }),
        );
        loader.request(1, "old.png".into(), 10);
        loader.request(2, "new.png".into(), 10);
        loop {
            received.recv_timeout(Duration::from_secs(5)).unwrap();
            if let Some(event) = loader.try_recv()
                && event.generation == 2
            {
                assert!(event.result.is_ok());
                assert_eq!(budget.used(), 8);
                drop(event);
                assert_eq!(budget.used(), 0);
                break;
            }
        }
    }
}
