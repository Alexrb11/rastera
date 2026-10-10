use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, TrySendError, bounded};
use rastera_core::{
    CancellationToken, DecodedImage, ErrorKind, FileStamp, ImageBackend, RasteraError, Result,
};

use crate::memory::{MemoryBudget, MemoryLease};

struct Request {
    generation: u64,
    path: PathBuf,
    max_edge: Option<u32>,
    expected: Option<FileStamp>,
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
pub struct ResidentImage {
    pub image: DecodedImage,
    _lease: MemoryLease,
}
impl LoadedImage {
    pub fn into_resident(self) -> Result<ResidentImage> {
        let image = self.result?;
        let lease = self._lease.ok_or_else(|| {
            RasteraError::new(ErrorKind::Internal, "Imagen sin reserva de memoria")
        })?;
        Ok(ResidentImage {
            image,
            _lease: lease,
        })
    }
}

pub struct ImageLoader {
    stats: Arc<Mutex<ImageLoadStats>>,
    state: Arc<(Mutex<State>, Condvar)>,
    results: Receiver<LoadedImage>,
    worker: Option<JoinHandle<()>>,
    budget: MemoryBudget,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ImageLoadStats {
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    /// Includes header inspection, waiting for a reservation, decoding and conversion.
    pub last_elapsed: Duration,
}

impl ImageLoader {
    pub fn new(
        backend: Arc<dyn ImageBackend>,
        budget: MemoryBudget,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let stats = Arc::new(Mutex::new(ImageLoadStats::default()));
        let worker_stats = stats.clone();
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
                let started = Instant::now();
                let operation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    request.cancel.check()?;
                    let info = backend.inspect(&request.path)?;
                    if request
                        .expected
                        .as_ref()
                        .is_some_and(|stamp| stamp != &info.stamp)
                    {
                        return Err(RasteraError::new(
                            ErrorKind::Changed,
                            "El original cambió; actualiza la imagen antes de ampliar",
                        ));
                    }
                    request.cancel.check()?;
                    let mut lease = worker_budget.reserve_wait(
                        info.working_bytes()?,
                        &request.cancel,
                        false,
                    )?;
                    let decoded = match request.max_edge {
                        Some(edge) => {
                            backend.decode(&request.path, &info, edge, &request.cancel)?
                        }
                        None => backend.decode_original(&request.path, &info, &request.cancel)?,
                    };
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
                {
                    let mut stats = worker_stats.lock().unwrap_or_else(|e| e.into_inner());
                    stats.last_elapsed = started.elapsed();
                    match &event.result {
                        Ok(_) => stats.completed += 1,
                        Err(error) if error.kind == ErrorKind::Cancelled => stats.cancelled += 1,
                        Err(_) => stats.failed += 1,
                    }
                }
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
            stats,
            state,
            results,
            worker: Some(worker),
            budget,
        }
    }

    pub fn request(&self, generation: u64, path: PathBuf, max_edge: u32) {
        self.submit(generation, path, Some(max_edge), None);
    }
    pub fn request_original(&self, generation: u64, path: PathBuf, expected: FileStamp) {
        self.submit(generation, path, None, Some(expected));
    }
    fn submit(
        &self,
        generation: u64,
        path: PathBuf,
        max_edge: Option<u32>,
        expected: Option<FileStamp>,
    ) {
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
            expected,
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
    pub fn stats(&self) -> ImageLoadStats {
        *self.stats.lock().unwrap_or_else(|e| e.into_inner())
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
        fn decode_original(
            &self,
            path: &std::path::Path,
            info: &ImageInfo,
            cancel: &CancellationToken,
        ) -> Result<DecodedImage> {
            self.decode(path, info, 1, cancel)
        }
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
                source_size: (1, 1),
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
    #[test]
    fn original_residency_holds_budget_and_changed_stamp_is_rejected_before_loading() {
        let budget = MemoryBudget::new(16);
        let (tx, rx) = bounded(2);
        let loader = ImageLoader::new(
            Arc::new(Backend),
            budget.clone(),
            Arc::new(move || {
                let _ = tx.try_send(());
            }),
        );
        loader.request_original(
            1,
            "image.png".into(),
            FileStamp {
                length: 4,
                modified: None,
            },
        );
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let resident = loader.try_recv().unwrap().into_resident().unwrap();
        assert_eq!(resident.image.source_size, (1, 1));
        assert_eq!(budget.used(), 8);
        assert!(budget.reserve(9).is_err());
        drop(resident);
        assert_eq!(budget.used(), 0);
        assert_eq!(loader.stats().completed, 1);
        loader.request_original(
            2,
            "image.png".into(),
            FileStamp {
                length: 5,
                modified: None,
            },
        );
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            loader.try_recv().unwrap().result.unwrap_err().kind,
            ErrorKind::Changed
        );
        assert_eq!(budget.used(), 0);
        assert_eq!(loader.stats().failed, 1);
    }
    #[test]
    fn closing_original_load_waiting_for_memory_releases_the_worker() {
        let budget = MemoryBudget::new(16);
        let held = budget.reserve(16).unwrap();
        let loader = ImageLoader::new(Arc::new(Backend), budget.clone(), Arc::new(|| {}));
        loader.request_original(
            1,
            "image.png".into(),
            FileStamp {
                length: 4,
                modified: None,
            },
        );
        std::thread::sleep(Duration::from_millis(30));
        drop(loader);
        assert_eq!(budget.used(), 16);
        drop(held);
        assert_eq!(budget.used(), 0);
    }
}
