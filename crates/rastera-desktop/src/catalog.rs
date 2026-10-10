use eframe::egui;
use rastera_adapters::{SqliteCatalog, catalog_path};
use rastera_core::{Annotation, ErrorKind, FolderEntry, ImageInfo, RasteraError, normalize_tags};
use rastera_engine::{CatalogCommand, CatalogWorker};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Catalog {
    worker: CatalogWorker,
    generation: u64,
    pub ready: bool,
    pub error: Option<String>,
    annotations: HashMap<PathBuf, Annotation>,
    pending: HashSet<PathBuf>,
    search: String,
    favorites: bool,
    format: String,
    dirty: bool,
    pub filtered: Vec<FolderEntry>,
    pub editor_open: bool,
    editor_path: Option<PathBuf>,
    editor_favorite: bool,
    editor_tags: String,
}

impl Catalog {
    pub fn new(ctx: &egui::Context) -> Self {
        let ctx = ctx.clone();
        let worker = CatalogWorker::new(
            || {
                let path = catalog_path()?;
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|error| {
                    RasteraError::new(
                        ErrorKind::Io,
                        format!("No se pudo crear el catálogo: {error}"),
                    )
                })?;
                Ok(Box::new(SqliteCatalog::open(&path)?))
            },
            Arc::new(move || ctx.request_repaint()),
        );
        Self::with_worker(worker)
    }
    fn with_worker(worker: CatalogWorker) -> Self {
        Self {
            worker,
            generation: 0,
            ready: false,
            error: None,
            annotations: HashMap::new(),
            pending: HashSet::new(),
            search: String::new(),
            favorites: false,
            format: String::new(),
            dirty: true,
            filtered: Vec::new(),
            editor_open: false,
            editor_path: None,
            editor_favorite: false,
            editor_tags: String::new(),
        }
    }
    pub fn reset(&mut self, generation: u64) {
        self.generation = generation;
        self.ready = false;
        self.error = None;
        self.annotations.clear();
        self.pending.clear();
        self.filtered.clear();
        self.dirty = true;
        self.editor_path = None;
        self.editor_open = false;
    }
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }
    pub fn annotation(&self, path: &Path) -> Option<&Annotation> {
        self.annotations.get(path)
    }
    pub fn clear_filters(&mut self) {
        self.search.clear();
        self.favorites = false;
        self.format.clear();
        self.dirty = true;
    }
    #[cfg(feature = "capture")]
    pub fn capture_search(&mut self, search: String) {
        self.search = search;
        self.dirty = true;
    }
    pub fn load(&mut self, paths: Vec<PathBuf>) {
        if let Err(error) = self.worker.request(CatalogCommand::Load {
            generation: self.generation,
            paths,
        }) {
            self.error = Some(error.to_string());
        }
    }
    pub fn receive(&mut self) {
        while let Some(event) = self.worker.try_recv() {
            if event.generation != self.generation {
                continue;
            }
            match event.result {
                Ok(records) => {
                    self.ready = true;
                    self.error = None;
                    for (path, annotation) in records {
                        if self.pending.contains(&path) {
                            if self.annotations.get(&path) == Some(&annotation) {
                                self.pending.remove(&path);
                            }
                        } else {
                            self.annotations.insert(path, annotation);
                        }
                    }
                    if !self.editor_open {
                        self.editor_path = None;
                    }
                    self.dirty = true;
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                }
            }
        }
    }
    pub fn save_metadata(&mut self, path: &Path, info: &ImageInfo) {
        let mut annotation = self.annotations.get(path).cloned().unwrap_or_default();
        annotation.width = Some(info.width);
        annotation.height = Some(info.height);
        annotation.format = Some(info.kind.to_string());
        self.save(path.to_path_buf(), annotation, Some(info.clone()));
    }
    fn save(&mut self, path: PathBuf, annotation: Annotation, info: Option<ImageInfo>) -> bool {
        if let Err(error) = self.worker.request(CatalogCommand::Save {
            generation: self.generation,
            path: path.clone(),
            annotation: annotation.clone(),
            info,
        }) {
            self.error = Some(error.to_string());
            false
        } else {
            self.pending.insert(path.clone());
            self.annotations.insert(path, annotation);
            self.dirty = true;
            true
        }
    }

    pub fn filters(&mut self, ui: &mut egui::Ui, entries: &[FolderEntry]) {
        ui.horizontal_wrapped(|ui| {
            self.dirty |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Buscar nombre o etiqueta")
                        .desired_width((ui.available_width() - 335.0).clamp(140.0, 300.0)),
                )
                .changed();
            self.dirty |= ui.checkbox(&mut self.favorites, "Solo favoritos").changed();
            egui::ComboBox::from_id_salt("format-filter")
                .selected_text(if self.format.is_empty() {
                    "Todos los formatos"
                } else {
                    &self.format
                })
                .show_ui(ui, |ui| {
                    for format in ["", "JPEG", "PNG", "WebP"] {
                        self.dirty |= ui
                            .selectable_value(
                                &mut self.format,
                                format.to_owned(),
                                if format.is_empty() {
                                    "Todos los formatos"
                                } else {
                                    format
                                },
                            )
                            .changed();
                    }
                });
            if (!self.search.is_empty() || self.favorites || !self.format.is_empty())
                && ui.button("Restablecer").clicked()
            {
                self.clear_filters();
            }
        });
        if self.dirty {
            let search = self.search.to_lowercase();
            self.filtered = entries
                .iter()
                .filter(|entry| {
                    let annotation = self.annotations.get(&entry.path);
                    let name = entry
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase();
                    let tags_match = annotation.is_some_and(|item| {
                        item.tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&search))
                    });
                    let extension = entry
                        .path
                        .extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_ascii_lowercase();
                    let format = match extension.as_str() {
                        "jpg" | "jpeg" => "JPEG",
                        "png" => "PNG",
                        "webp" => "WebP",
                        _ => "",
                    };
                    (search.is_empty() || name.contains(&search) || tags_match)
                        && (!self.favorites || annotation.is_some_and(|item| item.favorite))
                        && (self.format.is_empty() || format == self.format)
                })
                .cloned()
                .collect();
            self.dirty = false;
        }
    }

    pub fn editor(&mut self, ctx: &egui::Context, path: Option<&Path>, info: Option<&ImageInfo>) {
        if path != self.editor_path.as_deref() {
            self.editor_path = path.map(Path::to_path_buf);
            let annotation = path
                .and_then(|path| self.annotations.get(path))
                .cloned()
                .unwrap_or_default();
            self.editor_favorite = annotation.favorite;
            self.editor_tags = annotation.tags.join(", ");
        }
        let mut open = self.editor_open;
        let mut saved = false;
        egui::Window::new("Organizar imagen")
            .open(&mut open)
            .default_width(360.0)
            .collapsible(false)
            .show(ctx, |ui| {
                if let Some(path) = path {
                    ui.label(
                        egui::RichText::new(path.file_name().unwrap_or_default().to_string_lossy())
                            .strong(),
                    );
                    ui.add_space(8.0);
                    ui.checkbox(&mut self.editor_favorite, "Favorita");
                    ui.label("Etiquetas separadas por comas (hasta 32, de 64 caracteres)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.editor_tags)
                            .char_limit(2048)
                            .desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_enabled(self.ready, crate::theme::primary("Guardar cambios"))
                        .clicked()
                    {
                        let mut annotation =
                            self.annotations.get(path).cloned().unwrap_or_default();
                        annotation.favorite = self.editor_favorite;
                        annotation.tags = normalize_tags(&self.editor_tags);
                        saved = self.save(path.to_path_buf(), annotation, info.cloned());
                    }
                    ui.label(
                        "La organización se guarda en el catálogo local, sin modificar el archivo.",
                    );
                } else {
                    ui.label("Selecciona una imagen.");
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            });
        self.editor_open = open && !saved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rastera_core::{CatalogBackend, FileStamp, ImageKind};
    #[test]
    fn metadata_updates_preserve_accepted_edits_and_close_flushes_writes() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("catalog.sqlite");
        let worker_path = db_path.clone();
        let worker = CatalogWorker::new(
            move || Ok(Box::new(SqliteCatalog::open(&worker_path)?)),
            Arc::new(|| {}),
        );
        let path = dir.path().join("image.png");
        let mut catalog = Catalog::with_worker(worker);
        catalog.reset(1);
        catalog.load(vec![path.clone()]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !catalog.ready {
            catalog.receive();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let annotation = Annotation {
            favorite: true,
            tags: vec!["Viajes".into()],
            ..Default::default()
        };
        catalog.save(path.clone(), annotation, None);
        let info = ImageInfo {
            width: 90,
            height: 60,
            kind: ImageKind::Png,
            stamp: FileStamp {
                length: 200,
                modified: None,
            },
            has_color_profile: false,
        };
        // A decode can finish while the annotation write is still awaiting acknowledgment.
        catalog.save_metadata(&path, &info);
        assert!(catalog.annotations[&path].favorite);
        drop(catalog);
        let mut db = SqliteCatalog::open(&db_path).unwrap();
        let saved = db.load(&[path]).unwrap().remove(0).1;
        assert!(saved.favorite);
        assert_eq!(saved.tags, ["Viajes"]);
        assert_eq!((saved.width, saved.height), (Some(90), Some(60)));
    }
}
