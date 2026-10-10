use std::{fs, path::Path};

use rastera_core::{
    CancellationToken, ErrorKind, FolderBackend, FolderEntry, MAX_FOLDER_ENTRIES,
    MAX_FOLDER_PATH_BYTES, RasteraError, Result,
};

#[derive(Default)]
pub struct LocalFolders;

fn io_error(error: std::io::Error) -> RasteraError {
    RasteraError::new(
        ErrorKind::Io,
        format!("No se pudo acceder a la ruta: {error}"),
    )
}

impl FolderBackend for LocalFolders {
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
            page.push(FolderEntry { path });
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
