use crossbeam_channel::{Receiver, Sender, bounded};
use rastera_core::{Annotation, CatalogBackend, ErrorKind, ImageInfo, RasteraError, Result};
use std::{
    path::PathBuf,
    sync::Arc,
    thread::{self, JoinHandle},
};

pub enum CatalogCommand {
    Load {
        generation: u64,
        paths: Vec<PathBuf>,
    },
    Save {
        generation: u64,
        path: PathBuf,
        annotation: Annotation,
        info: Option<ImageInfo>,
    },
}
pub struct CatalogEvent {
    pub generation: u64,
    pub result: Result<Vec<(PathBuf, Annotation)>>,
}
pub struct CatalogWorker {
    tx: Option<Sender<CatalogCommand>>,
    rx: Option<Receiver<CatalogEvent>>,
    worker: Option<JoinHandle<()>>,
}
impl CatalogWorker {
    pub fn new(
        open: impl FnOnce() -> Result<Box<dyn CatalogBackend>> + Send + 'static,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (tx, commands) = bounded(32);
        let (events, rx) = bounded(32);
        let worker = thread::spawn(move || {
            let mut db = open();
            while let Ok(command) = commands.recv() {
                let (generation, result) = match command {
                    CatalogCommand::Load { generation, paths } => (
                        generation,
                        match &mut db {
                            Ok(db) => db.load(&paths),
                            Err(error) => Err(error.clone()),
                        },
                    ),
                    CatalogCommand::Save {
                        generation,
                        path,
                        annotation,
                        info,
                    } => (
                        generation,
                        match &mut db {
                            Ok(db) => db
                                .save(&path, &annotation, info.as_ref())
                                .map(|()| vec![(path, annotation)]),
                            Err(error) => Err(error.clone()),
                        },
                    ),
                };
                // Drain accepted writes even if the window closes before their acknowledgments.
                let _ = events.send(CatalogEvent { generation, result });
                wake();
            }
        });
        Self {
            tx: Some(tx),
            rx: Some(rx),
            worker: Some(worker),
        }
    }
    pub fn request(&self, command: CatalogCommand) -> Result<()> {
        self.tx.as_ref().unwrap().try_send(command).map_err(|_| {
            RasteraError::new(
                ErrorKind::Busy,
                "El catálogo está ocupado; vuelve a intentarlo",
            )
        })
    }
    pub fn try_recv(&self) -> Option<CatalogEvent> {
        self.rx.as_ref()?.try_recv().ok()
    }
}
impl Drop for CatalogWorker {
    fn drop(&mut self) {
        self.tx.take();
        self.rx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
