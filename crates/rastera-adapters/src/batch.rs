use crate::{
    catalog::path_key,
    images::{decoder, image_error, io_error, stamp},
};
use image::{DynamicImage, ImageEncoder, metadata::Orientation};
use rastera_core::{
    BatchBackend, CPU_IMAGE_BUDGET, CancellationToken, ErrorKind, ExportFormat, ExportItem,
    ExportPlan, ExportRecipe, ExportSession, ImageInfo, ImageKind, RasteraError, RecoveryReport,
    Result,
};
use rusqlite::{Connection, params};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

pub struct LocalBatch;

pub(crate) fn file_hash(path: &Path, cancel: &CancellationToken) -> Result<[u8; 32]> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        cancel.check()?;
        let length = file.read(&mut buffer).map_err(io_error)?;
        if length == 0 {
            break;
        }
        total += length as u64;
        if total > 128 * 1024 * 1024 {
            return Err(RasteraError::new(
                ErrorKind::TooLarge,
                "El archivo supera 128 MiB",
            ));
        }
        hasher.update(&buffer[..length]);
    }
    Ok(*hasher.finalize().as_bytes())
}
pub(crate) fn journal_lock(path: &Path) -> Result<File> {
    let mut lock_name = path.as_os_str().to_os_string();
    lock_name.push(".lock");
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(lock_name))
        .map_err(io_error)?;
    lock.try_lock().map_err(|error| {
        RasteraError::new(
            ErrorKind::Busy,
            format!("El diario está ocupado o no puede bloquearse: {error}"),
        )
    })?;
    Ok(lock)
}

fn journal_error(error: rusqlite::Error) -> RasteraError {
    RasteraError::new(
        ErrorKind::Journal,
        format!("Error del diario de exportación: {error}"),
    )
}
fn ensure_export_color(
    path: &Path,
    info: &ImageInfo,
    decoder: &dyn image::ImageDecoder,
) -> Result<()> {
    let color = decoder.color_type();
    if info.has_color_profile
        || !matches!(
            color,
            image::ColorType::L8
                | image::ColorType::La8
                | image::ColorType::Rgb8
                | image::ColorType::Rgba8
        )
    {
        return Err(RasteraError::new(
            ErrorKind::Unsupported,
            "Los lotes admiten imágenes de 8 bits sin perfil ICC; la gestión de color está pendiente",
        ));
    }
    if info.kind == ImageKind::Png {
        let png = png::Decoder::new_with_limits(
            BufReader::new(File::open(path).map_err(io_error)?),
            png::Limits {
                bytes: 16 * 1024 * 1024,
            },
        );
        let reader = png
            .read_info()
            .map_err(|error| RasteraError::new(ErrorKind::InvalidImage, error.to_string()))?;
        let header = reader.info();
        if header.coding_independent_code_points.is_some()
            || (header.srgb.is_none()
                && (header.chrm_chunk.is_some()
                    || header
                        .gama_chunk
                        .is_some_and(|gamma| (gamma.into_value() - 0.45455).abs() > 0.0001)))
        {
            return Err(RasteraError::new(
                ErrorKind::Unsupported,
                "Este PNG declara un espacio de color que los lotes todavía no pueden convertir",
            ));
        }
    }
    Ok(())
}

impl BatchBackend for LocalBatch {
    fn recover(&self, journal: &Path, cancel: &CancellationToken) -> Result<RecoveryReport> {
        crate::recovery::recover(journal, cancel)
    }
    fn plan(
        &self,
        sources: &[PathBuf],
        destination: &Path,
        recipe: ExportRecipe,
        cancel: &CancellationToken,
    ) -> Result<ExportPlan> {
        recipe.validate()?;
        if sources.is_empty() || sources.len() > 10_000 {
            return Err(RasteraError::new(
                ErrorKind::TooLarge,
                "Selecciona entre 1 y 10.000 imágenes",
            ));
        }
        let destination = fs::canonicalize(destination).map_err(io_error)?;
        if !destination.is_dir() {
            return Err(RasteraError::new(
                ErrorKind::Io,
                "El destino debe ser una carpeta existente",
            ));
        }
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        for source in sources {
            cancel.check()?;
            let source = fs::canonicalize(source).map_err(io_error)?;
            if source.parent() == Some(destination.as_path()) {
                return Err(RasteraError::new(
                    ErrorKind::Unsupported,
                    "Elige una carpeta distinta de la de los originales",
                ));
            }
            let (info, mut decoder) = decoder(&source)?;
            ensure_export_color(&source, &info, decoder.as_ref())?;
            if info.working_bytes()?.saturating_mul(2) > CPU_IMAGE_BUDGET {
                return Err(RasteraError::new(
                    ErrorKind::TooLarge,
                    "Una imagen supera el presupuesto de exportación (32 bytes estimados por píxel)",
                ));
            }
            let orientation = decoder.orientation().map_err(image_error)?;
            let (mut width, mut height) = (info.width, info.height);
            if matches!(
                orientation,
                Orientation::Rotate90
                    | Orientation::Rotate270
                    | Orientation::Rotate90FlipH
                    | Orientation::Rotate270FlipH
            ) {
                std::mem::swap(&mut width, &mut height);
            }
            let mut name = source.file_stem().unwrap_or_default().to_os_string();
            name.push("-rastera.");
            name.push(recipe.format.extension());
            // Conservative case-insensitive check also on case-sensitive filesystems.
            if !seen.insert(name.to_string_lossy().to_lowercase()) {
                return Err(RasteraError::new(
                    ErrorKind::Changed,
                    "Dos entradas producirían el mismo nombre de salida",
                ));
            }
            let output = destination.join(name);
            match fs::symlink_metadata(&output) {
                Ok(_) => {
                    return Err(RasteraError::new(
                        ErrorKind::Changed,
                        "Ya existe una salida del lote en el destino",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
            let source_hash = file_hash(&source, cancel)?;
            if stamp(fs::metadata(&source).map_err(io_error)?) != info.stamp {
                return Err(RasteraError::new(
                    ErrorKind::Changed,
                    "El original cambió durante la revisión",
                ));
            }
            items.push(ExportItem {
                source_hash,
                source,
                destination: output,
                output_size: recipe.dimensions(width, height),
                info,
            });
        }
        Ok(ExportPlan { items, recipe })
    }

    fn start(&self, plan: &ExportPlan) -> Result<Box<dyn ExportSession>> {
        let parent = plan
            .items
            .first()
            .ok_or_else(|| RasteraError::new(ErrorKind::Internal, "Plan vacío"))?
            .destination
            .parent()
            .unwrap();
        let journal = tempfile::Builder::new()
            .prefix("rastera-export-")
            .suffix(".sqlite")
            .tempfile_in(parent)
            .map_err(io_error)?;
        let (_, path) = journal.keep().map_err(|error| io_error(error.error))?;
        let lock = journal_lock(&path)?;
        let mut db = Connection::open(&path).map_err(journal_error)?;
        db.execute_batch("PRAGMA synchronous=FULL;
            CREATE TABLE batch(state TEXT NOT NULL, recipe TEXT NOT NULL,
                platform TEXT NOT NULL, format TEXT NOT NULL, max_edge INTEGER,
                rotation INTEGER NOT NULL, quality INTEGER NOT NULL, background BLOB NOT NULL);
            CREATE TABLE items(id INTEGER PRIMARY KEY, source BLOB NOT NULL, destination BLOB NOT NULL,
                source_text TEXT NOT NULL, destination_text TEXT NOT NULL, temporary BLOB,
                length INTEGER NOT NULL, modified TEXT, state TEXT NOT NULL, message TEXT NOT NULL DEFAULT '',
                source_hash BLOB NOT NULL, output_hash BLOB);
            PRAGMA user_version=2;").map_err(journal_error)?;
        let tx = db.transaction().map_err(journal_error)?;
        tx.execute(
            "INSERT INTO batch VALUES('running',?1,?2,?3,?4,?5,?6,?7)",
            params![
                format!("{:?}", plan.recipe),
                if cfg!(windows) { "windows" } else { "unix" },
                plan.recipe.format.extension(),
                plan.recipe.max_edge,
                plan.recipe.rotation,
                plan.recipe.jpeg_quality,
                plan.recipe.jpeg_background.as_slice()
            ],
        )
        .map_err(journal_error)?;
        for (index, item) in plan.items.iter().enumerate() {
            tx.execute("INSERT INTO items(id,source,destination,source_text,destination_text,length,modified,source_hash,state) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending')",
                params![index as i64, path_key(&item.source), path_key(&item.destination),
                    item.source.to_string_lossy(), item.destination.to_string_lossy(),
                    item.info.stamp.length as i64, format!("{:?}", item.info.stamp.modified),
                    item.source_hash.as_slice()]).map_err(journal_error)?;
        }
        tx.commit().map_err(journal_error)?;
        Ok(Box::new(LocalExportSession {
            db,
            path,
            _lock: lock,
        }))
    }
}

struct LocalExportSession {
    db: Connection,
    path: PathBuf,
    _lock: File,
}
impl ExportSession for LocalExportSession {
    fn journal_path(&self) -> &Path {
        &self.path
    }
    fn export(
        &mut self,
        index: usize,
        item: &ExportItem,
        recipe: &ExportRecipe,
        cancel: &CancellationToken,
    ) -> Result<()> {
        recipe.validate()?;
        cancel.check()?;
        if stamp(fs::metadata(&item.source).map_err(io_error)?) != item.info.stamp {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El original cambió después de revisar el plan",
            ));
        }
        if file_hash(&item.source, cancel)? != item.source_hash {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El contenido del original cambió después de revisar el plan",
            ));
        }
        let (info, mut decoder) = decoder(&item.source)?;
        if info.stamp != item.info.stamp {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El original cambió después de revisar el plan",
            ));
        }
        ensure_export_color(&item.source, &info, decoder.as_ref())?;
        let orientation = decoder.orientation().map_err(image_error)?;
        let mut image = DynamicImage::from_decoder(decoder).map_err(image_error)?;
        cancel.check()?;
        image.apply_orientation(orientation);
        image = match recipe.rotation {
            90 => image.rotate90(),
            180 => image.rotate180(),
            270 => image.rotate270(),
            _ => image,
        };
        cancel.check()?;
        if let Some(edge) = recipe.max_edge
            && image.width().max(image.height()) > edge
        {
            image = image.resize(edge, edge, image::imageops::FilterType::Triangle);
        }
        cancel.check()?;
        let image = image.into_rgba8();
        if (image.width(), image.height()) != item.output_size {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "Las dimensiones no coinciden con el plan",
            ));
        }
        let mut temporary = tempfile::Builder::new()
            .prefix(".rastera-")
            .suffix(".tmp")
            .tempfile_in(item.destination.parent().unwrap())
            .map_err(io_error)?;
        self.db
            .execute(
                "UPDATE items SET state='encoding',temporary=?1 WHERE id=?2",
                params![path_key(temporary.path()), index as i64],
            )
            .map_err(journal_error)?;
        {
            let mut writer = BufWriter::new(temporary.as_file_mut());
            match recipe.format {
                ExportFormat::Png => image::codecs::png::PngEncoder::new(&mut writer)
                    .write_image(
                        image.as_raw(),
                        image.width(),
                        image.height(),
                        image::ExtendedColorType::Rgba8,
                    )
                    .map_err(image_error)?,
                ExportFormat::WebP => image::codecs::webp::WebPEncoder::new_lossless(&mut writer)
                    .write_image(
                        image.as_raw(),
                        image.width(),
                        image.height(),
                        image::ExtendedColorType::Rgba8,
                    )
                    .map_err(image_error)?,
                ExportFormat::Jpeg => {
                    let rgb = image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
                        let pixel = image.get_pixel(x, y).0;
                        let alpha = u32::from(pixel[3]);
                        image::Rgb(std::array::from_fn(|channel| {
                            ((u32::from(pixel[channel]) * alpha
                                + u32::from(recipe.jpeg_background[channel]) * (255 - alpha)
                                + 127)
                                / 255) as u8
                        }))
                    });
                    image::codecs::jpeg::JpegEncoder::new_with_quality(
                        &mut writer,
                        recipe.jpeg_quality,
                    )
                    .encode_image(&rgb)
                    .map_err(image_error)?;
                }
            }
            writer.flush().map_err(io_error)?;
        }
        temporary.as_file().sync_all().map_err(io_error)?;
        cancel.check()?;
        if stamp(fs::metadata(&item.source).map_err(io_error)?) != item.info.stamp {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El original cambió durante la exportación",
            ));
        }
        if file_hash(&item.source, cancel)? != item.source_hash {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El contenido del original cambió durante la exportación",
            ));
        }
        let output_hash = file_hash(temporary.path(), cancel)?;
        self.db
            .execute(
                "UPDATE items SET output_hash=?1 WHERE id=?2",
                params![output_hash.as_slice(), index as i64],
            )
            .map_err(journal_error)?;
        self.record(index, "ready", "")?;
        temporary
            .persist_noclobber(&item.destination)
            .map_err(|error| io_error(error.error))?;
        // Publication may succeed before this update; recovery must reconcile a 'ready' item.
        self.record(index, "published", "").map_err(|error| {
            RasteraError::new(
                ErrorKind::Published,
                format!("La salida se publicó, pero no se pudo actualizar el diario: {error}"),
            )
        })?;
        Ok(())
    }
    fn record(&mut self, index: usize, state: &str, message: &str) -> Result<()> {
        self.db
            .execute(
                "UPDATE items SET state=?1,message=?2 WHERE id=?3",
                params![state, message, index as i64],
            )
            .map_err(journal_error)?;
        Ok(())
    }
    fn finish(&mut self, cancelled: bool) -> Result<()> {
        self.db
            .execute(
                "UPDATE batch SET state=?1",
                [if cancelled { "cancelled" } else { "finished" }],
            )
            .map_err(journal_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("original.png");
        let output = dir.path().join("output");
        fs::create_dir(&output).unwrap();
        image::RgbaImage::from_pixel(80, 40, image::Rgba([40, 90, 160, 128]))
            .save(&source)
            .unwrap();
        (dir, source, output)
    }

    #[test]
    fn exports_all_formats_with_rotation_resize_alpha_and_journal() {
        let (_dir, source, output) = fixture();
        let original = fs::read(&source).unwrap();
        for format in [ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::WebP] {
            let recipe = ExportRecipe {
                format,
                rotation: 90,
                max_edge: Some(20),
                jpeg_background: [200, 100, 50],
                jpeg_quality: 100,
            };
            let plan = LocalBatch
                .plan(
                    std::slice::from_ref(&source),
                    &output,
                    recipe,
                    &CancellationToken::default(),
                )
                .unwrap();
            assert_eq!(plan.items[0].output_size, (10, 20));
            let mut session = LocalBatch.start(&plan).unwrap();
            session
                .export(
                    0,
                    &plan.items[0],
                    &plan.recipe,
                    &CancellationToken::default(),
                )
                .unwrap();
            session.finish(false).unwrap();
            let result = image::open(&plan.items[0].destination)
                .unwrap()
                .into_rgba8();
            assert_eq!(result.dimensions(), (10, 20));
            let pixel = result.get_pixel(5, 5).0;
            if format == ExportFormat::Jpeg {
                let expected = [120i16, 95, 105];
                for channel in 0..3 {
                    assert!((i16::from(pixel[channel]) - expected[channel]).abs() <= 4);
                }
                assert_eq!(pixel[3], 255);
            } else {
                assert_eq!(pixel, [40, 90, 160, 128]);
            }
            let db = Connection::open(session.journal_path()).unwrap();
            let state: String = db
                .query_row("SELECT state FROM items WHERE id=0", [], |row| row.get(0))
                .unwrap();
            assert_eq!(state, "published");
            assert_eq!(
                db.query_row("SELECT state FROM batch", [], |row| row.get::<_, String>(0))
                    .unwrap(),
                "finished"
            );
        }
        assert_eq!(fs::read(source).unwrap(), original);
    }

    #[test]
    fn full_resolution_export_does_not_use_the_viewer_preview() {
        let (_dir, source, output) = fixture();
        image::RgbaImage::from_pixel(5000, 2, image::Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let plan = LocalBatch
            .plan(
                &[source],
                &output,
                ExportRecipe {
                    max_edge: None,
                    ..Default::default()
                },
                &CancellationToken::default(),
            )
            .unwrap();
        let mut session = LocalBatch.start(&plan).unwrap();
        session
            .export(
                0,
                &plan.items[0],
                &plan.recipe,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(
            image::image_dimensions(&plan.items[0].destination).unwrap(),
            (5000, 2)
        );
    }

    #[test]
    fn rejects_original_folder_duplicate_names_existing_outputs_and_racing_collisions() {
        let (dir, source, output) = fixture();
        assert!(
            LocalBatch
                .plan(
                    std::slice::from_ref(&source),
                    dir.path(),
                    ExportRecipe::default(),
                    &CancellationToken::default()
                )
                .is_err()
        );
        let other = dir.path().join("original.jpg");
        image::RgbImage::from_pixel(10, 10, image::Rgb([0, 0, 0]))
            .save(&other)
            .unwrap();
        assert!(
            LocalBatch
                .plan(
                    &[source.clone(), other],
                    &output,
                    ExportRecipe::default(),
                    &CancellationToken::default()
                )
                .is_err()
        );
        let plan = LocalBatch
            .plan(
                std::slice::from_ref(&source),
                &output,
                ExportRecipe::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        fs::write(&plan.items[0].destination, b"do not replace").unwrap();
        assert!(
            LocalBatch
                .plan(
                    &[source],
                    &output,
                    ExportRecipe::default(),
                    &CancellationToken::default()
                )
                .is_err()
        );
        let mut session = LocalBatch.start(&plan).unwrap();
        assert!(
            session
                .export(
                    0,
                    &plan.items[0],
                    &plan.recipe,
                    &CancellationToken::default()
                )
                .is_err()
        );
        assert_eq!(
            fs::read(&plan.items[0].destination).unwrap(),
            b"do not replace"
        );
        assert!(
            !fs::read_dir(output)
                .unwrap()
                .filter_map(std::result::Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        );
    }

    #[test]
    fn changed_and_cancelled_inputs_publish_nothing() {
        let (_dir, source, output) = fixture();
        let plan = LocalBatch
            .plan(
                std::slice::from_ref(&source),
                &output,
                ExportRecipe::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        let mut session = LocalBatch.start(&plan).unwrap();
        let cancel = CancellationToken::default();
        cancel.cancel();
        assert_eq!(
            session
                .export(0, &plan.items[0], &plan.recipe, &cancel)
                .unwrap_err()
                .kind,
            ErrorKind::Cancelled
        );
        fs::write(source, b"changed").unwrap();
        assert_eq!(
            session
                .export(
                    0,
                    &plan.items[0],
                    &plan.recipe,
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            ErrorKind::Changed
        );
        assert!(!plan.items[0].destination.exists());
    }

    #[test]
    fn refuses_high_bit_depth_icc_and_custom_png_gamma() {
        let (_dir, source, output) = fixture();
        image::ImageBuffer::<image::Rgba<u16>, _>::from_pixel(
            2,
            2,
            image::Rgba([10u16, 20, 30, 65535]),
        )
        .save(&source)
        .unwrap();
        assert_eq!(
            LocalBatch
                .plan(
                    std::slice::from_ref(&source),
                    &output,
                    ExportRecipe::default(),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
        {
            let mut encoder = image::codecs::png::PngEncoder::new(File::create(&source).unwrap());
            encoder.set_icc_profile(vec![1; 128]).unwrap();
            encoder
                .write_image(&[1, 2, 3, 255], 1, 1, image::ExtendedColorType::Rgba8)
                .unwrap();
        }
        assert_eq!(
            LocalBatch
                .plan(
                    std::slice::from_ref(&source),
                    &output,
                    ExportRecipe::default(),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
        {
            let mut encoder = png::Encoder::new(File::create(&source).unwrap(), 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_source_gamma(png::ScaledFloat::new(1.0));
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[1, 2, 3])
                .unwrap();
        }
        assert_eq!(
            LocalBatch
                .plan(
                    &[source],
                    &output,
                    ExportRecipe::default(),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
    }

    #[test]
    fn exif_orientation_is_normalized_once_before_recipe_rotation() {
        let (dir, _, output) = fixture();
        let source = dir.path().join("oriented.jpg");
        image::RgbImage::from_pixel(30, 20, image::Rgb([90, 80, 70]))
            .save(&source)
            .unwrap();
        let jpeg = fs::read(&source).unwrap();
        let exif: &[u8] = &[
            b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0,
            0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut oriented = jpeg[..2].to_vec();
        oriented.extend([0xff, 0xe1]);
        oriented.extend(((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend(exif);
        oriented.extend(&jpeg[2..]);
        fs::write(&source, oriented).unwrap();
        let recipe = ExportRecipe {
            format: ExportFormat::Jpeg,
            max_edge: None,
            rotation: 90,
            ..Default::default()
        };
        let plan = LocalBatch
            .plan(&[source], &output, recipe, &CancellationToken::default())
            .unwrap();
        assert_eq!(plan.items[0].output_size, (30, 20));
        let mut session = LocalBatch.start(&plan).unwrap();
        session
            .export(
                0,
                &plan.items[0],
                &plan.recipe,
                &CancellationToken::default(),
            )
            .unwrap();
        let (_, mut decoded) = decoder(&plan.items[0].destination).unwrap();
        assert_eq!(decoded.dimensions(), (30, 20));
        assert_eq!(decoded.orientation().unwrap(), Orientation::NoTransforms);
    }

    #[test]
    fn publication_is_reported_even_when_the_final_journal_update_fails() {
        let (_dir, source, output) = fixture();
        let plan = LocalBatch
            .plan(
                &[source],
                &output,
                ExportRecipe::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        let mut session = LocalBatch.start(&plan).unwrap();
        let db = Connection::open(session.journal_path()).unwrap();
        db.execute_batch(
            "CREATE TRIGGER refuse_ack BEFORE UPDATE OF state ON items
            WHEN NEW.state='published' BEGIN SELECT RAISE(ABORT,'test journal failure'); END;",
        )
        .unwrap();
        let error = session
            .export(
                0,
                &plan.items[0],
                &plan.recipe,
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Published);
        assert!(plan.items[0].destination.exists());
        assert_eq!(
            db.query_row("SELECT state FROM items", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ready"
        );
    }
}
