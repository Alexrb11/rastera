use crate::{
    LocalBatch,
    batch::{file_hash, journal_lock},
    catalog::path_key,
};
use rastera_core::{
    BatchBackend, CancellationToken, ErrorKind, ExportFormat, ExportPlan, ExportRecipe,
    RasteraError, RecoveryReport, Result,
};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn invalid(message: impl Into<String>) -> RasteraError {
    RasteraError::new(ErrorKind::Journal, message)
}
fn sql(error: rusqlite::Error) -> RasteraError {
    invalid(format!("No se pudo leer el diario: {error}"))
}
fn path_from_key(bytes: &[u8]) -> Result<PathBuf> {
    if bytes.len() > 65_536 || bytes.is_empty() {
        return Err(invalid("Ruta del diario fuera de rango"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        if !bytes.len().is_multiple_of(2) {
            return Err(invalid("Ruta UTF-16 incompleta"));
        }
        let units: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        if units.contains(&0) {
            return Err(invalid("Ruta con caracteres nulos"));
        }
        Ok(PathBuf::from(std::ffi::OsString::from_wide(&units)))
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStringExt;
        if bytes.contains(&0) {
            return Err(invalid("Ruta con caracteres nulos"));
        }
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec())))
    }
}

pub(crate) fn recover(journal: &Path, cancel: &CancellationToken) -> Result<RecoveryReport> {
    cancel.check()?;
    let journal = fs::canonicalize(journal).map_err(crate::images::io_error)?;
    if fs::metadata(&journal)
        .map_err(crate::images::io_error)?
        .len()
        > 128 * 1024 * 1024
    {
        return Err(invalid("Diario demasiado grande"));
    }
    let _lock = journal_lock(&journal)?;
    let db =
        Connection::open_with_flags(&journal, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(sql)?;
    let version: u32 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql)?;
    if version != 2 {
        return Err(invalid(
            "Solo se recuperan diarios de versión 2; los anteriores no contienen huellas verificables",
        ));
    }
    let (platform, format, edge, rotation, quality, background): (
        String,
        String,
        Option<u32>,
        u16,
        u8,
        Vec<u8>,
    ) = db
        .query_row(
            "SELECT platform,format,max_edge,rotation,quality,background FROM batch",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .map_err(sql)?;
    if platform != if cfg!(windows) { "windows" } else { "unix" } {
        return Err(invalid("El diario usa rutas de otro sistema operativo"));
    }
    let recipe = ExportRecipe {
        format: match format.as_str() {
            "png" => ExportFormat::Png,
            "jpg" => ExportFormat::Jpeg,
            "webp" => ExportFormat::WebP,
            _ => return Err(invalid("Formato de diario desconocido")),
        },
        max_edge: edge,
        rotation,
        jpeg_quality: quality,
        jpeg_background: background
            .try_into()
            .map_err(|_| invalid("Fondo JPEG inválido"))?,
    };
    recipe.validate()?;
    let root = journal.parent().unwrap();
    let mut report = RecoveryReport {
        journal: journal.clone(),
        total: 0,
        published: 0,
        conflicts: Vec::new(),
        pending: ExportPlan {
            items: Vec::new(),
            recipe: recipe.clone(),
        },
    };
    let mut query = db.prepare("SELECT source,destination,source_hash,output_hash,state FROM items ORDER BY id LIMIT 10001").map_err(sql)?;
    let mut rows = query.query([]).map_err(sql)?;
    while let Some(row) = rows.next().map_err(sql)? {
        cancel.check()?;
        report.total += 1;
        if report.total > 10_000 {
            return Err(invalid("El diario supera 10.000 entradas"));
        }
        let source = path_from_key(&row.get::<_, Vec<u8>>(0).map_err(sql)?)?;
        let destination = path_from_key(&row.get::<_, Vec<u8>>(1).map_err(sql)?)?;
        let source_hash: [u8; 32] = row
            .get::<_, Vec<u8>>(2)
            .map_err(sql)?
            .try_into()
            .map_err(|_| invalid("Huella del original inválida"))?;
        let output_hash: Option<Vec<u8>> = row.get(3).map_err(sql)?;
        let state: String = row.get(4).map_err(sql)?;
        // Only inspect output files within the journal directory, never journal-supplied temporaries.
        if !source.is_absolute() || !destination.is_absolute() || destination.parent() != Some(root)
        {
            return Err(invalid(
                "Las salidas del diario deben estar en su misma carpeta",
            ));
        }
        match fs::symlink_metadata(&destination) {
            Ok(meta) => {
                if meta.is_file()
                    && !meta.is_symlink()
                    && matches!(state.as_str(), "ready" | "published")
                    && let Some(hash) = &output_hash
                    && hash.len() == 32
                    && file_hash(&destination, cancel)?.as_slice() == hash
                {
                    report.published += 1;
                } else {
                    report.conflicts.push(format!(
                        "{}: salida existente sin identidad comprobada",
                        destination.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let pending = (|| -> Result<_> {
                    if file_hash(&source, cancel)? != source_hash {
                        return Err(RasteraError::new(ErrorKind::Changed, "El original cambió"));
                    }
                    let mut plan = LocalBatch.plan(
                        std::slice::from_ref(&source),
                        root,
                        recipe.clone(),
                        cancel,
                    )?;
                    let item = plan.items.remove(0);
                    if path_key(&item.destination) != path_key(&destination)
                        || item.source_hash != source_hash
                    {
                        return Err(invalid(
                            "El nombre de salida o el original no coincide con el diario",
                        ));
                    }
                    Ok(item)
                })();
                match pending {
                    Ok(item) => report.pending.items.push(item),
                    Err(error) if error.kind == ErrorKind::Cancelled => return Err(error),
                    Err(error) => report
                        .conflicts
                        .push(format!("{}: {error}", source.display())),
                }
            }
            Err(error) => report
                .conflicts
                .push(format!("{}: {error}", destination.display())),
        }
    }
    if report.total == 0 {
        return Err(invalid("Diario sin entradas"));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn interrupted() -> (tempfile::TempDir, ExportPlan, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("output");
        fs::create_dir(&output).unwrap();
        let sources: Vec<_> = ["uno.png", "dos.png"]
            .iter()
            .map(|name| {
                let source = dir.path().join(name);
                image::RgbaImage::from_pixel(12, 6, image::Rgba([30, 70, 120, 128]))
                    .save(&source)
                    .unwrap();
                source
            })
            .collect();
        let plan = LocalBatch
            .plan(
                &sources,
                &output,
                ExportRecipe {
                    rotation: 90,
                    max_edge: Some(8),
                    ..Default::default()
                },
                &CancellationToken::default(),
            )
            .unwrap();
        let mut session = LocalBatch.start(&plan).unwrap();
        let journal = session.journal_path().to_path_buf();
        session
            .export(
                0,
                &plan.items[0],
                &plan.recipe,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(
            LocalBatch
                .recover(&journal, &CancellationToken::default())
                .unwrap_err()
                .kind,
            ErrorKind::Busy
        );
        // Model the crash window after publication but before durable acknowledgment.
        let db = Connection::open(&journal).unwrap();
        db.execute("UPDATE items SET state='ready' WHERE id=0", [])
            .unwrap();
        drop(session);
        (dir, plan, journal)
    }
    #[test]
    fn verifies_ready_output_and_resumes_only_missing_copies_in_a_new_journal() {
        let (_dir, plan, journal) = interrupted();
        let bytes = fs::read(&journal).unwrap();
        let first = fs::read(&plan.items[0].destination).unwrap();
        let report = LocalBatch
            .recover(&journal, &CancellationToken::default())
            .unwrap();
        assert_eq!(
            (report.total, report.published, report.pending.items.len()),
            (2, 1, 1)
        );
        assert!(report.conflicts.is_empty());
        assert_eq!(report.pending.recipe, plan.recipe);
        assert_eq!(
            report.pending.items[0].source_hash,
            plan.items[1].source_hash
        );
        assert_eq!(fs::read(&journal).unwrap(), bytes);
        let mut session = LocalBatch.start(&report.pending).unwrap();
        assert_ne!(session.journal_path(), journal);
        session
            .export(
                0,
                &report.pending.items[0],
                &report.pending.recipe,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(fs::read(&plan.items[0].destination).unwrap(), first);
        assert_eq!(
            image::image_dimensions(&plan.items[1].destination).unwrap(),
            (4, 8)
        );
    }
    #[test]
    fn preserves_foreign_outputs_and_rejects_changed_sources_even_with_same_stamp() {
        let (_dir, plan, journal) = interrupted();
        fs::write(&plan.items[0].destination, "foreign").unwrap();
        let path = &plan.items[1].source;
        let original_stamp = fs::metadata(path).unwrap().modified().unwrap();
        let mut bytes = fs::read(path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(path, bytes).unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(original_stamp))
            .unwrap();
        let report = LocalBatch
            .recover(&journal, &CancellationToken::default())
            .unwrap();
        assert_eq!(report.conflicts.len(), 2);
        assert_eq!(report.published, 0);
        assert!(report.pending.items.is_empty());
        assert_eq!(fs::read(&plan.items[0].destination).unwrap(), b"foreign");
        let mut session = LocalBatch.start(&plan).unwrap();
        assert_eq!(
            session
                .export(
                    1,
                    &plan.items[1],
                    &plan.recipe,
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            ErrorKind::Changed
        );
    }
    #[test]
    fn rejects_legacy_unsupported_paths_and_cancellation_without_rewriting_journal() {
        let (_dir, plan, journal) = interrupted();
        let cancel = CancellationToken::default();
        cancel.cancel();
        assert_eq!(
            LocalBatch.recover(&journal, &cancel).unwrap_err().kind,
            ErrorKind::Cancelled
        );
        let db = Connection::open(&journal).unwrap();
        db.execute(
            "UPDATE items SET destination=?1 WHERE id=1",
            [path_key(&plan.items[1].source)],
        )
        .unwrap();
        assert_eq!(
            LocalBatch
                .recover(&journal, &CancellationToken::default())
                .unwrap_err()
                .kind,
            ErrorKind::Journal
        );
        db.pragma_update(None, "user_version", 1).unwrap();
        drop(db);
        let bytes = fs::read(&journal).unwrap();
        assert_eq!(
            LocalBatch
                .recover(&journal, &CancellationToken::default())
                .unwrap_err()
                .kind,
            ErrorKind::Journal
        );
        assert_eq!(fs::read(&journal).unwrap(), bytes);
    }
}
