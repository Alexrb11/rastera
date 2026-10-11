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
    source_size: Option<(u32, u32)>,
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
    pub organize: bool,
    pub export: bool,
    anchor: Option<PathBuf>,
    focused: Option<PathBuf>,
    scroll_offset: f32,
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
            organize: false,
            export: false,
            anchor: None,
            focused: None,
            scroll_offset: 0.0,
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
        self.focused = None;
        self.scroll_offset = 0.0;
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn backend(&self) -> Arc<CachedThumbnails> {
        self.backend.clone()
    }
    pub fn reconcile(&mut self, entries: &[FolderEntry], changed: &HashSet<PathBuf>) {
        let paths: HashSet<_> = entries.iter().map(|entry| &entry.path).collect();
        self.cache
            .retain(|path, _| paths.contains(path) && !changed.contains(path));
        self.selected.retain(|path| paths.contains(path));
        if self
            .anchor
            .as_ref()
            .is_some_and(|path| !paths.contains(path))
        {
            self.anchor = None;
        }
        if self
            .focused
            .as_ref()
            .is_some_and(|path| !paths.contains(path))
        {
            self.focused = None;
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|path| !paths.contains(path) || changed.contains(path))
        {
            self.generation += 1;
            self.loader.cancel();
            self.pending = None;
        }
    }
    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }
    pub fn select_all(&mut self, entries: &[FolderEntry]) {
        self.selected = entries.iter().map(|entry| entry.path.clone()).collect();
        self.anchor = entries.first().map(|entry| entry.path.clone());
    }
    fn navigate(
        &mut self,
        entries: &[FolderEntry],
        current: Option<&std::path::Path>,
        key: egui::Key,
        stride: usize,
        modifiers: egui::Modifiers,
    ) -> Option<usize> {
        let last = entries.len().checked_sub(1)?;
        let position = self
            .focused
            .as_deref()
            .or(current)
            .and_then(|path| entries.iter().position(|entry| entry.path == path));
        let index = match key {
            egui::Key::Home => 0,
            egui::Key::End => last,
            egui::Key::ArrowLeft | egui::Key::ArrowUp | egui::Key::PageUp => {
                position.unwrap_or(0).saturating_sub(stride)
            }
            _ => position.map_or(0, |position| position.saturating_add(stride).min(last)),
        };
        let path = &entries[index].path;
        if modifiers.shift {
            if self.anchor.is_none() {
                self.anchor = position.map(|position| entries[position].path.clone());
            }
            self.select(path, entries, modifiers.command, true);
        } else if !modifiers.command {
            self.select(path, entries, false, false);
        }
        self.focused = Some(path.clone());
        Some(index)
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
                    source_size: Some(image.source_size),
                },
                Err(error) => Thumbnail {
                    texture: None,
                    size: Vec2::ZERO,
                    error: Some(error.to_string()),
                    touched: self.clock,
                    source_size: None,
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
        keyboard_enabled: bool,
    ) -> Option<PathBuf> {
        use crate::theme;
        let mut opened = None;
        let mut clicked = None;
        self.open_viewer = false;
        self.organize = false;
        self.export = false;
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
            + 46.0;
        let keyboard = ui.interact(
            ui.available_rect_before_wrap(),
            egui::Id::new("grid-keyboard"),
            Sense::focusable_noninteractive(),
        );
        keyboard.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, keyboard_enabled,
            format!("Biblioteca, {} imágenes. Foco: {}. Flechas para navegar, Espacio para seleccionar, Enter para abrir.", entries.len(), self.focused.as_deref().or(current).and_then(|path| path.file_name()).unwrap_or_default().to_string_lossy())));
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                keyboard.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                    ..Default::default()
                },
            )
        });
        let mut keyboard_index = None;
        if keyboard_enabled && (keyboard.has_focus() || !ui.ctx().egui_wants_keyboard_input()) {
            let modifiers = ui.input(|input| input.modifiers);
            if !modifiers.alt {
                let page = (ui.available_height() / (row_height + ui.spacing().item_spacing.y))
                    .floor()
                    .max(1.0) as usize
                    * columns;
                for (key, stride) in [
                    (egui::Key::ArrowLeft, 1),
                    (egui::Key::ArrowRight, 1),
                    (egui::Key::ArrowUp, columns),
                    (egui::Key::ArrowDown, columns),
                    (egui::Key::PageUp, page),
                    (egui::Key::PageDown, page),
                    (egui::Key::Home, 1),
                    (egui::Key::End, 1),
                ] {
                    if ui.input_mut(|input| input.consume_key(modifiers, key)) {
                        keyboard_index = self.navigate(entries, current, key, stride, modifiers);
                        opened = keyboard_index.map(|index| entries[index].path.clone());
                        keyboard.request_focus();
                        break;
                    }
                }
                if ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
                    self.select_all(entries);
                }
                if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    self.clear_selection();
                }
                let focused = self
                    .focused
                    .as_deref()
                    .or(current)
                    .and_then(|path| entries.iter().find(|entry| entry.path == path))
                    .map(|entry| entry.path.clone());
                if let Some(path) = focused {
                    if ui.input_mut(|input| input.consume_key(modifiers, egui::Key::Space)) {
                        self.select(&path, entries, true, false);
                        keyboard.request_focus();
                        opened = Some(path.clone());
                    }
                    if ui.input_mut(|input| input.consume_key(modifiers, egui::Key::Enter)) {
                        self.open_viewer = true;
                        opened = Some(path);
                    }
                }
            }
        }
        let mut wanted = None;
        let mut pending_visible = false;
        #[cfg(feature = "capture")]
        let mut frame = GridFrame::default();
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt(("thumbnail-grid", self.density))
            .auto_shrink([false, false]);
        if let Some(index) = keyboard_index {
            let top = (index / columns) as f32 * (row_height + ui.spacing().item_spacing.y);
            let bottom = top + row_height;
            let height = ui.available_height();
            let offset = if top < self.scroll_offset {
                top
            } else if bottom > self.scroll_offset + height {
                bottom - height
            } else {
                self.scroll_offset
            };
            scroll = scroll.vertical_scroll_offset(offset.max(0.0));
        }
        #[cfg(feature = "capture")]
        let scroll = if let Some(index) = self.benchmark_index {
            let maximum = (entries.len().div_ceil(columns) as f32 * row_height
                - ui.available_height())
            .max(0.0);
            scroll.vertical_scroll_offset(((index / columns) as f32 * row_height).min(maximum))
        } else {
            scroll
        };
        let output = scroll.show_rows(
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
                                Sense::CLICK,
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::SelectableLabel,
                                    true,
                                    selected,
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                )
                            });
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
                                    if selected {
                                        2.0
                                    } else if current == Some(path.as_path()) {
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
                            if keyboard.has_focus()
                                && self.focused.as_deref().or(current) == Some(path.as_path())
                            {
                                ui.painter().rect_stroke(
                                    rect.shrink(3.0),
                                    1.0,
                                    egui::Stroke::new(1.0, theme::TEXT),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            let area = Rect::from_min_max(
                                rect.min + egui::vec2(5.0, 5.0),
                                rect.max - egui::vec2(5.0, 42.0),
                            );
                            ui.painter().rect_filled(area, 1.0, theme::CANVAS);
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
                                egui::pos2(rect.min.x + 5.0, rect.max.y - 37.0),
                                galley,
                                theme::TEXT,
                            );
                            let kind = path
                                .extension()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_ascii_uppercase();
                            let details = self
                                .cache
                                .get(path)
                                .and_then(|thumbnail| thumbnail.source_size)
                                .map_or(kind.clone(), |(width, height)| {
                                    format!("{kind} · {width} × {height}")
                                });
                            ui.painter().text(
                                rect.left_bottom() + egui::vec2(5.0, -8.0),
                                egui::Align2::LEFT_BOTTOM,
                                details,
                                egui::FontId::proportional(10.0),
                                theme::MUTED,
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
                                    area.right_top() - egui::vec2(26.0, -4.0),
                                    egui::vec2(22.0, 22.0),
                                );
                                ui.painter().rect_filled(badge, 2.0, theme::SURFACE);
                                ui.painter().text(
                                    badge.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "★",
                                    egui::FontId::proportional(15.0),
                                    Color32::from_rgb(225, 190, 114),
                                );
                            }
                            response.clone().on_hover_text(format!(
                                "{name}\n{}\nCtrl+clic: añadir · Shift+clic: rango",
                                path.display()
                            ));
                            if response.clicked() {
                                keyboard.request_focus();
                                let modifiers = ui.input(|input| input.modifiers);
                                clicked = Some((path.clone(), modifiers.command, modifiers.shift));
                                opened = Some(path.clone());
                            }
                            if response.secondary_clicked() && !selected {
                                clicked = Some((path.clone(), false, false));
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
                                ui.separator();
                                if ui
                                    .add_enabled(
                                        catalog.can_edit(),
                                        egui::Button::new("Organizar imagen…"),
                                    )
                                    .clicked()
                                {
                                    self.organize = true;
                                    opened = Some(path.clone());
                                    ui.close();
                                }
                                if ui.button("Exportar selección…").clicked() {
                                    self.export = true;
                                    opened = Some(path.clone());
                                    ui.close();
                                }
                            });
                        }
                    });
                }
            },
        );
        self.scroll_offset = output.state.offset.y;
        #[cfg(feature = "capture")]
        {
            self.measurement = frame;
        }
        if let Some((path, additive, range)) = clicked {
            self.focused = Some(path.clone());
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
    fn reconciliation_preserves_unchanged_textures_scroll_and_hidden_selection() {
        let ctx = egui::Context::default();
        let mut grid = ThumbnailGrid::new(&ctx, MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET));
        let entries = ["keep.png", "changed.png", "removed.png"].map(|name| FolderEntry {
            path: name.into(),
            stamp: None,
        });
        for entry in &entries {
            grid.cache.insert(
                entry.path.clone(),
                Thumbnail {
                    texture: None,
                    size: Vec2::ZERO,
                    error: None,
                    touched: 0,
                    source_size: None,
                },
            );
            grid.selected.insert(entry.path.clone());
        }
        grid.anchor = Some(entries[0].path.clone());
        grid.focused = Some(entries[2].path.clone());
        grid.scroll_offset = 328.0;
        grid.pending = Some(entries[1].path.clone());
        let generation = grid.generation;
        grid.reconcile(&entries[..2], &HashSet::from([entries[1].path.clone()]));
        assert_eq!(grid.cache.len(), 1);
        assert!(grid.cache.contains_key(&entries[0].path));
        assert_eq!(grid.selected.len(), 2);
        assert_eq!(grid.anchor.as_ref(), Some(&entries[0].path));
        assert!(grid.focused.is_none() && grid.pending.is_none());
        assert!(grid.generation > generation);
        assert_eq!(grid.scroll_offset, 328.0);
    }
    #[test]
    fn keyboard_ranges_preserve_anchor_and_ctrl_moves_without_replacing_selection() {
        let ctx = egui::Context::default();
        let mut grid = ThumbnailGrid::new(&ctx, MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET));
        let entries: Vec<_> = (0..10)
            .map(|index| FolderEntry {
                path: format!("{index}.png").into(),
                stamp: None,
            })
            .collect();
        assert_eq!(
            grid.navigate(
                &entries,
                None,
                egui::Key::ArrowRight,
                1,
                egui::Modifiers::NONE
            ),
            Some(0)
        );
        assert_eq!(
            grid.navigate(
                &entries,
                None,
                egui::Key::ArrowDown,
                3,
                egui::Modifiers::SHIFT
            ),
            Some(3)
        );
        assert_eq!(grid.selected.len(), 4);
        grid.navigate(
            &entries,
            None,
            egui::Key::ArrowUp,
            3,
            egui::Modifiers::SHIFT,
        );
        assert_eq!(grid.selected, [entries[0].path.clone()].into());
        grid.navigate(&entries, None, egui::Key::End, 1, egui::Modifiers::COMMAND);
        assert_eq!(grid.focused, Some(entries[9].path.clone()));
        assert_eq!(grid.selected, [entries[0].path.clone()].into());
        grid.navigate(
            &entries,
            None,
            egui::Key::ArrowDown,
            3,
            egui::Modifiers::NONE,
        );
        assert_eq!(grid.selected, [entries[9].path.clone()].into());
        grid.navigate(
            &entries[1..3],
            None,
            egui::Key::ArrowRight,
            1,
            egui::Modifiers::NONE,
        );
        assert_eq!(grid.focused, Some(entries[1].path.clone()));
        assert_eq!(
            grid.navigate(&[], None, egui::Key::End, 1, egui::Modifiers::NONE),
            None
        );
    }
    #[test]
    fn keyboard_events_scroll_open_and_respect_text_focus_and_disabled_grid() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let catalog = crate::catalog::Catalog::for_test(dir.path().join("catalog.sqlite"));
        let mut grid = ThumbnailGrid::new(&ctx, MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET));
        let entries: Vec<_> = (0..100)
            .map(|index| FolderEntry {
                path: dir.path().join(format!("{index}.png")),
                stamp: None,
            })
            .collect();
        let render = |grid: &mut ThumbnailGrid,
                      key: Option<egui::Key>,
                      enabled: bool,
                      search_focus: bool| {
            let events = key
                .map(|key| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .into_iter()
                .collect();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Default::default(),
                    egui::vec2(640.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            let mut opened = None;
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut String::new())
                            .id(egui::Id::new("search-test")),
                    );
                    if search_focus {
                        response.request_focus();
                    }
                    opened = grid.show(ui, &entries, true, None, &catalog, enabled);
                });
            });
            output.textures_delta.clear();
            opened
        };
        render(&mut grid, None, true, false);
        assert_eq!(
            render(&mut grid, Some(egui::Key::End), true, false),
            Some(entries[99].path.clone())
        );
        assert!(grid.scroll_offset > 0.0);
        assert_eq!(
            render(&mut grid, Some(egui::Key::Enter), true, false),
            Some(entries[99].path.clone())
        );
        assert!(grid.open_viewer);
        render(&mut grid, Some(egui::Key::Home), false, false);
        assert_eq!(grid.focused, Some(entries[99].path.clone()));
        render(&mut grid, None, true, true);
        render(&mut grid, Some(egui::Key::Home), true, true);
        assert_eq!(grid.focused, Some(entries[99].path.clone()));
        assert!(!grid.open_viewer);
    }
    #[test]
    fn ranges_follow_visible_order_and_hidden_anchors_fall_back_to_click() {
        let entries: Vec<_> = ["a.png", "b.png", "c.png", "d.png"]
            .into_iter()
            .map(|name| FolderEntry {
                path: PathBuf::from(name),
                stamp: None,
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
