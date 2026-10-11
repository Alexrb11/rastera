use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use notify::{EventKind, RecursiveMode, Watcher};

use rastera_core::{
    CancellationToken, ErrorKind, FileStamp, FolderBackend, FolderChanges, FolderEntry,
    FolderWatch, MAX_FOLDER_ENTRIES, MAX_FOLDER_PATH_BYTES, RasteraError, Result,
};

#[derive(Default)]
pub struct LocalFolders;

#[derive(Default)]
struct Changes {
    paths: HashSet<PathBuf>,
    path_bytes: usize,
    rescan: bool,
    error: Option<String>,
}

struct NativeWatch {
    _watcher: notify::RecommendedWatcher,
    changes: Arc<Mutex<Changes>>,
}

impl FolderWatch for NativeWatch {
    fn take_changes(&self) -> Result<FolderChanges> {
        let mut changes = self.changes.lock().unwrap_or_else(|e| e.into_inner());
        let taken = std::mem::take(&mut *changes);
        if let Some(error) = taken.error {
            return Err(RasteraError::new(ErrorKind::Io, error));
        }
        Ok(FolderChanges {
            paths: taken.paths.into_iter().collect(),
            rescan: taken.rescan,
        })
    }
}

fn io_error(error: std::io::Error) -> RasteraError {
    RasteraError::new(
        ErrorKind::Io,
        format!("No se pudo acceder a la ruta: {error}"),
    )
}

impl FolderBackend for LocalFolders {
    fn watch(&self, path: &Path) -> Result<Box<dyn FolderWatch>> {
        let changes = Arc::new(Mutex::new(Changes::default()));
        let callback_changes = changes.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let mut changes = callback_changes.lock().unwrap_or_else(|e| e.into_inner());
                match event {
                    Ok(event) if !matches!(event.kind, EventKind::Access(_)) => {
                        changes.rescan = true;
                        for path in event.paths {
                            let bytes = path.as_os_str().as_encoded_bytes().len();
                            if changes.paths.len() < MAX_FOLDER_ENTRIES
                                && changes.path_bytes + bytes <= MAX_FOLDER_PATH_BYTES
                                && changes.paths.insert(path)
                            {
                                changes.path_bytes += bytes;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(error) => changes.error = Some(format!("Falló el observador: {error}")),
                }
            })
            .map_err(|error| RasteraError::new(ErrorKind::Io, error.to_string()))?;
        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|error| RasteraError::new(ErrorKind::Io, error.to_string()))?;
        Ok(Box::new(NativeWatch {
            _watcher: watcher,
            changes,
        }))
    }
    fn is_directory(&self, path: &Path) -> Result<bool> {
        fs::metadata(path)
            .map(|metadata| metadata.is_dir())
            .map_err(io_error)
    }

    fn scan(
        &self,
        path: &Path,
        cancel: &CancellationToken,
        emit: &mut dyn FnMut(Vec<FolderEntry>) -> Result<()>,
    ) -> Result<()> {
        let mut page = Vec::with_capacity(64);
        let mut count = 0;
        let mut path_bytes = 0;
        for entry in fs::read_dir(path).map_err(io_error)? {
            cancel.check()?;
            let entry = entry.map_err(io_error)?;
            // Non-recursive scan; links are deliberately excluded.
            if !entry.file_type().map_err(io_error)?.is_file() {
                continue;
            }
            let path = entry.path();
            let supported = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    ["jpg", "jpeg", "png", "webp"]
                        .iter()
                        .any(|kind| extension.eq_ignore_ascii_case(kind))
                });
            if !supported {
                continue;
            }
            path_bytes += path.as_os_str().as_encoded_bytes().len();
            if count >= MAX_FOLDER_ENTRIES || path_bytes > MAX_FOLDER_PATH_BYTES {
                if !page.is_empty() {
                    emit(page)?;
                }
                return Err(RasteraError::new(
                    ErrorKind::TooLarge,
                    "Lectura parcial: se alcanzó el límite inicial de 10.000 archivos o 16 MiB de rutas",
                ));
            }
            let metadata = entry.metadata().map_err(io_error)?;
            page.push(FolderEntry {
                path,
                stamp: Some(FileStamp {
                    length: metadata.len(),
                    modified: metadata.modified().ok(),
                }),
            });
            count += 1;
            if page.len() == 64 {
                emit(std::mem::replace(&mut page, Vec::with_capacity(64)))?;
            }
        }
        cancel.check()?;
        if !page.is_empty() {
            emit(page)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_monitor_tracks_create_modify_rename_delete_and_directory_recovery() {
        use rastera_engine::{FolderMonitor, LibraryUpdate};
        use std::time::{Duration, Instant};
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("fotografías");
        fs::create_dir(&dir).unwrap();
        let monitor = FolderMonitor::new(Arc::new(LocalFolders), Arc::new(|| {}));
        monitor.request(42, dir.clone(), vec![]);
        let wait = |predicate: &dyn Fn(&LibraryUpdate) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                if let Some(event) = monitor.try_recv() {
                    assert_eq!(event.generation, 42);
                    if predicate(&event) {
                        return event;
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "filesystem change was not reconciled"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        let initial = wait(&|event| event.error.is_none());
        assert!(
            !initial.polling,
            "this native integration test needs filesystem notifications"
        );
        let path = dir.join("añadida.PNG");
        fs::write(&path, [1]).unwrap();
        let added = wait(&|event| {
            event
                .entries
                .as_ref()
                .is_some_and(|entries| entries.len() == 1)
        });
        assert_eq!(added.entries.unwrap()[0].stamp.as_ref().unwrap().length, 1);
        fs::write(&path, [1, 2, 3]).unwrap();
        let modified = wait(&|event| event.changed.contains(&path));
        assert_eq!(
            modified.entries.unwrap()[0].stamp.as_ref().unwrap().length,
            3
        );
        let renamed = dir.join("renombrada.png");
        fs::rename(&path, &renamed).unwrap();
        wait(&|event| {
            event
                .entries
                .as_ref()
                .is_some_and(|entries| entries.len() == 1 && entries[0].path == renamed)
        });
        fs::remove_file(&renamed).unwrap();
        wait(&|event| event.entries.as_ref().is_some_and(Vec::is_empty));
        fs::remove_dir(&dir).unwrap();
        wait(&|event| event.error.is_some());
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("recuperada.webp"), [5]).unwrap();
        let recovered = wait(&|event| {
            event.error.is_none()
                && event
                    .entries
                    .as_ref()
                    .is_some_and(|entries| entries.len() == 1)
        });
        assert!(!recovered.polling);
        // A burst must converge to the filesystem, regardless of event order or duplication.
        for index in 0..60 {
            fs::write(dir.join(format!("ráfaga-{index}.png")), [1, 2]).unwrap();
        }
        fs::write(dir.join("omitido.txt"), [3]).unwrap();
        fs::create_dir(dir.join("subcarpeta")).unwrap();
        fs::write(dir.join("subcarpeta/oculta.png"), [3]).unwrap();
        wait(&|event| {
            event
                .entries
                .as_ref()
                .is_some_and(|entries| entries.len() == 61)
        });
    }

    #[test]
    fn scans_progressively_without_recursion_or_unsupported_files() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..130 {
            fs::write(dir.path().join(format!("{index}.PNG")), []).unwrap();
        }
        fs::write(dir.path().join("ignore.txt"), []).unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/hidden.jpg"), []).unwrap();
        let mut pages = Vec::new();
        LocalFolders
            .scan(dir.path(), &CancellationToken::default(), &mut |page| {
                pages.push(page);
                Ok(())
            })
            .unwrap();
        assert_eq!(pages.iter().map(Vec::len).collect::<Vec<_>>(), [64, 64, 2]);
        assert!(
            pages
                .into_iter()
                .flatten()
                .all(|entry| entry.path.extension().unwrap() == "PNG")
        );
    }

    #[test]
    fn cancellation_stops_discovery_before_emitting_a_page() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.png"), []).unwrap();
        let cancel = CancellationToken::default();
        cancel.cancel();
        let error = LocalFolders
            .scan(dir.path(), &cancel, &mut |_| panic!("should not emit"))
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Cancelled);
    }
}
