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

#[cfg(feature = "capture")]
#[derive(Clone, Copy, Default)]
pub struct GridFrame {
    pub first: usize,
    pub last: usize,
    pub visible: usize,
    pub ready: usize,
    pub errors: usize,
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
    anchor: Option<PathBuf>,
    pub density: u8,
    #[cfg(feature = "capture")]
    pub benchmark_index: Option<usize>,
    #[cfg(feature = "capture")]
    pub measurement: GridFrame,
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
            anchor: None,
            density: 1,
            #[cfg(feature = "capture")]
            benchmark_index: None,
            #[cfg(feature = "capture")]
            measurement: GridFrame::default(),
        }
    }

    pub fn reset(&mut self) {
        self.generation += 1;
        self.loader.cancel();
        self.pending = None;
        self.cache.clear();
        self.selected.clear();
        self.anchor = None;
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }
    pub fn select_all(&mut self, entries: &[FolderEntry]) {
        self.selected = entries.iter().map(|entry| entry.path.clone()).collect();
        self.anchor = entries.first().map(|entry| entry.path.clone());
    }
    fn select(
        &mut self,
        path: &std::path::Path,
        entries: &[FolderEntry],
        additive: bool,
        range: bool,
    ) {
        if range
            && let Some(anchor) = &self.anchor
            && let (Some(from), Some(to)) = (
                entries.iter().position(|entry| &entry.path == anchor),
                entries.iter().position(|entry| entry.path == path),
            )
        {
            if !additive {
                self.selected.clear();
            }
            self.selected.extend(
                entries[from.min(to)..=from.max(to)]
                    .iter()
                    .map(|entry| entry.path.clone()),
            );
            return;
        }
        if !additive {
            self.selected.clear();
        }
        if !self.selected.insert(path.to_path_buf()) {
            self.selected.remove(path);
        }
        self.anchor = Some(path.to_path_buf());
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
        current: Option<&std::path::Path>,
        catalog: &crate::catalog::Catalog,
    ) -> Option<PathBuf> {
        use crate::theme;
        let mut opened = None;
        let mut clicked = None;
        self.open_viewer = false;
        let target_width = match self.density {
            0 => 112.0,
            2 => 204.0,
            _ => 152.0,
        };
        let spacing = ui.spacing().item_spacing.x;
        let columns = ((ui.available_width() + spacing) / (target_width + spacing))
            .floor()
            .max(1.0) as usize;
        let tile_width =
            ((ui.available_width() - spacing * (columns - 1) as f32) / columns as f32).max(40.0);
        let row_height = tile_width
            * if ui.ctx().viewport_rect().height() < 560.0 {
                0.48
            } else {
                0.76
            }
            + 30.0;
        let mut wanted = None;
        let mut pending_visible = false;
        #[cfg(feature = "capture")]
        let mut frame = GridFrame::default();
        let scroll = egui::ScrollArea::vertical()
            .id_salt(("thumbnail-grid", self.density))
            .auto_shrink([false, false]);
        #[cfg(feature = "capture")]
        let scroll = if let Some(index) = self.benchmark_index {
            let maximum = (entries.len().div_ceil(columns) as f32 * row_height
                - ui.available_height())
            .max(0.0);
            scroll.vertical_scroll_offset(((index / columns) as f32 * row_height).min(maximum))
        } else {
            scroll
        };
        scroll.show_rows(
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
                            #[cfg(feature = "capture")]
                            {
                                if frame.visible == 0 {
                                    frame.first = row * columns;
                                }
                                frame.visible += 1;
                                // The interval is exclusive and includes the virtualizer's overscan rows.
                                frame.last = frame.first + frame.visible;
                            }
                            self.clock += 1;
                            pending_visible |= self.pending.as_ref() == Some(path);
                            let selected = self.selected.contains(path);
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(tile_width, row_height - spacing),
                                Sense::click(),
                            );
                            let color = if selected {
                                theme::SELECTED
                            } else if response.hovered() {
                                Color32::from_rgb(45, 47, 50)
                            } else {
                                theme::BACKGROUND
                            };
                            ui.painter().rect_filled(rect, 2.0, color);
                            ui.painter().rect_stroke(
                                rect,
                                2.0,
                                egui::Stroke::new(
                                    if selected || current == Some(path.as_path()) {
                                        1.0
                                    } else {
                                        0.0
                                    },
                                    if selected {
                                        theme::ACCENT
                                    } else if current == Some(path.as_path()) {
                                        theme::MUTED
                                    } else {
                                        theme::BORDER
                                    },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            let area = Rect::from_min_max(
                                rect.min + egui::vec2(5.0, 5.0),
                                rect.max - egui::vec2(5.0, 26.0),
                            );
                            ui.painter().rect_filled(area, 0.0, theme::BACKGROUND);
                            if let Some(thumbnail) = self.cache.get_mut(path) {
                                #[cfg(feature = "capture")]
                                {
                                    frame.ready += 1;
                                    frame.errors += usize::from(thumbnail.error.is_some());
                                }
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
                                        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                } else {
                                    ui.painter().text(
                                        area.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "No disponible",
                                        egui::FontId::proportional(12.0),
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
                                    "Cargando…",
                                    egui::FontId::proportional(12.0),
                                    theme::MUTED,
                                );
                            }
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            let mut job = egui::text::LayoutJob::simple(
                                name.to_string(),
                                egui::FontId::proportional(12.0),
                                theme::TEXT,
                                tile_width - 10.0,
                            );
                            job.wrap.max_rows = 1;
                            job.wrap.break_anywhere = true;
                            let galley = ui.painter().layout_job(job);
                            ui.painter().galley(
                                egui::pos2(rect.min.x + 5.0, rect.max.y - 21.0),
                                galley,
                                theme::TEXT,
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    selected,
                                    &name,
                                )
                            });
                            if selected || response.hovered() {
                                let center = area.left_top() + egui::vec2(12.0, 12.0);
                                ui.painter().rect_filled(
                                    Rect::from_center_size(center, egui::vec2(14.0, 14.0)),
                                    1.0,
                                    if selected {
                                        theme::ACCENT
                                    } else {
                                        theme::SURFACE
                                    },
                                );
                                ui.painter().rect_stroke(
                                    Rect::from_center_size(center, egui::vec2(14.0, 14.0)),
                                    1.0,
                                    egui::Stroke::new(1.0, theme::ACCENT),
                                    egui::StrokeKind::Inside,
                                );
                                if selected {
                                    ui.painter().line_segment(
                                        [
                                            center + egui::vec2(-4.0, 0.0),
                                            center + egui::vec2(-1.0, 3.0),
                                        ],
                                        egui::Stroke::new(1.8, theme::BACKGROUND),
                                    );
                                    ui.painter().line_segment(
                                        [
                                            center + egui::vec2(-1.0, 3.0),
                                            center + egui::vec2(4.0, -3.0),
                                        ],
                                        egui::Stroke::new(1.8, theme::BACKGROUND),
                                    );
                                }
                            }
                            if catalog.annotation(path).is_some_and(|a| a.favorite) {
                                let badge = Rect::from_min_size(
                                    area.right_top() - egui::vec2(68.0, -4.0),
                                    egui::vec2(64.0, 20.0),
                                );
                                ui.painter().rect_filled(badge, 4.0, theme::SURFACE);
                                ui.painter().text(
                                    badge.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Favorita",
                                    egui::FontId::proportional(11.0),
                                    theme::ACCENT,
                                );
                            }
                            response.clone().on_hover_text(format!(
                                "{name}\n{}\nCtrl+clic: añadir · Shift+clic: rango",
                                path.display()
                            ));
                            if response.clicked() {
                                let modifiers = ui.input(|input| input.modifiers);
                                clicked = Some((path.clone(), modifiers.command, modifiers.shift));
                                opened = Some(path.clone());
                            }
                            if response.double_clicked() {
                                self.open_viewer = true;
                                opened = Some(path.clone());
                            }
                            response.context_menu(|ui| {
                                if ui.button("Abrir en visor").clicked() {
                                    self.open_viewer = true;
                                    opened = Some(path.clone());
                                    ui.close();
                                }
                                if ui.button("Copiar ruta").clicked() {
                                    ui.ctx().copy_text(path.to_string_lossy().into_owned());
                                    ui.close();
                                }
                            });
                        }
                    });
                }
            },
        );
        #[cfg(feature = "capture")]
        {
            self.measurement = frame;
        }
        if let Some((path, additive, range)) = clicked {
            self.select(&path, entries, additive, range);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_follow_visible_order_and_hidden_anchors_fall_back_to_click() {
        let entries: Vec<_> = ["a.png", "b.png", "c.png", "d.png"]
            .into_iter()
            .map(|name| FolderEntry {
                path: PathBuf::from(name),
            })
            .collect();
        let mut grid = ThumbnailGrid::new(
            &egui::Context::default(),
            MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET),
        );
        grid.select(&entries[1].path, &entries, false, false);
        grid.select(&entries[3].path, &entries, false, true);
        assert_eq!(
            grid.selected,
            entries[1..].iter().map(|e| e.path.clone()).collect()
        );
        grid.select(&entries[0].path, &entries, true, true);
        assert_eq!(grid.selected.len(), 4);
        grid.select(&entries[2].path, &entries[2..], false, true);
        assert_eq!(grid.selected, [entries[2].path.clone()].into());
        grid.select(&entries[2].path, &entries, true, false);
        assert!(grid.selected.is_empty());
        grid.select_all(&entries[2..]);
        assert_eq!(grid.selected.len(), 2);
        grid.clear_selection();
        assert!(grid.selected.is_empty());
        assert!(grid.anchor.is_none());
    }
}
