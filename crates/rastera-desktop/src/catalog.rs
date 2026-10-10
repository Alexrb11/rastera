use eframe::egui;
use rastera_adapters::{SqliteCatalog, catalog_path};
use rastera_core::{Annotation, ErrorKind, FolderEntry, ImageInfo, RasteraError, normalize_tags};
use rastera_engine::{CatalogCommand, CatalogWorker};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Catalog {
    worker: CatalogWorker,
    generation: u64,
    pub ready: bool,
    pub error: Option<String>,
    annotations: HashMap<PathBuf, Annotation>,
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
        Self {
            worker,
            generation: 0,
            ready: false,
            error: None,
            annotations: HashMap::new(),
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
        self.filtered.clear();
        self.dirty = true;
        self.editor_path = None;
    }
    pub fn invalidate(&mut self) {
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
                    self.annotations.extend(records);
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
    fn save(&mut self, path: PathBuf, annotation: Annotation, info: Option<ImageInfo>) {
        if let Err(error) = self.worker.request(CatalogCommand::Save {
            generation: self.generation,
            path,
            annotation,
            info,
        }) {
            self.error = Some(error.to_string());
        }
    }

    pub fn filters(&mut self, ui: &mut egui::Ui, entries: &[FolderEntry]) {
        ui.horizontal_wrapped(|ui| {
            self.dirty |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Buscar nombre o etiqueta")
                        .desired_width(220.0),
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
            if ui
                .add_enabled(self.ready, egui::Button::new("Favoritos y etiquetas"))
                .clicked()
            {
                self.editor_open = true;
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
        egui::Window::new("Organizar imagen")
            .open(&mut open)
            .default_width(360.0)
            .show(ctx, |ui| {
                if let Some(path) = path {
                    ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                    ui.checkbox(&mut self.editor_favorite, "Favorita");
                    ui.label("Etiquetas separadas por comas (hasta 32, de 64 caracteres)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.editor_tags)
                            .char_limit(2048)
                            .desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_enabled(self.ready, egui::Button::new("Guardar"))
                        .clicked()
                    {
                        let mut annotation =
                            self.annotations.get(path).cloned().unwrap_or_default();
                        annotation.favorite = self.editor_favorite;
                        annotation.tags = normalize_tags(&self.editor_tags);
                        self.save(path.to_path_buf(), annotation, info.cloned());
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
        self.editor_open = open;
    }
}
