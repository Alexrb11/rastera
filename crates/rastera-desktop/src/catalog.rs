use eframe::egui;
use rastera_adapters::{SqliteCatalog, catalog_path};
use rastera_core::{
    Annotation, CatalogEdit, ErrorKind, FolderEntry, ImageInfo, RasteraError, normalize_tags,
};
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
    active: Option<HashSet<PathBuf>>,
    search: String,
    favorites: bool,
    format: String,
    descending: bool,
    dirty: bool,
    pub filtered: Vec<FolderEntry>,
    pub editor_open: bool,
    editor_path: Option<PathBuf>,
    editor_favorite: bool,
    editor_tags: String,
    pub bulk_open: bool,
    pub bulk_busy: bool,
    bulk_paths: Vec<PathBuf>,
    bulk_favorite: Option<bool>,
    bulk_tags: String,
    bulk_remove_tags: bool,
    bulk_error: Option<String>,
    bulk_result: Option<usize>,
    bulk_preview_dirty: bool,
    bulk_preview: std::result::Result<usize, String>,
    bulk_hidden: usize,
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
            active: None,
            search: String::new(),
            favorites: false,
            format: String::new(),
            descending: false,
            dirty: true,
            filtered: Vec::new(),
            editor_open: false,
            editor_path: None,
            editor_favorite: false,
            editor_tags: String::new(),
            bulk_open: false,
            bulk_busy: false,
            bulk_paths: Vec::new(),
            bulk_favorite: None,
            bulk_tags: String::new(),
            bulk_remove_tags: false,
            bulk_error: None,
            bulk_result: None,
            bulk_preview_dirty: true,
            bulk_preview: Ok(0),
            bulk_hidden: 0,
        }
    }
    #[cfg(test)]
    pub fn for_test(path: PathBuf) -> Self {
        Self::with_worker(CatalogWorker::new(
            move || Ok(Box::new(SqliteCatalog::open(&path)?)),
            Arc::new(|| {}),
        ))
    }
    pub fn reset(&mut self, generation: u64) {
        self.generation = generation;
        self.ready = false;
        self.error = None;
        self.annotations.clear();
        self.pending.clear();
        self.active = Some(HashSet::new());
        self.filtered.clear();
        self.dirty = true;
        self.editor_path = None;
        self.editor_open = false;
        self.bulk_open = false;
    }
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }
    pub fn reconcile(&mut self, entries: &[FolderEntry]) {
        let paths: HashSet<_> = entries.iter().map(|entry| &entry.path).collect();
        self.active = Some(paths.iter().map(|path| (*path).clone()).collect());
        self.annotations.retain(|path, _| paths.contains(path));
        if self
            .editor_path
            .as_ref()
            .is_some_and(|path| !paths.contains(path))
        {
            self.editor_open = false;
            self.editor_path = None;
        }
        self.invalidate();
        self.refresh(entries);
    }
    pub fn annotation(&self, path: &Path) -> Option<&Annotation> {
        self.annotations.get(path)
    }
    pub fn can_edit(&self) -> bool {
        self.ready && !self.bulk_busy
    }
    pub fn open_bulk(&mut self, paths: Vec<PathBuf>) {
        if self.bulk_busy {
            self.bulk_open = true;
            return;
        }
        self.editor_open = false;
        self.editor_path = None;
        self.bulk_paths = paths;
        self.bulk_paths.sort();
        self.bulk_paths.dedup();
        self.bulk_favorite = None;
        self.bulk_tags.clear();
        self.bulk_remove_tags = false;
        self.bulk_error = None;
        self.bulk_result = None;
        self.bulk_preview_dirty = true;
        self.bulk_open = !self.bulk_paths.is_empty();
    }
    fn bulk_edit(&self) -> CatalogEdit {
        let mut tags: Vec<_> = self
            .bulk_tags
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect();
        tags.sort();
        tags.dedup();
        CatalogEdit {
            favorite: self.bulk_favorite,
            tags,
            remove_tags: self.bulk_remove_tags,
        }
    }
    #[cfg(feature = "capture")]
    pub fn capture_bulk(&mut self, paths: Vec<PathBuf>) {
        self.open_bulk(paths);
        self.bulk_tags = "colección-demo".into();
        self.bulk_favorite = Some(true);
        if std::env::var("RASTERA_CAPTURE_BULK").as_deref() == Ok("apply") {
            self.apply_bulk();
        }
    }
    fn apply_bulk(&mut self) {
        if !self.can_edit() {
            return;
        }
        let edit = self.bulk_edit();
        if let Err(error) = edit.apply(&Annotation::default()) {
            self.bulk_error = Some(error.to_string());
            return;
        }
        match self.worker.request(CatalogCommand::Edit {
            generation: self.generation,
            paths: self.bulk_paths.clone(),
            edit,
        }) {
            Ok(()) => {
                self.bulk_busy = true;
                self.bulk_error = None;
                self.bulk_result = None;
            }
            Err(error) => self.bulk_error = Some(error.to_string()),
        }
    }
    pub fn open_editor(&mut self) {
        if self.bulk_busy {
            return;
        }
        self.bulk_open = false;
        self.editor_path = None;
        self.editor_open = true;
    }
    pub fn toggle_favorite(&mut self, path: &Path, info: Option<ImageInfo>) {
        let mut annotation = self.annotations.get(path).cloned().unwrap_or_default();
        annotation.favorite = !annotation.favorite;
        self.save(path.to_path_buf(), annotation, info);
    }
    pub fn focus_search(&self, ctx: &egui::Context) {
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("catalog-search")));
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
        if let Some(active) = &mut self.active {
            active.extend(paths.iter().cloned());
        }
        if let Err(error) = self.worker.request(CatalogCommand::Load {
            generation: self.generation,
            paths,
        }) {
            self.error = Some(error.to_string());
        }
    }
    pub fn receive(&mut self) {
        while let Some(event) = self.worker.try_recv() {
            self.bulk_preview_dirty = true;
            if event.edited {
                self.bulk_busy = false;
                match &event.result {
                    Ok(records) => {
                        self.bulk_result = Some(records.len());
                        self.bulk_error = None;
                    }
                    Err(error) => self.bulk_error = Some(error.to_string()),
                }
            }
            if event.generation != self.generation {
                continue;
            }
            match event.result {
                Ok(records) => {
                    self.ready = true;
                    self.error = None;
                    for (path, annotation) in records {
                        if self
                            .active
                            .as_ref()
                            .is_some_and(|active| !active.contains(&path))
                        {
                            self.pending.remove(&path);
                            continue;
                        }
                        if event.edited {
                            self.pending.remove(&path);
                            self.annotations.insert(path, annotation);
                        } else if self.pending.contains(&path) {
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
    pub fn save_metadata(&mut self, path: &Path, info: &ImageInfo) -> bool {
        if self.bulk_busy {
            return false;
        }
        let mut annotation = self.annotations.get(path).cloned().unwrap_or_default();
        annotation.width = Some(info.width);
        annotation.height = Some(info.height);
        annotation.format = Some(info.kind.to_string());
        self.save(path.to_path_buf(), annotation, Some(info.clone()))
    }
    fn save(&mut self, path: PathBuf, annotation: Annotation, info: Option<ImageInfo>) -> bool {
        if self.bulk_busy {
            return false;
        }
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
            self.bulk_preview_dirty = true;
            true
        }
    }

    pub fn bulk_editor(&mut self, ctx: &egui::Context) {
        let mut open = self.bulk_open;
        let mut close = false;
        egui::Window::new("Organizar selección")
            .open(&mut open)
            .collapsible(false)
            .default_width(500.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("bulk-organization")
                    .max_height((ctx.viewport_rect().height() - 190.0).max(140.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} imágenes en la selección",
                                self.bulk_paths.len()
                            ))
                            .strong(),
                        );
                        if self.bulk_preview_dirty {
                            let visible: HashSet<_> =
                                self.filtered.iter().map(|entry| &entry.path).collect();
                            self.bulk_hidden = self
                                .bulk_paths
                                .iter()
                                .filter(|path| !visible.contains(path))
                                .count();
                        }
                        if self.bulk_hidden > 0 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Incluye {} fuera del filtro actual",
                                    self.bulk_hidden
                                ))
                                .small()
                                .color(crate::theme::MUTED),
                            );
                        }
                        let mut changed = false;
                        ui.add_enabled_ui(self.can_edit(), |ui| {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.label("Favoritas");
                                egui::ComboBox::from_id_salt("bulk-favorite")
                                    .selected_text(match self.bulk_favorite {
                                        None => "Sin cambios",
                                        Some(true) => "Marcar como favoritas",
                                        Some(false) => "Quitar de favoritas",
                                    })
                                    .show_ui(ui, |ui| {
                                        for (value, label) in [
                                            (None, "Sin cambios"),
                                            (Some(true), "Marcar como favoritas"),
                                            (Some(false), "Quitar de favoritas"),
                                        ] {
                                            changed |= ui
                                                .selectable_value(
                                                    &mut self.bulk_favorite,
                                                    value,
                                                    label,
                                                )
                                                .changed();
                                        }
                                    });
                            });
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("bulk-tags-mode")
                                    .selected_text(if self.bulk_remove_tags {
                                        "Retirar etiquetas"
                                    } else {
                                        "Añadir etiquetas"
                                    })
                                    .show_ui(ui, |ui| {
                                        changed |= ui
                                            .selectable_value(
                                                &mut self.bulk_remove_tags,
                                                false,
                                                "Añadir etiquetas",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut self.bulk_remove_tags,
                                                true,
                                                "Retirar etiquetas",
                                            )
                                            .changed();
                                    });
                            });
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.bulk_tags)
                                        .char_limit(4096)
                                        .hint_text("Ej. viaje, aprobada")
                                        .desired_width(f32::INFINITY),
                                )
                                .changed();
                            ui.label(
                                egui::RichText::new(
                                    "Separa con comas. Las demás etiquetas se conservan.",
                                )
                                .small()
                                .color(crate::theme::MUTED),
                            );
                        });
                        if changed {
                            self.bulk_preview_dirty = true;
                            self.bulk_error = None;
                            self.bulk_result = None;
                        }
                        if self.bulk_preview_dirty {
                            let edit = self.bulk_edit();
                            self.bulk_preview =
                                self.bulk_paths.iter().try_fold(0, |count, path| {
                                    let before =
                                        self.annotations.get(path).cloned().unwrap_or_default();
                                    let after = edit.apply(&before).map_err(|error| {
                                        format!(
                                            "{}: {error}",
                                            path.file_name().unwrap_or_default().to_string_lossy()
                                        )
                                    })?;
                                    Ok(count + usize::from(before != after))
                                });
                            self.bulk_preview_dirty = false;
                        }
                        ui.add_space(8.0);
                        if self.bulk_busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Guardando organización…");
                            });
                        } else if let Some(count) = self.bulk_result {
                            ui.colored_label(
                                egui::Color32::from_rgb(149, 207, 166),
                                format!("Organización guardada para {count} imágenes"),
                            );
                        } else {
                            match &self.bulk_preview {
                                Ok(0) => {
                                    ui.label(
                                        egui::RichText::new("No hay cambios que guardar")
                                            .color(crate::theme::MUTED),
                                    );
                                }
                                Ok(count) => {
                                    ui.label(format!("Vista previa: {count} imágenes cambiarán"));
                                }
                                Err(error) => {
                                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                                }
                            }
                        }
                        if let Some(error) = &self.bulk_error {
                            ui.colored_label(egui::Color32::LIGHT_RED, error);
                        }
                        egui::CollapsingHeader::new("Imágenes incluidas").show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("bulk-paths")
                                .max_height(100.0)
                                .show_rows(ui, 22.0, self.bulk_paths.len(), |ui, rows| {
                                    for index in rows {
                                        let path = &self.bulk_paths[index];
                                        ui.add(
                                            egui::Label::new(
                                                path.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy(),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(crate::theme::display_path(path));
                                    }
                                });
                        });
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.can_edit() && matches!(self.bulk_preview, Ok(count) if count > 0),
                            crate::theme::primary("Guardar organización"),
                        )
                        .clicked()
                    {
                        self.apply_bulk();
                    }
                    close = ui
                        .button(if self.bulk_busy {
                            "Ocultar panel"
                        } else {
                            "Cerrar"
                        })
                        .clicked();
                });
                ui.label(
                    egui::RichText::new(
                        "Se comprueba y guarda toda la selección. Los originales se conservan.",
                    )
                    .small()
                    .color(crate::theme::MUTED),
                );
            });
        self.bulk_open = open && !close;
    }

    pub fn filters(&mut self, ui: &mut egui::Ui, entries: &[FolderEntry]) {
        ui.horizontal_wrapped(|ui| {
            self.dirty |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Buscar nombre o etiqueta")
                        .id(egui::Id::new("catalog-search"))
                        .desired_width((ui.available_width() - 375.0).clamp(160.0, 350.0)),
                )
                .on_hover_text("Buscar en esta carpeta · Ctrl+F")
                .changed();
            self.dirty |= ui
                .selectable_value(&mut self.favorites, false, "Todas")
                .changed();
            self.dirty |= ui
                .selectable_value(&mut self.favorites, true, "Favoritas")
                .changed();
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
        self.refresh(entries);
    }
    pub fn refresh(&mut self, entries: &[FolderEntry]) {
        if self.dirty {
            self.bulk_preview_dirty = true;
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
            self.filtered.sort_by_cached_key(|entry| {
                entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
            });
            if self.descending {
                self.filtered.reverse();
            }
            self.dirty = false;
        }
    }

    pub fn sort_control(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("catalog-sort")
            .selected_text(if self.descending {
                "Nombre Z–A"
            } else {
                "Nombre A–Z"
            })
            .width(108.0)
            .show_ui(ui, |ui| {
                self.dirty |= ui
                    .selectable_value(&mut self.descending, false, "Nombre A–Z")
                    .changed();
                self.dirty |= ui
                    .selectable_value(&mut self.descending, true, "Nombre Z–A")
                    .changed();
            });
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
        let mut cancelled = false;
        egui::Window::new("Organizar imagen")
            .open(&mut open)
            .default_width(360.0)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                if let Some(path) = path {
                    ui.label(
                        egui::RichText::new(path.file_name().unwrap_or_default().to_string_lossy())
                            .strong(),
                    );
                    ui.add_space(8.0);
                    ui.checkbox(&mut self.editor_favorite, "Favorita");
                    ui.label("Etiquetas");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.editor_tags)
                            .char_limit(2048)
                            .hint_text("Ej. viaje, paisaje, para imprimir")
                            .desired_width(f32::INFINITY),
                    );
                    ui.label(
                        egui::RichText::new("Separa las etiquetas con comas. Hasta 32 etiquetas.")
                            .small()
                            .color(crate::theme::MUTED),
                    );
                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(self.can_edit(), crate::theme::primary("Guardar cambios"))
                            .clicked()
                        {
                            let mut annotation =
                                self.annotations.get(path).cloned().unwrap_or_default();
                            annotation.favorite = self.editor_favorite;
                            annotation.tags = normalize_tags(&self.editor_tags);
                            saved = self.save(path.to_path_buf(), annotation, info.cloned());
                        }
                        cancelled = ui.button("Cancelar").clicked();
                    });
                    ui.label(
                        egui::RichText::new("Guardado local. El archivo original se conserva.")
                            .small()
                            .color(crate::theme::MUTED),
                    );
                } else {
                    ui.label("Selecciona una imagen.");
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            });
        self.editor_open = open && !saved && !cancelled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconciliation_keeps_filters_and_does_not_restore_removed_rows_from_late_load() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::for_test(dir.path().join("catalog.sqlite"));
        let entries = ["keep.png", "removed.png", "hidden.jpg"].map(|name| FolderEntry {
            path: name.into(),
            stamp: None,
        });
        let mut db = SqliteCatalog::open(&dir.path().join("catalog.sqlite")).unwrap();
        db.save(&entries[1].path, &Annotation::default(), None)
            .unwrap();
        drop(db);
        catalog.search = "keep".into();
        catalog.descending = true;
        catalog.favorites = true;
        catalog.annotations.insert(
            entries[0].path.clone(),
            Annotation {
                favorite: true,
                tags: vec!["aprobada".into()],
                ..Default::default()
            },
        );
        catalog
            .annotations
            .insert(entries[1].path.clone(), Annotation::default());
        catalog.load(vec![entries[1].path.clone()]);
        catalog.reconcile(&[entries[0].clone(), entries[2].clone()]);
        assert_eq!(catalog.filtered, [entries[0].clone()]);
        assert!(catalog.descending && catalog.favorites);
        assert_eq!(catalog.search, "keep");
        assert!(catalog.annotations[&entries[0].path].favorite);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !catalog.ready {
            catalog.receive();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!catalog.annotations.contains_key(&entries[1].path));
    }
    use rastera_core::{CatalogBackend, FileStamp, ImageKind};
    fn settle_bulk(catalog: &mut Catalog) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while catalog.bulk_busy {
            catalog.receive();
            assert!(std::time::Instant::now() < deadline, "Bulk edit timeout");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    fn selection_edit_waits_for_accepted_writes_and_defers_metadata_until_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("catalog.sqlite");
        let paths = vec![dir.path().join("first.png"), dir.path().join("second.png")];
        let mut catalog = Catalog::for_test(db_path.clone());
        catalog.ready = true;
        catalog.save(
            paths[0].clone(),
            Annotation {
                tags: vec!["individual".into()],
                ..Default::default()
            },
            None,
        );
        catalog.open_bulk(paths.clone());
        catalog.bulk_tags = "aprobada".into();
        catalog.bulk_favorite = Some(true);
        catalog.apply_bulk();
        assert!(catalog.bulk_busy);
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
        assert!(!catalog.save_metadata(&paths[0], &info));
        settle_bulk(&mut catalog);
        assert_eq!(catalog.bulk_result, Some(2));
        assert!(catalog.bulk_error.is_none());
        assert_eq!(
            catalog.annotations[&paths[0]].tags,
            ["aprobada", "individual"]
        );
        assert_eq!(catalog.annotations[&paths[1]].tags, ["aprobada"]);
        assert!(catalog.pending.is_empty());
        assert!(catalog.save_metadata(&paths[0], &info));
        drop(catalog);
        let mut db = SqliteCatalog::open(&db_path).unwrap();
        let records = db.load(&paths).unwrap();
        assert!(records.iter().all(|(_, annotation)| annotation.favorite));
        assert_eq!(records[0].1.tags, ["aprobada", "individual"]);
        assert_eq!(
            (records[0].1.width, records[0].1.height),
            (Some(90), Some(60))
        );
    }
    #[test]
    fn changing_folder_preserves_accepted_bulk_edit_without_polluting_new_generation() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("catalog.sqlite");
        let path = dir.path().join("first.png");
        let mut catalog = Catalog::for_test(db_path.clone());
        catalog.reset(1);
        catalog.ready = true;
        catalog.open_bulk(vec![path.clone()]);
        catalog.bulk_tags = "pendiente".into();
        catalog.apply_bulk();
        catalog.reset(2);
        assert!(!catalog.bulk_open);
        settle_bulk(&mut catalog);
        assert!(catalog.annotations.is_empty());
        assert!(!catalog.ready);
        assert_eq!(catalog.bulk_result, Some(1));
        drop(catalog);
        assert_eq!(
            SqliteCatalog::open(&db_path)
                .unwrap()
                .load(&[path])
                .unwrap()[0]
                .1
                .tags,
            ["pendiente"]
        );
    }

    #[test]
    fn filtered_library_sorts_names_and_keeps_order_when_filters_reset() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("catalog.sqlite");
        let worker = CatalogWorker::new(
            move || Ok(Box::new(SqliteCatalog::open(&db_path)?)),
            Arc::new(|| {}),
        );
        let mut catalog = Catalog::with_worker(worker);
        let entries: Vec<_> = ["zebra.png", "Alpha.png", "middle.jpg"]
            .into_iter()
            .map(|name| FolderEntry {
                path: dir.path().join(name),
                stamp: None,
            })
            .collect();
        catalog.annotations.insert(
            entries[0].path.clone(),
            Annotation {
                favorite: true,
                tags: vec!["Aprobada".into()],
                ..Default::default()
            },
        );
        catalog.annotations.insert(
            entries[1].path.clone(),
            Annotation {
                favorite: true,
                tags: vec!["aprobada".into()],
                ..Default::default()
            },
        );
        let ctx = egui::Context::default();
        let refresh = |catalog: &mut Catalog| {
            let mut output = ctx.run_ui(Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| catalog.filters(ui, &entries));
            });
            output.textures_delta.clear();
            catalog
                .filtered
                .iter()
                .map(|entry| {
                    entry
                        .path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            refresh(&mut catalog),
            ["Alpha.png", "middle.jpg", "zebra.png"]
        );
        catalog.descending = true;
        catalog.search = "APROBADA".into();
        catalog.favorites = true;
        catalog.format = "PNG".into();
        catalog.invalidate();
        assert_eq!(refresh(&mut catalog), ["zebra.png", "Alpha.png"]);
        catalog.clear_filters();
        assert_eq!(
            refresh(&mut catalog),
            ["zebra.png", "middle.jpg", "Alpha.png"]
        );
    }

    #[test]
    fn inline_favorite_preserves_tags_and_dimensions_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("catalog.sqlite");
        let worker_path = db_path.clone();
        let worker = CatalogWorker::new(
            move || Ok(Box::new(SqliteCatalog::open(&worker_path)?)),
            Arc::new(|| {}),
        );
        let mut catalog = Catalog::with_worker(worker);
        let path = dir.path().join("photo.png");
        let annotation = Annotation {
            favorite: true,
            tags: vec!["aprobada".into()],
            width: Some(90),
            height: Some(60),
            format: Some("PNG".into()),
        };
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
        assert!(catalog.save(path.clone(), annotation.clone(), Some(info)));
        catalog.toggle_favorite(&path, None);
        assert!(!catalog.annotations[&path].favorite);
        drop(catalog);
        let mut db = SqliteCatalog::open(&db_path).unwrap();
        let saved = db.load(&[path]).unwrap().remove(0).1;
        assert_eq!(
            saved,
            Annotation {
                favorite: false,
                ..annotation
            }
        );
    }
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
