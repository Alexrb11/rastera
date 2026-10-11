use rastera_core::{
    Annotation, CatalogBackend, CatalogEdit, ErrorKind, ImageInfo, RasteraError, Result,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

fn db_error(error: rusqlite::Error) -> RasteraError {
    RasteraError::new(
        ErrorKind::Io,
        format!("No se pudo acceder al catálogo: {error}"),
    )
}

// Lossless path keys; display text is never used as file identity.
#[cfg(windows)]
pub(crate) fn path_key(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}
#[cfg(not(windows))]
pub(crate) fn path_key(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

pub struct SqliteCatalog(Connection);

impl SqliteCatalog {
    pub fn open(path: &Path) -> Result<Self> {
        let mut db = Connection::open(path).map_err(db_error)?;
        db.busy_timeout(Duration::from_secs(2)).map_err(db_error)?;
        let version: u32 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(db_error)?;
        if version > 1 {
            return Err(RasteraError::new(
                ErrorKind::Unsupported,
                "El catálogo pertenece a una versión más reciente de Rastera",
            ));
        }
        if version == 0 {
            // SQLite's online backup also captures committed pages in a WAL.
            let tables: u32 = db
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if tables != 0 {
                let backup = path.with_extension(format!(
                    "backup-{}.sqlite",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                ));
                db.backup("main", backup, None).map_err(db_error)?;
            }
            let tx = db.transaction().map_err(db_error)?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS images (
                path BLOB PRIMARY KEY, favorite INTEGER NOT NULL DEFAULT 0,
                tags TEXT NOT NULL DEFAULT '', width INTEGER, height INTEGER, format TEXT
            ); PRAGMA user_version=1;",
            )
            .map_err(db_error)?;
            tx.commit().map_err(db_error)?;
        }
        db.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_error)?;
        db.pragma_update(None, "synchronous", "FULL")
            .map_err(db_error)?;
        Ok(Self(db))
    }
}

impl CatalogBackend for SqliteCatalog {
    fn load(&mut self, paths: &[PathBuf]) -> Result<Vec<(PathBuf, Annotation)>> {
        let mut query = self
            .0
            .prepare_cached("SELECT favorite,tags,width,height,format FROM images WHERE path=?1")
            .map_err(db_error)?;
        let mut result = Vec::new();
        for path in paths {
            let annotation = query
                .query_row([path_key(path)], |row| {
                    let tags: String = row.get(1)?;
                    Ok(Annotation {
                        favorite: row.get(0)?,
                        tags: tags.lines().map(str::to_owned).collect(),
                        width: row.get(2)?,
                        height: row.get(3)?,
                        format: row.get(4)?,
                    })
                })
                .optional()
                .map_err(db_error)?;
            if let Some(annotation) = annotation {
                result.push((path.clone(), annotation));
            }
        }
        Ok(result)
    }

    fn save(
        &mut self,
        path: &Path,
        annotation: &Annotation,
        info: Option<&ImageInfo>,
    ) -> Result<()> {
        self.0.execute("INSERT INTO images(path,favorite,tags,width,height,format) VALUES(?1,?2,?3,?4,?5,?6)
            ON CONFLICT(path) DO UPDATE SET favorite=excluded.favorite,tags=excluded.tags,
            width=coalesce(excluded.width,images.width),height=coalesce(excluded.height,images.height),
            format=coalesce(excluded.format,images.format)",
            params![path_key(path), annotation.favorite, annotation.tags.join("\n"),
                info.map(|info| info.width), info.map(|info| info.height), info.map(|info| info.kind.to_string())])
            .map_err(db_error)?;
        Ok(())
    }

    fn edit(
        &mut self,
        paths: &[PathBuf],
        edit: &CatalogEdit,
    ) -> Result<Vec<(PathBuf, Annotation)>> {
        if paths.is_empty() || paths.len() > 10_000 {
            return Err(RasteraError::new(
                ErrorKind::TooLarge,
                "Selecciona entre 1 y 10.000 imágenes",
            ));
        }
        let tx = self
            .0
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let mut seen = std::collections::HashSet::new();
        let mut records = Vec::new();
        {
            let mut query = tx
                .prepare_cached(
                    "SELECT favorite,tags,width,height,format FROM images WHERE path=?1",
                )
                .map_err(db_error)?;
            let mut write = tx.prepare_cached("INSERT INTO images(path,favorite,tags) VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET favorite=excluded.favorite,tags=excluded.tags").map_err(db_error)?;
            for path in paths {
                let key = path_key(path);
                if !seen.insert(key.clone()) {
                    continue;
                }
                let annotation = query
                    .query_row([&key], |row| {
                        let tags: String = row.get(1)?;
                        Ok(Annotation {
                            favorite: row.get(0)?,
                            tags: tags.lines().map(str::to_owned).collect(),
                            width: row.get(2)?,
                            height: row.get(3)?,
                            format: row.get(4)?,
                        })
                    })
                    .optional()
                    .map_err(db_error)?
                    .unwrap_or_default();
                let updated = edit.apply(&annotation).map_err(|error| {
                    RasteraError::new(
                        error.kind,
                        format!(
                            "{}: {}",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            error.message
                        ),
                    )
                })?;
                write
                    .execute(params![key, updated.favorite, updated.tags.join("\n")])
                    .map_err(db_error)?;
                records.push((path.clone(), updated));
            }
        }
        tx.commit().map_err(db_error)?;
        Ok(records)
    }
}

pub fn catalog_path() -> Result<PathBuf> {
    crate::platform::catalog_path()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_edits_persist_per_image_tags_metadata_and_unicode_paths() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("catalog.sqlite");
        let first = dir.path().join("cerámica-猫.png");
        let second = dir.path().join("paisaje.jpg");
        let mut db = SqliteCatalog::open(&db_path).unwrap();
        let info = ImageInfo {
            width: 80,
            height: 40,
            kind: rastera_core::ImageKind::Png,
            stamp: rastera_core::FileStamp {
                length: 10,
                modified: None,
            },
            has_color_profile: false,
        };
        db.save(
            &first,
            &Annotation {
                tags: vec!["cerámica".into()],
                ..Default::default()
            },
            Some(&info),
        )
        .unwrap();
        db.save(
            &second,
            &Annotation {
                tags: vec!["viaje".into()],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let paths = [first.clone(), second.clone(), first.clone()];
        let records = db
            .edit(
                &paths,
                &CatalogEdit {
                    favorite: Some(true),
                    tags: vec!["aprobada".into()],
                    remove_tags: false,
                },
            )
            .unwrap();
        assert_eq!(records.len(), 2);
        drop(db);
        let mut db = SqliteCatalog::open(&db_path).unwrap();
        let records = db.load(&[first.clone(), second]).unwrap();
        assert_eq!(records[0].1.tags, ["aprobada", "cerámica"]);
        assert_eq!(records[1].1.tags, ["aprobada", "viaje"]);
        assert!(records.iter().all(|record| record.1.favorite));
        assert_eq!(
            (records[0].1.width, records[0].1.height),
            (Some(80), Some(40))
        );
        db.edit(
            std::slice::from_ref(&first),
            &CatalogEdit {
                favorite: None,
                tags: vec!["aprobada".into()],
                remove_tags: true,
            },
        )
        .unwrap();
        assert_eq!(db.load(&[first]).unwrap()[0].1.tags, ["cerámica"]);
    }
    #[test]
    fn failed_selection_edit_rolls_back_all_prior_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = SqliteCatalog::open(&dir.path().join("catalog.sqlite")).unwrap();
        let paths = [dir.path().join("new.png"), dir.path().join("full.png")];
        let full = Annotation {
            tags: (0..32).map(|index| format!("tag-{index}")).collect(),
            ..Default::default()
        };
        db.save(&paths[1], &full, None).unwrap();
        assert!(
            db.edit(
                &paths,
                &CatalogEdit {
                    favorite: Some(true),
                    tags: vec!["extra".into()],
                    remove_tags: false
                }
            )
            .is_err()
        );
        assert_eq!(
            db.load(&paths).unwrap(),
            vec![(paths[1].clone(), full.clone())]
        );
        db.0.execute_batch("CREATE TRIGGER reject_updates BEFORE UPDATE ON images BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(
            db.edit(
                &paths,
                &CatalogEdit {
                    favorite: Some(true),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(db.load(&paths).unwrap(), vec![(paths[1].clone(), full)]);
    }
    #[test]
    fn annotations_survive_restart_and_metadata_updates_preserve_tags() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("catalog.sqlite");
        let path = dir.path().join("Álbum 日本.png");
        let annotation = Annotation {
            favorite: true,
            tags: vec!["Viajes".into(), "Familia".into()],
            ..Default::default()
        };
        {
            let mut db = SqliteCatalog::open(&db_path).unwrap();
            db.save(&path, &annotation, None).unwrap();
        }
        let mut db = SqliteCatalog::open(&db_path).unwrap();
        assert_eq!(
            db.load(std::slice::from_ref(&path)).unwrap()[0].1,
            annotation
        );
        assert!(
            db.load(&[dir.path().join("ausente.png")])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn newer_schema_is_not_modified() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.sqlite");
        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "user_version", 99).unwrap();
        drop(db);
        assert!(SqliteCatalog::open(&path).is_err());
        assert_eq!(
            Connection::open(path)
                .unwrap()
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            99
        );
    }

    #[test]
    fn storing_dimensions_keeps_existing_organization() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        let db_path = dir.path().join("catalog.sqlite");
        let annotation = Annotation {
            favorite: true,
            tags: vec!["Viajes".into()],
            ..Default::default()
        };
        let info = ImageInfo {
            width: 42,
            height: 24,
            kind: rastera_core::ImageKind::Png,
            stamp: rastera_core::FileStamp {
                length: 100,
                modified: None,
            },
            has_color_profile: false,
        };
        let mut db = SqliteCatalog::open(&db_path).unwrap();
        db.save(&path, &annotation, Some(&info)).unwrap();
        drop(db);
        let mut db = SqliteCatalog::open(&db_path).unwrap();
        let updated = db.load(&[path]).unwrap().remove(0).1;
        assert!(updated.favorite);
        assert_eq!(updated.tags, annotation.tags);
        assert_eq!((updated.width, updated.height), (Some(42), Some(24)));
        assert_eq!(updated.format.as_deref(), Some("PNG"));
    }

    #[test]
    fn migration_backs_up_an_existing_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.sqlite");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE previous(value TEXT); INSERT INTO previous VALUES('keep');")
            .unwrap();
        drop(db);
        SqliteCatalog::open(&path).unwrap();
        let backup = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().contains("backup-"))
            .unwrap();
        let value: String = Connection::open(backup.path())
            .unwrap()
            .query_row("SELECT value FROM previous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "keep");
    }
}
