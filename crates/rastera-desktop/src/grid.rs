use eframe::egui::{self, Color32, Rect, Sense, TextureHandle, Vec2};
use rastera_adapters::CachedThumbnails;
use rastera_core::FolderEntry;
use rastera_engine::{ImageLoader, memory::MemoryBudget};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

struct Thumbnail {
    texture: Option<TextureHandle>,
    size: Vec2,
    error: Option<String>,
    touched: u64,
}

pub struct ThumbnailGrid {
    backend: Arc<CachedThumbnails>,
    loader: ImageLoader,
    cache: HashMap<PathBuf, Thumbnail>,
    pending: Option<PathBuf>,
    generation: u64,
    clock: u64,
    pub selected: HashSet<PathBuf>,
    pub open_viewer: bool,
}

impl ThumbnailGrid {
    pub fn new(ctx: &egui::Context, budget: MemoryBudget) -> Self {
        let ctx = ctx.clone();
        let backend = Arc::new(CachedThumbnails::default());
        Self {
            loader: ImageLoader::new(
                backend.clone(),
                budget,
                Arc::new(move || ctx.request_repaint()),
            ),
            backend,
            cache: HashMap::new(),
            pending: None,
            generation: 0,
            clock: 0,
            selected: HashSet::new(),
            open_viewer: false,
        }
    }

    pub fn reset(&mut self) {
        self.generation += 1;
        self.loader.cancel();
        self.pending = None;
        self.cache.clear();
        self.selected.clear();
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn diagnostics(
        &self,
    ) -> (
        usize,
        rastera_engine::ImageLoadStats,
        rastera_adapters::ThumbnailCacheStats,
    ) {
        (self.cache.len(), self.loader.stats(), self.backend.stats())
    }

    pub fn receive(&mut self, ctx: &egui::Context) {
        while let Some(event) = self.loader.try_recv() {
            if event.generation != self.generation {
                continue;
            }
            let Some(path) = self.pending.take() else {
                continue;
            };
            let thumbnail = match event.result {
                Ok(image) => Thumbnail {
                    size: egui::vec2(image.width as f32, image.height as f32),
                    texture: Some(ctx.load_texture(
                        format!("thumbnail-{}", self.clock),
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width as usize, image.height as usize],
                            &image.rgba,
                        ),
                        egui::TextureOptions::LINEAR,
                    )),
                    error: None,
                    touched: self.clock,
                },
                Err(error) => Thumbnail {
                    texture: None,
                    size: Vec2::ZERO,
                    error: Some(error.to_string()),
                    touched: self.clock,
                },
            };
            // At most 128 textures of 256² RGBA pixels: 32 MiB of GPU pixel data.
            if self.cache.len() >= 128
                && let Some(old) = self
                    .cache
                    .iter()
                    .min_by_key(|(_, thumb)| thumb.touched)
                    .map(|(path, _)| path.clone())
            {
                self.cache.remove(&old);
            }
            self.cache.insert(path, thumbnail);
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        entries: &[FolderEntry],
        preview_loading: bool,
    ) -> Option<PathBuf> {
        let mut opened = None;
        self.open_viewer = false;
        ui.horizontal(|ui| {
            ui.heading("Biblioteca");
            ui.label(format!(
                "{} imágenes · {} seleccionadas",
                entries.len(),
                self.selected.len()
            ));
            if ui.button("Seleccionar todo").clicked() {
                self.selected = entries.iter().map(|entry| entry.path.clone()).collect();
            }
            if ui.button("Limpiar selección").clicked() {
                self.selected.clear();
            }
        });
        ui.label("Clic: seleccionar · Ctrl+clic: añadir o quitar · Doble clic: abrir");
        ui.add_space(8.0);
        let columns = (ui.available_width() / 170.0).floor().max(1.0) as usize;
        let tile_width = (ui.available_width() / columns as f32 - 8.0).max(40.0);
        let row_height = 166.0;
        let mut wanted = None;
        let mut pending_visible = false;
        egui::ScrollArea::vertical()
            .id_salt("thumbnail-grid")
            .show_rows(
                ui,
                row_height,
                entries.len().div_ceil(columns),
                |ui, rows| {
                    for row in rows {
                        ui.horizontal(|ui| {
                            for entry in entries
                                .iter()
                                .take(((row + 1) * columns).min(entries.len()))
                                .skip(row * columns)
                            {
                                let path = &entry.path;
                                self.clock += 1;
                                pending_visible |= self.pending.as_ref() == Some(path);
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(tile_width, row_height - 8.0),
                                    Sense::click(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    6.0,
                                    if self.selected.contains(path) {
                                        Color32::from_rgb(42, 77, 64)
                                    } else {
                                        Color32::from_rgb(26, 33, 42)
                                    },
                                );
                                let area = Rect::from_min_max(
                                    rect.min + egui::vec2(8.0, 8.0),
                                    rect.max - egui::vec2(8.0, 30.0),
                                );
                                if let Some(thumbnail) = self.cache.get_mut(path) {
                                    thumbnail.touched = self.clock;
                                    if let Some(texture) = &thumbnail.texture {
                                        let scale = (area.width() / thumbnail.size.x)
                                            .min(area.height() / thumbnail.size.y);
                                        ui.painter().image(
                                            texture.id(),
                                            Rect::from_center_size(
                                                area.center(),
                                                thumbnail.size * scale,
                                            ),
                                            Rect::from_min_max(
                                                egui::Pos2::ZERO,
                                                egui::pos2(1.0, 1.0),
                                            ),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        ui.painter().text(
                                            area.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "No disponible",
                                            egui::FontId::proportional(13.0),
                                            Color32::LIGHT_RED,
                                        );
                                        response.clone().on_hover_text(
                                            thumbnail.error.as_deref().unwrap_or_default(),
                                        );
                                    }
                                } else {
                                    wanted.get_or_insert_with(|| path.clone());
                                    ui.painter().text(
                                        area.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "…",
                                        egui::FontId::proportional(20.0),
                                        Color32::GRAY,
                                    );
                                }
                                let name = path.file_name().unwrap_or_default().to_string_lossy();
                                let galley = ui.painter().layout(
                                    name.into_owned(),
                                    egui::FontId::proportional(13.0),
                                    Color32::WHITE,
                                    tile_width - 16.0,
                                );
                                ui.painter()
                                    .with_clip_rect(Rect::from_min_max(
                                        egui::pos2(rect.min.x + 8.0, rect.max.y - 25.0),
                                        rect.max - egui::vec2(8.0, 2.0),
                                    ))
                                    .galley(
                                        egui::pos2(rect.min.x + 8.0, rect.max.y - 25.0),
                                        galley,
                                        Color32::WHITE,
                                    );
                                if response.clicked() {
                                    opened = Some(path.clone());
                                    if !ui.input(|input| input.modifiers.command) {
                                        self.selected.clear();
                                    }
                                    if !self.selected.insert(path.clone()) {
                                        self.selected.remove(path);
                                    }
                                }
                                if response.double_clicked() {
                                    self.open_viewer = true;
                                    opened = Some(path.clone());
                                }
                            }
                        });
                    }
                },
            );
        // Only one thumbnail in flight; scrolling cancels off-screen work.
        if self.pending.is_some() && (!pending_visible || preview_loading) {
            self.generation += 1;
            self.loader.cancel();
            self.pending = None;
        }
        if !preview_loading
            && self.pending.is_none()
            && let Some(path) = wanted
        {
            self.generation += 1;
            self.loader.request(self.generation, path.clone(), 256);
            self.pending = Some(path);
        }
        opened
    }
}
