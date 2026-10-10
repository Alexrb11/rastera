use std::{path::PathBuf, sync::Arc};

use crossbeam_channel::{Receiver, bounded};
use eframe::egui::{self, Align, Color32, Layout, Rect, RichText, Sense, TextureHandle, Vec2};
use rastera_adapters::{LocalFolders, LocalImages};
use rastera_core::{CPU_IMAGE_BUDGET, ErrorKind, FolderEntry, ImageInfo};
use rastera_engine::{FolderScanner, FolderUpdate, ImageLoader, memory::MemoryBudget};

const ACCENT: Color32 = Color32::from_rgb(138, 211, 174);
const MUTED: Color32 = Color32::from_rgb(150, 165, 180);

struct Preview {
    texture: TextureHandle,
    size: Vec2,
    info: ImageInfo,
}

pub struct RasteraApp {
    catalog: crate::catalog::Catalog,
    metadata_dirty: bool,
    grid: crate::grid::ThumbnailGrid,
    grid_view: bool,
    loader: ImageLoader,
    scanner: FolderScanner,
    folder_generation: u64,
    folder: Option<PathBuf>,
    entries: Vec<FolderEntry>,
    folder_loading: bool,
    folder_error: Option<String>,
    generation: u64,
    path: Option<PathBuf>,
    path_input: String,
    preview: Option<Preview>,
    loading: bool,
    error: Option<String>,
    dialog: Option<Receiver<Option<PathBuf>>>,
    zoom: Option<f32>,
    current_scale: f32,
    pan: Vec2,
    #[cfg(feature = "capture")]
    capture_requested: bool,
}

impl RasteraApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let ctx = cc.egui_ctx.clone();
        let budget = MemoryBudget::new(CPU_IMAGE_BUDGET);
        let grid = crate::grid::ThumbnailGrid::new(&cc.egui_ctx, budget.clone());
        let loader = ImageLoader::new(
            Arc::new(LocalImages),
            budget,
            Arc::new(move || ctx.request_repaint()),
        );
        let ctx = cc.egui_ctx.clone();
        let scanner = FolderScanner::new(
            Arc::new(LocalFolders),
            Arc::new(move || ctx.request_repaint()),
        );
        let mut app = Self {
            catalog: crate::catalog::Catalog::new(&cc.egui_ctx),
            metadata_dirty: false,
            grid,
            grid_view: false,
            loader,
            scanner,
            folder_generation: 0,
            folder: None,
            entries: Vec::new(),
            folder_loading: false,
            folder_error: None,
            generation: 0,
            path: None,
            path_input: String::new(),
            preview: None,
            loading: false,
            error: None,
            dialog: None,
            zoom: None,
            current_scale: 1.0,
            pan: Vec2::ZERO,
            #[cfg(feature = "capture")]
            capture_requested: false,
        };
        if let Some(path) = initial {
            app.open(path);
        }
        app
    }

    fn open(&mut self, path: PathBuf) {
        let path = std::path::absolute(&path).unwrap_or(path);
        self.grid.reset();
        self.grid_view = true;
        self.folder_generation += 1;
        self.catalog.reset(self.folder_generation);
        self.metadata_dirty = false;
        self.generation += 1;
        self.loader.cancel();
        self.preview = None;
        self.path = None;
        self.path_input = path.to_string_lossy().into_owned();
        self.entries.clear();
        self.error = None;
        self.loading = false;
        self.folder_loading = true;
        self.folder_error = None;
        self.folder = Some(path.clone());
        self.scanner.request(self.folder_generation, path);
    }

    fn open_image(&mut self, path: PathBuf) {
        self.generation += 1;
        self.path_input = path.to_string_lossy().into_owned();
        self.path = Some(path.clone());
        self.preview = None;
        self.error = None;
        self.loading = true;
        self.zoom = None;
        self.pan = Vec2::ZERO;
        self.loader.request(self.generation, path, 4_096);
    }

    fn pick_file(&mut self, ctx: &egui::Context) {
        self.pick_path(ctx, false);
    }

    fn pick_path(&mut self, ctx: &egui::Context, folder: bool) {
        if self.dialog.is_some() {
            return;
        }
        let (tx, rx) = bounded(1);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let selected = if folder {
                rfd::FileDialog::new()
                    .set_title("Abrir carpeta en Rastera")
                    .pick_folder()
            } else {
                rfd::FileDialog::new()
                    .set_title("Abrir imagen en Rastera")
                    .add_filter("Imágenes", &["jpg", "jpeg", "png", "webp"])
                    .pick_file()
            };
            let _ = tx.send(selected);
            ctx.request_repaint();
        });
        self.dialog = Some(rx);
    }

    fn receive(&mut self, ctx: &egui::Context) {
        self.catalog.receive();
        self.grid.receive(ctx);
        if let Some(receiver) = &self.dialog {
            match receiver.try_recv() {
                Ok(path) => {
                    self.dialog = None;
                    if let Some(path) = path {
                        self.open(path);
                    }
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.dialog = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }
        while let Some(event) = self.scanner.try_recv() {
            if event.generation != self.folder_generation {
                continue;
            }
            match event.update {
                FolderUpdate::File(path) => {
                    self.grid_view = false;
                    self.folder = None;
                    self.open_image(path);
                }
                FolderUpdate::Page(page) => {
                    self.catalog.invalidate();
                    let first = if self.entries.is_empty() && self.path.is_none() {
                        page.first().map(|entry| entry.path.clone())
                    } else {
                        None
                    };
                    self.entries.extend(page);
                    if let Some(path) = first {
                        self.open_image(path);
                    }
                }
                FolderUpdate::Sorted(entries) => {
                    self.catalog.invalidate();
                    self.entries = entries;
                }
                FolderUpdate::Finished(result) => {
                    let paths = if self.entries.is_empty() {
                        self.path.iter().cloned().collect()
                    } else {
                        self.entries
                            .iter()
                            .map(|entry| entry.path.clone())
                            .collect()
                    };
                    self.catalog.load(paths);
                    self.folder_loading = false;
                    if let Err(error) = result
                        && error.kind != ErrorKind::Cancelled
                    {
                        self.folder_error = Some(error.to_string());
                        if self.entries.is_empty() {
                            self.folder = None;
                        }
                    }
                }
            }
        }
        while let Some(event) = self.loader.try_recv() {
            if event.generation != self.generation {
                continue;
            }
            self.loading = false;
            match event.result {
                Ok(image) => {
                    self.metadata_dirty = true;
                    let size = egui::vec2(image.width as f32, image.height as f32);
                    let pixels = egui::ColorImage::from_rgba_unmultiplied(
                        [image.width as usize, image.height as usize],
                        &image.rgba,
                    );
                    self.preview = Some(Preview {
                        texture: ctx.load_texture(
                            format!("preview-{}", self.generation),
                            pixels,
                            egui::TextureOptions::LINEAR,
                        ),
                        size,
                        info: image.info,
                    });
                }
                Err(error) if error.kind == ErrorKind::Cancelled => {}
                Err(error) => {
                    self.error = Some(error.to_string());
                }
            }
        }
        if self.metadata_dirty
            && self.catalog.ready
            && let (Some(path), Some(preview)) = (&self.path, &self.preview)
        {
            self.catalog.save_metadata(path, &preview.info);
            self.metadata_dirty = false;
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.pick_file(ctx);
        }
        if !ctx.egui_wants_keyboard_input() {
            if ctx.input(|input| input.key_pressed(egui::Key::ArrowRight)) {
                self.step(1);
            }
            if ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft)) {
                self.step(-1);
            }
            if ctx.input(|input| input.key_pressed(egui::Key::Home)) {
                self.zoom = None;
                self.pan = Vec2::ZERO;
            }
            if ctx.input(|input| {
                input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)
            }) {
                self.zoom =
                    Some((self.zoom.unwrap_or(self.current_scale) * 1.25).clamp(0.05, 16.0));
            }
            if ctx.input(|input| input.key_pressed(egui::Key::Minus)) {
                self.zoom =
                    Some((self.zoom.unwrap_or(self.current_scale) / 1.25).clamp(0.05, 16.0));
            }
        }
        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        });
        if let Some(path) = dropped {
            self.open(path);
        }
    }

    fn step(&mut self, delta: isize) {
        let current = self
            .entries
            .iter()
            .position(|entry| Some(&entry.path) == self.path.as_ref());
        if let Some(current) = current {
            let next = current.checked_add_signed(delta);
            if let Some(path) = next
                .and_then(|index| self.entries.get(index))
                .map(|entry| entry.path.clone())
            {
                self.open_image(path);
            }
        }
    }

    #[cfg(feature = "capture")]
    fn capture(&mut self, ctx: &egui::Context) {
        let Some(path) = std::env::var_os("RASTERA_CAPTURE_TO") else {
            return;
        };
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            let bytes: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            if let Err(error) = image::save_buffer(
                PathBuf::from(path),
                &bytes,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("No se pudo guardar la captura: {error}");
                std::process::exit(1);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if !self.capture_requested
            && !self.loading
            && !self.folder_loading
            && !self.grid.busy()
        {
            self.capture_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("RASTERA").size(24.0).strong().color(ACCENT));
            ui.add_space(16.0);
            ui.label(RichText::new("Tus imágenes, a tu ritmo").color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button("Abrir imagen · Ctrl+O").clicked() {
                    self.pick_file(ui.ctx());
                }
                if ui.button("Abrir carpeta").clicked() {
                    self.pick_path(ui.ctx(), true);
                }
                if self.folder.is_some() {
                    ui.selectable_value(&mut self.grid_view, true, "Cuadrícula");
                    ui.selectable_value(&mut self.grid_view, false, "Visor");
                    if ui.button("Actualizar").clicked()
                        && let Some(folder) = self.folder.clone()
                    {
                        self.open(folder);
                    }
                }
                if self.folder.is_none()
                    && ui
                        .add_enabled(self.catalog.ready, egui::Button::new("Organizar"))
                        .clicked()
                {
                    self.catalog.editor_open = true;
                }
            });
        });
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.path_input)
                    .hint_text("Ruta de una imagen o carpeta…")
                    .desired_width((ui.available_width() - 90.0).max(80.0)),
            );
            let enter =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (ui.button("Abrir").clicked() || enter) && !self.path_input.trim().is_empty() {
                self.open(PathBuf::from(self.path_input.trim()));
            }
        });
        ui.add_space(8.0);
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(205.0);
        ui.heading("Imagen");
        ui.add_space(12.0);
        if let Some(path) = &self.path {
            ui.label(
                RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong(),
            );
            ui.add_space(8.0);
        } else {
            ui.label(RichText::new("Abre una imagen para ver sus detalles.").color(MUTED));
        }
        if let Some(preview) = &self.preview {
            ui.label(format!(
                "{} × {} px",
                preview.info.width, preview.info.height
            ));
            ui.label(format!(
                "{} · {:.1} MiB",
                preview.info.kind,
                preview.info.stamp.length as f64 / 1_048_576.0
            ));
            if preview.info.has_color_profile {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Perfil ICC detectado. La gestión de color está pendiente.")
                        .color(Color32::YELLOW),
                );
            }
            if preview.size.x as u32 != preview.info.width
                || preview.size.y as u32 != preview.info.height
            {
                ui.add_space(8.0);
                ui.label(RichText::new("Puede incluir orientación EXIF o una vista reducida. Zoom sobre la vista previa.").color(MUTED));
            }
        }
        if let Some(folder) = &self.folder {
            ui.add_space(16.0);
            ui.separator();
            ui.label(RichText::new("CARPETA").small().color(MUTED));
            ui.label(
                folder
                    .file_name()
                    .unwrap_or(folder.as_os_str())
                    .to_string_lossy(),
            );
            ui.label(format!(
                "{} imágenes{}",
                self.entries.len(),
                if self.folder_loading {
                    " · leyendo…"
                } else {
                    ""
                }
            ));
            let mut chosen = None;
            let row_height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
            egui::ScrollArea::vertical()
                .id_salt("folder-list")
                .max_height(180.0)
                .show_rows(ui, row_height, self.entries.len(), |ui, rows| {
                    for index in rows {
                        let entry = &self.entries[index];
                        let selected = self.path.as_ref() == Some(&entry.path);
                        if ui
                            .add(
                                egui::Button::selectable(
                                    selected,
                                    entry.path.file_name().unwrap_or_default().to_string_lossy(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(entry.path.to_string_lossy())
                            .clicked()
                        {
                            chosen = Some(entry.path.clone());
                        }
                    }
                });
            if let Some(path) = chosen {
                self.open_image(path);
            }
        }
        ui.add_space(20.0);
        ui.separator();
        ui.add_space(12.0);
        ui.label(RichText::new("CONTROLES").small().color(MUTED));
        ui.label("Rueda: ampliar o reducir");
        ui.label("Arrastrar: desplazar");
        ui.label("Home: ajustar a ventana");
        ui.label("+ / −: cambiar zoom");
        ui.label("Flechas: anterior o siguiente");
        ui.add_space(14.0);
        ui.label(RichText::new("Los originales permanecen intactos.").color(MUTED));
    }

    fn viewer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if !self.entries.is_empty() {
                if ui.button("Anterior").clicked() {
                    self.step(-1);
                }
                if ui.button("Siguiente").clicked() {
                    self.step(1);
                }
                ui.separator();
            }
            if ui.button("Ajustar").clicked() {
                self.zoom = None;
                self.pan = Vec2::ZERO;
            }
            if ui.button("100%").clicked() {
                self.zoom = Some(1.0);
                self.pan = Vec2::ZERO;
            }
            if ui.button("−").clicked() {
                self.zoom =
                    Some((self.zoom.unwrap_or(self.current_scale) / 1.25).clamp(0.05, 16.0));
            }
            if ui.button("+").clicked() {
                self.zoom =
                    Some((self.zoom.unwrap_or(self.current_scale) * 1.25).clamp(0.05, 16.0));
            }
            ui.label(self.zoom.map_or_else(
                || "Ajuste a ventana".to_owned(),
                |zoom| format!("{:.0}% de la vista previa", zoom * 100.0),
            ));
        });
        ui.add_space(8.0);
        let (canvas, response) = ui.allocate_exact_size(ui.available_size(), Sense::drag());
        let painter = ui.painter().with_clip_rect(canvas);
        painter.rect_filled(canvas, 8.0, Color32::from_rgb(16, 22, 29));
        if let Some(preview) = &self.preview {
            let fit = ((canvas.width() - 32.0) / preview.size.x)
                .min((canvas.height() - 32.0) / preview.size.y)
                .clamp(0.001, 1.0);
            if response.hovered() {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    self.zoom =
                        Some((self.zoom.unwrap_or(fit) * (scroll * 0.003).exp()).clamp(0.05, 16.0));
                }
            }
            if response.dragged() {
                self.pan += response.drag_delta();
            }
            if response.double_clicked() {
                self.zoom = None;
                self.pan = Vec2::ZERO;
            }
            let size = preview.size * self.zoom.unwrap_or(fit);
            self.current_scale = self.zoom.unwrap_or(fit);
            let image_rect = Rect::from_center_size(canvas.center() + self.pan, size);
            // Checkerboard only in the visible image area.
            let checker = image_rect.intersect(canvas);
            let step = 20.0;
            for row in 0..(checker.height() / step).ceil() as i32 {
                for column in 0..(checker.width() / step).ceil() as i32 {
                    let rect = Rect::from_min_size(
                        checker.min + egui::vec2(column as f32 * step, row as f32 * step),
                        egui::vec2(step, step),
                    )
                    .intersect(checker);
                    painter.rect_filled(
                        rect,
                        0.0,
                        if (row + column) % 2 == 0 {
                            Color32::from_gray(38)
                        } else {
                            Color32::from_gray(48)
                        },
                    );
                }
            }
            painter.image(
                preview.texture.id(),
                image_rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            let message = if self.loading || self.folder_loading {
                "Cargando imagen…"
            } else if self.error.is_some() {
                "No se pudo abrir esta imagen"
            } else if self.folder_error.is_some() {
                "No se pudo abrir esta ruta"
            } else if self.folder.is_some() {
                "No hay imágenes compatibles en esta carpeta"
            } else {
                "Arrastra una imagen aquí"
            };
            let text_width = (canvas.width() - 48.0).max(1.0);
            let heading = painter.layout(
                message.to_owned(),
                egui::FontId::proportional(26.0),
                Color32::WHITE,
                text_width,
            );
            let hint = painter.layout(
                "JPEG · PNG · WebP  |  Ctrl+O para abrir".to_owned(),
                egui::FontId::proportional(15.0),
                MUTED,
                text_width,
            );
            let height = heading.size().y + hint.size().y + 16.0;
            let top = canvas.center().y - height * 0.5;
            let hint_top = top + heading.size().y + 16.0;
            painter.galley(
                egui::pos2(canvas.center().x - heading.size().x * 0.5, top),
                heading,
                Color32::WHITE,
            );
            painter.galley(
                egui::pos2(canvas.center().x - hint.size().x * 0.5, hint_top),
                hint,
                MUTED,
            );
        }
    }
}

impl eframe::App for RasteraApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        self.shortcuts(&ctx);
        self.header(ui);
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).color(Color32::from_rgb(255, 155, 140)));
        }
        if let Some(error) = &self.folder_error {
            ui.label(RichText::new(error).color(Color32::from_rgb(255, 155, 140)));
        }
        let height = (ui.available_height() - 30.0).max(100.0);
        ui.horizontal_top(|ui| {
            egui::Frame::NONE
                .fill(Color32::from_rgb(26, 33, 42))
                .inner_margin(16)
                .show(ui, |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(205.0, height - 32.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_height(height - 32.0);
                            self.inspector(ui);
                        },
                    );
                });
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                Layout::top_down(Align::Min),
                |ui| {
                    if self.grid_view && !self.entries.is_empty() {
                        self.catalog.filters(ui, &self.entries);
                        if let Some(path) = self.grid.show(ui, &self.catalog.filtered, self.loading)
                        {
                            self.open_image(path);
                            self.grid_view = !self.grid.open_viewer;
                        }
                    } else {
                        self.viewer(ui);
                    }
                },
            );
        });
        self.catalog.editor(
            &ctx,
            self.path.as_deref(),
            self.preview.as_ref().map(|preview| &preview.info),
        );
        if let Some(error) = &self.catalog.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if self.loading || self.folder_loading || self.grid.busy() {
                    "Cargando…"
                } else {
                    "Listo"
                })
                .small()
                .color(MUTED),
            );
            if self.dialog.is_some() {
                ui.label("Seleccionando archivo…");
            }
        });
        #[cfg(feature = "capture")]
        self.capture(&ctx);
    }
}
