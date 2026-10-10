use crate::memory::MemoryBudget;
use crossbeam_channel::{Receiver, Sender, bounded};
use rastera_core::{
    BatchBackend, CancellationToken, ErrorKind, ExportPlan, ExportRecipe, RasteraError, Result,
};
use std::{
    path::PathBuf,
    sync::Arc,
    thread::{self, JoinHandle},
};

enum Command {
    Plan {
        generation: u64,
        paths: Vec<PathBuf>,
        destination: PathBuf,
        recipe: ExportRecipe,
        cancel: CancellationToken,
    },
    Run {
        generation: u64,
        plan: ExportPlan,
        cancel: CancellationToken,
    },
}
pub enum BatchUpdate {
    Planned(Result<ExportPlan>),
    Started(PathBuf),
    Item { index: usize, result: Result<()> },
    Finished { cancelled: bool, result: Result<()> },
}
pub struct BatchEvent {
    pub generation: u64,
    pub update: BatchUpdate,
}
pub struct BatchWorker {
    tx: Option<Sender<Command>>,
    rx: Option<Receiver<BatchEvent>>,
    cancel: CancellationToken,
    worker: Option<JoinHandle<()>>,
}
impl BatchWorker {
    pub fn new(
        backend: Arc<dyn BatchBackend>,
        budget: MemoryBudget,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (tx, commands) = bounded(1);
        let (events, rx) = bounded(4);
        let worker = thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                let send = |generation, update| {
                    let sent = events.send(BatchEvent { generation, update }).is_ok();
                    wake();
                    sent
                };
                match command {
                    Command::Plan {
                        generation,
                        paths,
                        destination,
                        recipe,
                        cancel,
                    } => {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            backend.plan(&paths, &destination, recipe, &cancel)
                        }))
                        .unwrap_or_else(|_| {
                            Err(RasteraError::new(
                                ErrorKind::Internal,
                                "No se pudo preparar el lote",
                            ))
                        });
                        if !send(generation, BatchUpdate::Planned(result)) {
                            break;
                        }
                    }
                    Command::Run {
                        generation,
                        plan,
                        cancel,
                    } => {
                        let operation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                            || -> Result<()> {
                                cancel.check()?;
                                let mut session = backend.start(&plan)?;
                                if !send(
                                    generation,
                                    BatchUpdate::Started(session.journal_path().to_path_buf()),
                                ) {
                                    cancel.cancel();
                                }
                                for (index, item) in plan.items.iter().enumerate() {
                                    let result = (|| {
                                        cancel.check()?;
                                        let _lease = budget.reserve_wait(
                                            item.info.working_bytes()?.saturating_mul(2),
                                            &cancel,
                                            true,
                                        )?;
                                        session.export(index, item, &plan.recipe, &cancel)
                                    })();
                                    let record = match &result {
                                        Ok(()) => Ok(()),
                                        Err(error)
                                            if matches!(
                                                error.kind,
                                                ErrorKind::Published | ErrorKind::Journal
                                            ) =>
                                        {
                                            Err(error.clone())
                                        }
                                        Err(error) => session.record(
                                            index,
                                            if error.kind == ErrorKind::Cancelled {
                                                "cancelled"
                                            } else {
                                                "error"
                                            },
                                            &error.to_string(),
                                        ),
                                    };
                                    if !send(generation, BatchUpdate::Item { index, result }) {
                                        cancel.cancel();
                                    }
                                    record?;
                                }
                                session.finish(cancel.check().is_err())?;
                                Ok(())
                            },
                        ))
                        .unwrap_or_else(|_| {
                            Err(RasteraError::new(
                                ErrorKind::Internal,
                                "El procesador del lote falló; revisa el diario",
                            ))
                        });
                        if !send(
                            generation,
                            BatchUpdate::Finished {
                                cancelled: cancel.check().is_err(),
                                result: operation,
                            },
                        ) {
                            break;
                        }
                    }
                }
            }
        });
        Self {
            tx: Some(tx),
            rx: Some(rx),
            cancel: CancellationToken::default(),
            worker: Some(worker),
        }
    }
    fn submit(&mut self, command: Command, cancel: CancellationToken) -> Result<()> {
        self.tx.as_ref().unwrap().try_send(command).map_err(|_| {
            RasteraError::new(ErrorKind::Busy, "Ya hay una petición de lote pendiente")
        })?;
        self.cancel = cancel;
        Ok(())
    }
    pub fn plan(
        &mut self,
        generation: u64,
        paths: Vec<PathBuf>,
        destination: PathBuf,
        recipe: ExportRecipe,
    ) -> Result<()> {
        let cancel = CancellationToken::default();
        self.submit(
            Command::Plan {
                generation,
                paths,
                destination,
                recipe,
                cancel: cancel.clone(),
            },
            cancel,
        )
    }
    pub fn run(&mut self, generation: u64, plan: ExportPlan) -> Result<()> {
        let cancel = CancellationToken::default();
        self.submit(
            Command::Run {
                generation,
                plan,
                cancel: cancel.clone(),
            },
            cancel,
        )
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub fn try_recv(&self) -> Option<BatchEvent> {
        self.rx.as_ref()?.try_recv().ok()
    }
}
impl Drop for BatchWorker {
    fn drop(&mut self) {
        self.cancel();
        self.tx.take();
        self.rx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rastera_core::{ExportItem, ExportSession, FileStamp, ImageInfo, ImageKind};
    use std::{
        path::Path,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    struct Backend {
        cancel_first: bool,
        published: Arc<AtomicUsize>,
        states: Arc<Mutex<Vec<String>>>,
    }
    impl BatchBackend for Backend {
        fn plan(
            &self,
            _: &[PathBuf],
            _: &Path,
            _: ExportRecipe,
            _: &CancellationToken,
        ) -> Result<ExportPlan> {
            Ok(plan())
        }
        fn start(&self, _: &ExportPlan) -> Result<Box<dyn ExportSession>> {
            Ok(Box::new(Session {
                cancel_first: self.cancel_first,
                published: self.published.clone(),
                states: self.states.clone(),
            }))
        }
    }
    struct Session {
        cancel_first: bool,
        published: Arc<AtomicUsize>,
        states: Arc<Mutex<Vec<String>>>,
    }
    impl ExportSession for Session {
        fn journal_path(&self) -> &Path {
            Path::new("test-journal")
        }
        fn export(
            &mut self,
            index: usize,
            _: &ExportItem,
            _: &ExportRecipe,
            cancel: &CancellationToken,
        ) -> Result<()> {
            if !self.cancel_first && index == 1 {
                return Err(RasteraError::new(ErrorKind::Io, "Test failure"));
            }
            self.published.fetch_add(1, Ordering::SeqCst);
            if self.cancel_first && index == 0 {
                cancel.cancel();
            }
            Ok(())
        }
        fn record(&mut self, _: usize, state: &str, _: &str) -> Result<()> {
            self.states.lock().unwrap().push(state.to_owned());
            Ok(())
        }
        fn finish(&mut self, cancelled: bool) -> Result<()> {
            self.states.lock().unwrap().push(
                if cancelled {
                    "batch-cancelled"
                } else {
                    "batch-finished"
                }
                .into(),
            );
            Ok(())
        }
    }
    fn plan() -> ExportPlan {
        ExportPlan {
            recipe: ExportRecipe::default(),
            items: (0..8)
                .map(|index| ExportItem {
                    source: format!("source-{index}").into(),
                    destination: format!("out-{index}").into(),
                    output_size: (1, 1),
                    info: ImageInfo {
                        width: 1,
                        height: 1,
                        kind: ImageKind::Png,
                        stamp: FileStamp {
                            length: 4,
                            modified: None,
                        },
                        has_color_profile: false,
                    },
                })
                .collect(),
        }
    }
    #[test]
    fn cancellation_keeps_published_items_and_marks_remaining_items() {
        let published = Arc::new(AtomicUsize::new(0));
        let states = Arc::new(Mutex::new(Vec::new()));
        let budget = MemoryBudget::new(32);
        let mut worker = BatchWorker::new(
            Arc::new(Backend {
                cancel_first: true,
                published: published.clone(),
                states: states.clone(),
            }),
            budget.clone(),
            Arc::new(|| {}),
        );
        worker.run(7, plan()).unwrap();
        let mut successes = 0;
        let mut cancellations = 0;
        loop {
            let event = worker
                .rx
                .as_ref()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            assert_eq!(event.generation, 7);
            match event.update {
                BatchUpdate::Item { result: Ok(()), .. } => successes += 1,
                BatchUpdate::Item {
                    result: Err(error), ..
                } => {
                    assert_eq!(error.kind, ErrorKind::Cancelled);
                    cancellations += 1;
                }
                BatchUpdate::Finished { cancelled, result } => {
                    assert!(cancelled);
                    result.unwrap();
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(
            (successes, cancellations, published.load(Ordering::SeqCst)),
            (1, 7, 1)
        );
        assert_eq!(budget.used(), 0);
        assert_eq!(states.lock().unwrap().last().unwrap(), "batch-cancelled");
    }
    #[test]
    fn individual_failures_do_not_abort_remaining_files() {
        let published = Arc::new(AtomicUsize::new(0));
        let states = Arc::new(Mutex::new(Vec::new()));
        let mut worker = BatchWorker::new(
            Arc::new(Backend {
                cancel_first: false,
                published: published.clone(),
                states: states.clone(),
            }),
            MemoryBudget::new(32),
            Arc::new(|| {}),
        );
        worker.run(1, plan()).unwrap();
        loop {
            if let BatchUpdate::Finished { cancelled, result } = worker
                .rx
                .as_ref()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .update
            {
                assert!(!cancelled);
                result.unwrap();
                break;
            }
        }
        assert_eq!(published.load(Ordering::SeqCst), 7);
        assert_eq!(*states.lock().unwrap(), ["error", "batch-finished"]);
    }
    #[test]
    fn closing_a_full_progress_queue_releases_worker_and_memory() {
        let (wake, received) = bounded(10);
        let budget = MemoryBudget::new(32);
        let mut worker = BatchWorker::new(
            Arc::new(Backend {
                cancel_first: false,
                published: Arc::new(AtomicUsize::new(0)),
                states: Arc::new(Mutex::new(Vec::new())),
            }),
            budget.clone(),
            Arc::new(move || {
                let _ = wake.try_send(());
            }),
        );
        worker.run(1, plan()).unwrap();
        for _ in 0..4 {
            received.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let (closed, done) = bounded(1);
        std::thread::spawn(move || {
            drop(worker);
            closed.send(()).unwrap();
        });
        done.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(budget.used(), 0);
    }
}
