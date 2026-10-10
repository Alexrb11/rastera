use std::{path::PathBuf, sync::Arc};

use crossbeam_channel::{Receiver, bounded};
use eframe::egui::{self, Align, Color32, Layout, Rect, RichText, Sense, TextureHandle, Vec2};
use rastera_adapters::{LocalFolders, LocalImages};
use rastera_core::{CPU_IMAGE_BUDGET, ErrorKind, FolderEntry, ImageInfo};
use rastera_engine::{FolderScanner, FolderUpdate, ImageLoader, memory::MemoryBudget};

use crate::theme::{self, ACCENT, MUTED};

struct Preview {
    texture: TextureHandle,
    size: Vec2,
    source_size: Vec2,
    info: ImageInfo,
}

pub struct RasteraApp {
    inspector_open: bool,
    details_popup: bool,
    route_open: bool,
    shortcuts_open: bool,
    diagnostics_open: bool,
    detail: crate::detail::DetailView,
    batch: crate::batch::Batch,
    budget: MemoryBudget,
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
    #[cfg(feature = "capture")]
    capture_ready_at: Option<std::time::Instant>,
}

impl RasteraApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        theme::configure(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let budget = MemoryBudget::new(CPU_IMAGE_BUDGET);
        let grid = crate::grid::ThumbnailGrid::new(&cc.egui_ctx, budget.clone());
        let loader = ImageLoader::new(
            Arc::new(LocalImages),
            budget.clone(),
            Arc::new(move || ctx.request_repaint()),
        );
        let ctx = cc.egui_ctx.clone();
        let scanner = FolderScanner::new(
            Arc::new(LocalFolders),
            Arc::new(move || ctx.request_repaint()),
        );
        let mut app = Self {
            inspector_open: true,
            details_popup: false,
            route_open: false,
            shortcuts_open: false,
            diagnostics_open: false,
            detail: crate::detail::DetailView::new(&cc.egui_ctx, budget.clone()),
            batch: crate::batch::Batch::new(&cc.egui_ctx, budget.clone()),
            budget,
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
            #[cfg(feature = "capture")]
            capture_ready_at: None,
        };
        if let Some(path) = initial {
            app.open(path);
        }
        #[cfg(feature = "capture")]
        if std::env::var_os("RASTERA_CAPTURE_PANEL").as_deref()
            == Some(std::ffi::OsStr::new("performance"))
        {
            app.diagnostics_open = true;
        }
        app
    }

    fn open(&mut self, path: PathBuf) {
        self.detail.clear();
        let path = std::path::absolute(&path).unwrap_or(path);
        self.grid.reset();
        self.grid_view = true;
        self.folder_generation += 1;
        self.catalog.reset(self.folder_generation);
        self.metadata_dirty = false;
        self.generation += 1;
        self.loader.cancel();
        self.budget.set_foreground_loading(false);
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
        self.detail.clear();
        self.budget.set_foreground_loading(true);
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
        self.batch.receive();
        if (self.batch.busy || self.grid_view) && self.detail.active() {
            self.detail.clear();
        }
        self.detail.receive();
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
                    #[cfg(feature = "capture")]
                    if std::env::var_os("RASTERA_CAPTURE_SELECTION").is_some() {
                        self.grid
                            .select_all(&self.entries[..self.entries.len().min(3)]);
                    }
                    #[cfg(feature = "capture")]
                    if let Ok(search) = std::env::var("RASTERA_CAPTURE_FILTER") {
                        self.catalog.capture_search(search);
                    }
                    #[cfg(feature = "capture")]
                    if std::env::var_os("RASTERA_CAPTURE_PANEL").as_deref()
                        == Some(std::ffi::OsStr::new("batch"))
                        && let Some(destination) = std::env::var_os("RASTERA_CAPTURE_DEST")
                    {
                        self.batch.capture_plan(
                            self.entries
                                .iter()
                                .map(|entry| entry.path.clone())
                                .collect(),
                            PathBuf::from(destination),
                        );
                    }
                    #[cfg(feature = "capture")]
                    if let Some(journal) = std::env::var_os("RASTERA_CAPTURE_JOURNAL") {
                        self.batch.capture_recovery(PathBuf::from(journal));
                    }
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
            self.budget.set_foreground_loading(false);
            match event.result {
                Ok(image) => {
                    self.metadata_dirty = true;
                    let size = egui::vec2(image.width as f32, image.height as f32);
                    let pixels = egui::ColorImage::from_rgba_unmultiplied(
                        [image.width as usize, image.height as usize],
                        &image.rgba,
                    );
                    self.preview = Some(Preview {
                        source_size: egui::vec2(
                            image.source_size.0 as f32,
                            image.source_size.1 as f32,
                        ),
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
        #[cfg(feature = "capture")]
        if self.preview.is_some() && std::env::var_os("RASTERA_CAPTURE_FULL").is_some() {
            self.grid_view = false;
            self.zoom = Some(1.0);
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
        if ctx.input_mut(|input| {
            input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::O,
            )
        }) {
            self.pick_path(ctx, true);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.pick_file(ctx);
        }
        if !ctx.egui_wants_keyboard_input() {
            if self.grid_view
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::A))
            {
                self.grid.select_all(&self.catalog.filtered);
            }
            if ctx.input(|input| input.key_pressed(egui::Key::Escape))
                && !self.batch.open
                && !self.catalog.editor_open
                && !self.route_open
                && !self.details_popup
                && !self.shortcuts_open
                && !self.diagnostics_open
            {
                if self.grid_view {
                    self.grid.clear_selection();
                } else if self.folder.is_some() {
                    self.grid_view = true;
                }
            }
            if !self.grid_view && ctx.input(|input| input.key_pressed(egui::Key::ArrowRight)) {
                self.step(1);
            }
            if !self.grid_view && ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft)) {
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
            && (self.path.is_none() || self.catalog.ready)
            && !self.grid.busy()
            && !self.batch.busy
            && !self.detail.busy()
        {
            let ready_at = self
                .capture_ready_at
                .get_or_insert_with(std::time::Instant::now);
            if ready_at.elapsed() >= std::time::Duration::from_millis(300) {
                self.capture_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if self.capture_requested {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn export_selection(&mut self) {
        let paths = if self.grid.selected.is_empty() {
            self.path.iter().cloned().collect()
        } else {
            self.grid.selected.iter().cloned().collect()
        };
        self.batch.select(paths);
    }

    fn auxiliary_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.route_open;
        let mut accepted = false;
        egui::Window::new("Abrir una ruta")
            .open(&mut open)
            .collapsible(false)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.label("Pega la ruta de una imagen o carpeta.");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.path_input).desired_width(f32::INFINITY),
                );
                accepted = ui
                    .add_enabled(!self.path_input.trim().is_empty(), theme::primary("Abrir"))
                    .clicked()
                    || (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        && !self.path_input.trim().is_empty());
            });
        if accepted {
            self.open(PathBuf::from(self.path_input.trim()));
            open = false;
        }
        self.route_open = open;
        egui::Window::new("Atajos de teclado")
            .open(&mut self.shortcuts_open)
            .collapsible(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                egui::Grid::new("shortcuts")
                    .spacing([24.0, 10.0])
                    .show(ui, |ui| {
                        for (key, action) in [
                            ("Ctrl+O", "Abrir imagen"),
                            ("Ctrl+Shift+O", "Abrir carpeta"),
                            ("Ctrl+A", "Seleccionar los resultados del filtro"),
                            ("Ctrl+clic", "Añadir o quitar de la selección"),
                            ("Shift+clic", "Seleccionar un rango"),
                            ("Esc", "Deseleccionar / volver a Biblioteca"),
                            ("← / →", "Imagen anterior / siguiente en el visor"),
                            ("Home", "Ajustar a ventana"),
                            ("+ / −", "Ampliar / reducir"),
                        ] {
                            ui.label(RichText::new(key).color(ACCENT));
                            ui.label(action);
                            ui.end_row();
                        }
                    });
            });
        let mut details = self.details_popup;
        egui::Window::new("Propiedades de imagen")
            .open(&mut details)
            .collapsible(false)
            .default_width(280.0)
            .vscroll(true)
            .show(ctx, |ui| self.inspector(ui));
        self.details_popup = details;
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.3).max(20.0));
            ui.heading("Abrir imágenes");
            ui.label(
                RichText::new("Abre una carpeta o arrastra imágenes a esta ventana.").color(MUTED),
            );
            ui.add_space(8.0);
            if ui.button("Abrir carpeta…").clicked() {
                self.pick_path(ui.ctx(), true);
            }
            if ui.button("Abrir imagen…").clicked() {
                self.pick_file(ui.ctx());
            }
            ui.add_space(8.0);
            ui.label(RichText::new("JPEG · PNG · WebP").small().color(MUTED));
        });
    }
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Rastera").strong());
            ui.separator();
            ui.menu_button("Archivo", |ui| {
                if ui.button("Abrir carpeta…      Ctrl+Shift+O").clicked() {
                    self.pick_path(ui.ctx(), true);
                    ui.close();
                }
                if ui.button("Abrir imagen…       Ctrl+O").clicked() {
                    self.pick_file(ui.ctx());
                    ui.close();
                }
                if ui.button("Abrir una ruta…").clicked() {
                    self.route_open = true;
                    ui.close();
                }
            });
            ui.menu_button("Ver", |ui| {
                if self.folder.is_some() {
                    if ui.selectable_label(self.grid_view, "Biblioteca").clicked() {
                        self.grid_view = true;
                        ui.close();
                    }
                    if ui.selectable_label(!self.grid_view, "Visor").clicked() {
                        self.grid_view = false;
                        ui.close();
                    }
                    ui.separator();
                }
                if ui.ctx().viewport_rect().width() >= 980.0 {
                    ui.checkbox(&mut self.inspector_open, "Panel de propiedades");
                } else if ui.button("Propiedades de imagen…").clicked() {
                    self.details_popup = true;
                    ui.close();
                }
                if ui.button("Atajos de teclado…").clicked() {
                    self.shortcuts_open = true;
                    ui.close();
                }
            });
            ui.menu_button("Herramientas", |ui| {
                if ui
                    .add_enabled(
                        !self.grid.selected.is_empty() || self.path.is_some() || self.batch.busy,
                        egui::Button::new("Exportar imágenes…"),
                    )
                    .clicked()
                {
                    self.export_selection();
                    ui.close();
                }
                if ui.button("Recuperar / ver lote…").clicked() {
                    self.batch.open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Diagnóstico de rendimiento…").clicked() {
                    self.diagnostics_open = true;
                    ui.close();
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(self.dialog.is_none(), egui::Button::new("Abrir imagen…"))
                    .on_hover_text("Ctrl+O")
                    .clicked()
                {
                    self.pick_file(ui.ctx());
                }
                if ui
                    .add_enabled(self.dialog.is_none(), egui::Button::new("Abrir carpeta…"))
                    .on_hover_text("Ctrl+Shift+O")
                    .clicked()
                {
                    self.pick_path(ui.ctx(), true);
                }
            });
        });
        ui.separator();
        if self.path.is_some() || self.folder.is_some() {
            ui.horizontal(|ui| {
                if self.folder.is_some() {
                    ui.selectable_value(&mut self.grid_view, true, "Biblioteca");
                    ui.selectable_value(&mut self.grid_view, false, "Visor");
                    if ui
                        .button("Actualizar")
                        .on_hover_text("Vuelve a leer la carpeta y limpia la selección")
                        .clicked()
                        && let Some(folder) = self.folder.clone()
                    {
                        self.open(folder);
                    }
                    ui.separator();
                }
                let path = self
                    .folder
                    .as_ref()
                    .or(self.path.as_ref())
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let width = (ui.available_width() - 93.0).max(60.0);
                if ui
                    .add_sized([width, 26.0], egui::Button::new(&path).truncate())
                    .on_hover_text("Abrir otra ruta")
                    .clicked()
                {
                    self.route_open = true;
                }
                let compact = ui.ctx().viewport_rect().width() < 980.0;
                if ui
                    .selectable_label(self.inspector_open && !compact, "Propiedades")
                    .clicked()
                {
                    if compact {
                        self.details_popup = true;
                    } else {
                        self.inspector_open = !self.inspector_open;
                    }
                }
            });
            ui.add_space(4.0);
        }
    }
    fn library(&mut self, ui: &mut egui::Ui) {
        self.catalog.filters(ui, &self.entries);
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!self.catalog.filtered.is_empty(), egui::Button::new("Seleccionar todo")).on_hover_text("Ctrl+A · Selecciona los resultados del filtro").clicked() {
                self.grid.select_all(&self.catalog.filtered);
            }
            if ui.add_enabled(!self.grid.selected.is_empty(), egui::Button::new("Deseleccionar"))
                .on_hover_text("Esc · Limpia también las seleccionadas fuera del filtro").clicked() {
                self.grid.clear_selection();
            }
            ui.label(RichText::new(format!("{} seleccionadas", self.grid.selected.len())).color(ACCENT));
            let hidden = self.grid.selected.len() - self.catalog.filtered.iter()
                .filter(|entry| self.grid.selected.contains(&entry.path)).count();
            if hidden > 0 {
                ui.label(RichText::new(format!("{hidden} fuera del filtro")).small().color(MUTED))
                    .on_hover_text("La exportación incluye estas imágenes. Deseleccionar limpia toda la selección.");
            }
            if ui.add_enabled(!self.grid.selected.is_empty() || self.batch.busy,
                theme::primary(if self.batch.busy { "Ver lote".to_owned() } else {
                    format!("Exportar selección ({})", self.grid.selected.len())
                })).clicked() {
                if self.batch.busy { self.batch.open = true; }
                else { self.export_selection(); }
            }
            egui::ComboBox::from_id_salt("tile-density").selected_text(match self.grid.density {
                0 => "Miniaturas pequeñas", 2 => "Miniaturas grandes", _ => "Miniaturas medianas",
            }).show_ui(ui, |ui| {
                for (value, label) in [(0, "Compacta"), (1, "Mediana"), (2, "Amplia")] {
                    ui.selectable_value(&mut self.grid.density, value, label);
                }
            });
        });
        ui.add_space(8.0);
        if self.entries.is_empty() {
            ui.add_space(36.0);
            ui.heading(if self.folder_loading {
                "Leyendo la carpeta…"
            } else {
                "Esta carpeta está vacía"
            });
            ui.label(
                RichText::new("Aquí aparecerán las imágenes JPEG, PNG y WebP de esta carpeta.")
                    .color(MUTED),
            );
        } else if self.catalog.filtered.is_empty() {
            ui.add_space(36.0);
            ui.heading("No hay imágenes que coincidan");
            ui.label(RichText::new("Prueba otro nombre, etiqueta o formato.").color(MUTED));
            if ui.button("Restablecer filtros").clicked() {
                self.catalog.clear_filters();
            }
        } else if let Some(path) = self.grid.show(
            ui,
            &self.catalog.filtered,
            self.loading,
            self.path.as_deref(),
            &self.catalog,
        ) {
            if self.path.as_ref() != Some(&path) {
                self.open_image(path);
            }
            if self.grid.open_viewer {
                self.grid_view = false;
            }
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        theme::eyebrow(ui, "PROPIEDADES");
        ui.add_space(12.0);
        if let Some(preview) = &self.preview {
            let width = ui.available_width();
            let size = preview.size * (width / preview.size.x).min(110.0 / preview.size.y);
            ui.add(egui::Image::new((preview.texture.id(), size)).corner_radius(0.0));
            ui.add_space(12.0);
        }
        if let Some(path) = &self.path {
            ui.add(
                egui::Label::new(
                    RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong(),
                )
                .wrap(),
            );
            ui.add_space(8.0);
            if let Some(preview) = &self.preview {
                egui::Grid::new("image-properties")
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        for (label, value) in [
                            (
                                "Dimensiones",
                                format!("{} × {} px", preview.info.width, preview.info.height),
                            ),
                            ("Formato", preview.info.kind.to_string()),
                            ("Tamaño", file_size(preview.info.stamp.length)),
                        ] {
                            ui.label(RichText::new(label).small().color(MUTED));
                            ui.label(value);
                            ui.end_row();
                        }
                    });
                ui.add_space(12.0);
                if self.detail.ready() {
                    ui.label(RichText::new("Resolución completa").small().color(ACCENT));
                } else if self.loading {
                    ui.label(RichText::new("Cargando vista previa…").small().color(MUTED));
                }
                if preview.info.has_color_profile {
                    ui.label(RichText::new("ICC convertido a sRGB").small().color(MUTED));
                }
            }
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            theme::eyebrow(ui, "ORGANIZACIÓN");
            if let Some(annotation) = self.catalog.annotation(path) {
                ui.label(if annotation.favorite {
                    "Favorita"
                } else {
                    "Sin favorito"
                });
                if annotation.tags.is_empty() {
                    ui.label(
                        RichText::new("Añade etiquetas para encontrarla después.")
                            .small()
                            .color(MUTED),
                    );
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for tag in &annotation.tags {
                            egui::Frame::NONE
                                .fill(theme::SELECTED)
                                .corner_radius(4)
                                .inner_margin(4)
                                .show(ui, |ui| {
                                    ui.label(RichText::new(tag).small().color(ACCENT));
                                });
                        }
                    });
                }
            }
            if ui
                .add_enabled(self.catalog.ready, egui::Button::new("Editar organización"))
                .clicked()
            {
                self.catalog.editor_open = true;
            }
        } else {
            ui.label(
                RichText::new("Selecciona una imagen para ver sus detalles y organizarla.")
                    .color(MUTED),
            );
        }
        if let Some(error) = &self.detail.error {
            ui.add_space(8.0);
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        ui.add_space(8.0);
        egui::CollapsingHeader::new("Ubicación").show(ui, |ui| {
            if let Some(path) = &self.path {
                ui.add(egui::Label::new(RichText::new(path.to_string_lossy()).small()).wrap());
                if ui.button("Copiar ruta").clicked() {
                    ui.ctx().copy_text(path.to_string_lossy().into_owned());
                }
            }
        });
        if !self.grid_view && !self.entries.is_empty() {
            egui::CollapsingHeader::new(format!("En esta carpeta · {}", self.entries.len())).show(
                ui,
                |ui| {
                    let mut chosen = None;
                    let row_height = ui.spacing().interact_size.y;
                    egui::ScrollArea::vertical()
                        .id_salt("folder-list")
                        .max_height(200.0)
                        .show_rows(ui, row_height, self.entries.len(), |ui, rows| {
                            for index in rows {
                                let entry = &self.entries[index];
                                if ui
                                    .add(
                                        egui::Button::selectable(
                                            self.path.as_ref() == Some(&entry.path),
                                            entry
                                                .path
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy(),
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
                },
            );
        }
    }

    fn viewer(&mut self, ui: &mut egui::Ui) {
        if self.path.is_none() && self.folder.is_none() && !self.folder_loading {
            self.welcome(ui);
            return;
        }
        let position = self
            .entries
            .iter()
            .position(|entry| Some(&entry.path) == self.path.as_ref());
        ui.horizontal_wrapped(|ui| {
            if self.folder.is_some() {
                if ui
                    .add_enabled(
                        position.is_some_and(|index| index > 0),
                        egui::Button::new("←"),
                    )
                    .on_hover_text("Imagen anterior · Flecha izquierda")
                    .clicked()
                {
                    self.step(-1);
                }
                if ui
                    .add_enabled(
                        position.is_some_and(|index| index + 1 < self.entries.len()),
                        egui::Button::new("→"),
                    )
                    .on_hover_text("Imagen siguiente · Flecha derecha")
                    .clicked()
                {
                    self.step(1);
                }
                if let Some(position) = position {
                    ui.label(
                        RichText::new(format!("{} / {}", position + 1, self.entries.len()))
                            .small()
                            .color(MUTED),
                    );
                }
                ui.separator();
            }
            ui.add_enabled_ui(self.preview.is_some(), |ui| {
                if ui
                    .selectable_label(self.zoom.is_none(), "Ajustar")
                    .on_hover_text("Home · Ajustar a la ventana")
                    .clicked()
                {
                    self.zoom = None;
                    self.pan = Vec2::ZERO;
                }
                if ui
                    .selectable_label(self.zoom == Some(1.0), "100%")
                    .on_hover_text("Un píxel de imagen por píxel de pantalla")
                    .clicked()
                {
                    self.zoom = Some(1.0);
                    self.pan = Vec2::ZERO;
                }
                if ui.button("−").on_hover_text("Reducir zoom").clicked() {
                    self.zoom =
                        Some((self.zoom.unwrap_or(self.current_scale) / 1.25).clamp(0.001, 16.0));
                }
                ui.label(
                    RichText::new(format!(
                        "{:.0}%",
                        self.zoom.unwrap_or(self.current_scale) * 100.0
                    ))
                    .color(MUTED),
                );
                if ui.button("+").on_hover_text("Ampliar zoom").clicked() {
                    self.zoom =
                        Some((self.zoom.unwrap_or(self.current_scale) * 1.25).clamp(0.001, 16.0));
                }
            });
            if ui
                .add_enabled(
                    self.path.is_some() || self.batch.busy,
                    theme::primary(if self.batch.busy {
                        "Ver lote"
                    } else {
                        "Exportar imagen"
                    }),
                )
                .clicked()
            {
                if self.batch.busy {
                    self.batch.open = true;
                } else {
                    self.batch.select(self.path.iter().cloned().collect());
                }
            }
        });
        if let Some(path) = &self.path {
            ui.add(
                egui::Label::new(
                    RichText::new(path.file_name().unwrap_or_default().to_string_lossy())
                        .small()
                        .color(MUTED),
                )
                .truncate(),
            )
            .on_hover_text(path.to_string_lossy());
        }
        ui.add_space(8.0);
        let (canvas, response) = ui.allocate_exact_size(ui.available_size(), Sense::drag());
        let painter = ui.painter().with_clip_rect(canvas);
        painter.rect_filled(canvas, 0.0, Color32::from_rgb(22, 23, 25));
        if let Some(preview) = &self.preview {
            let pixels_per_point = ui.ctx().pixels_per_point();
            let fit = (((canvas.width() - 32.0) / preview.source_size.x)
                .min((canvas.height() - 32.0) / preview.source_size.y)
                * pixels_per_point)
                .clamp(0.001, 1.0);
            if response.hovered() {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let old = self.zoom.unwrap_or(fit);
                    let new = (old * (scroll * 0.003).exp()).clamp(0.001, 16.0);
                    if let Some(pointer) = response.hover_pos() {
                        self.pan = anchored_pan(canvas.center(), self.pan, pointer, new / old);
                    }
                    self.zoom = Some(new);
                }
            }
            if response.dragged() {
                self.pan += response.drag_delta();
            }
            if response.double_clicked() {
                self.zoom = None;
                self.pan = Vec2::ZERO;
            }
            if self.zoom.is_none() && self.detail.active() {
                self.detail.clear();
            }
            let preview_ratio = preview.size.x / preview.source_size.x;
            let needs_detail = self.zoom.unwrap_or(fit) > preview_ratio && preview_ratio < 1.0;
            if needs_detail
                && !self.batch.busy
                && let Some(path) = &self.path
            {
                self.detail.request(path.clone(), &preview.info);
            }
            let size = preview.source_size * (self.zoom.unwrap_or(fit) / pixels_per_point);
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
            if needs_detail && self.detail.ready() {
                self.detail
                    .paint(ui.ctx(), &painter, image_rect, canvas, &preview.texture);
            } else {
                painter.image(
                    preview.texture.id(),
                    image_rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            if self.detail.busy() {
                painter.text(
                    canvas.min + egui::vec2(16.0, 16.0),
                    egui::Align2::LEFT_TOP,
                    "Cargando detalle…",
                    egui::FontId::proportional(14.0),
                    Color32::WHITE,
                );
            }
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
fn anchored_pan(center: egui::Pos2, pan: Vec2, pointer: egui::Pos2, ratio: f32) -> Vec2 {
    pointer - center - (pointer - (center + pan)) * ratio
}
fn file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MiB", bytes as f64 / 1_048_576.0)
    }
}
impl eframe::App for RasteraApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ui.set_style(ctx.global_style());
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, theme::BACKGROUND);
        self.receive(&ctx);
        self.shortcuts(&ctx);
        egui::Frame::NONE.inner_margin(8).show(ui, |ui| {
            self.header(ui);
            for error in [&self.error, &self.folder_error, &self.catalog.error]
                .into_iter()
                .flatten()
            {
                egui::Frame::NONE
                    .fill(Color32::from_rgb(57, 32, 36))
                    .corner_radius(6)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::from_rgb(255, 177, 170), error);
                    });
            }
            let height = (ui.available_height() - 38.0).max(100.0);
            let show_details =
                self.inspector_open && ui.available_width() >= 944.0 && self.path.is_some();
            let sidebar = if show_details { 216.0 } else { 0.0 };
            let width =
                (ui.available_width() - sidebar - if show_details { 8.0 } else { 0.0 }).max(100.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(width, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_height(height);
                        ui.set_min_width(width);
                        if self.grid_view && self.folder.is_some() {
                            self.library(ui);
                        } else {
                            self.viewer(ui);
                        }
                    },
                );
                if show_details {
                    egui::Frame::NONE
                        .fill(theme::SURFACE)
                        .corner_radius(0)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.set_width(200.0);
                            ui.set_min_height((height - 16.0).max(0.0));
                            egui::ScrollArea::vertical()
                                .id_salt("inspector-scroll")
                                .max_height((height - 16.0).max(0.0))
                                .show(ui, |ui| {
                                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                        self.inspector(ui)
                                    });
                                });
                        });
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let busy =
                    self.loading || self.folder_loading || self.grid.busy() || self.detail.busy();
                ui.label(
                    RichText::new(if self.dialog.is_some() {
                        "Esperando selección…"
                    } else if busy {
                        "Cargando…"
                    } else {
                        "Listo"
                    })
                    .small()
                    .color(MUTED),
                );
                if self.grid_view && self.folder.is_some() {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{} / {} imágenes",
                            self.catalog.filtered.len(),
                            self.entries.len()
                        ))
                        .small()
                        .color(MUTED),
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(if self.grid_view {
                            "Ctrl+clic: añadir · Shift+clic: rango · Doble clic: abrir"
                        } else if self.path.is_some() {
                            "Rueda: zoom · Arrastrar: desplazar · Home: ajustar"
                        } else {
                            "Ctrl+O: imagen · Ctrl+Shift+O: carpeta"
                        })
                        .small()
                        .color(MUTED),
                    );
                });
            });
        });
        self.auxiliary_windows(&ctx);
        #[cfg(feature = "capture")]
        if self.catalog.ready
            && std::env::var_os("RASTERA_CAPTURE_PANEL").as_deref()
                == Some(std::ffi::OsStr::new("organization"))
        {
            self.catalog.editor_open = true;
        }
        self.catalog.editor(
            &ctx,
            self.path.as_deref(),
            self.preview.as_ref().map(|preview| &preview.info),
        );
        self.batch.show(&ctx);
        if self.diagnostics_open {
            let (tiles, full) = self.detail.diagnostics();
            let (textures, thumbs, cache) = self.grid.diagnostics();
            let preview = self.loader.stats();
            egui::Window::new("Rendimiento").default_width(620.0).open(&mut self.diagnostics_open).show(&ctx, |ui| {
                ui.label(format!("Reservas CPU: {:.1} MiB · máximo {:.1} · límite {:.0}", self.budget.used() as f64/1048576.0, self.budget.peak() as f64/1048576.0, self.budget.limit() as f64/1048576.0));
                ui.label("Las reservas son estimaciones de buffers, no la RAM total del proceso.");
                let preview_bytes = self.preview.as_ref().map_or(0.0, |p| p.size.x * p.size.y * 4.0);
                ui.label(format!("Píxeles GPU: vista {:.1} MiB · detalle {:.1} MiB · {} miniaturas", preview_bytes/1048576.0, tiles as f64/1048576.0, textures));
                ui.label("Estos tamaños excluyen sobrecostes y copias del controlador gráfico.");
                for (name, stats) in [("Vista previa",preview), ("Detalle",full), ("Miniaturas",thumbs)] {
                    ui.label(format!("{name}: {} completadas · {} fallidas · {} canceladas · última {:.2} ms",
                        stats.completed, stats.failed, stats.cancelled, stats.last_elapsed.as_secs_f64()*1000.0));
                }
                ui.label(format!("Caché en disco: {} aciertos · {} decodificaciones solicitadas",cache.hits,cache.misses));
                ui.label("Los tiempos incluyen inspección, espera de memoria y procesamiento, hasta encolar el resultado. Los contadores abarcan esta sesión.");
            });
        }
        #[cfg(feature = "capture")]
        self.capture(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_keeps_the_source_point_under_the_cursor() {
        let center = egui::pos2(400.0, 300.0);
        let pan = egui::vec2(30.0, -20.0);
        let cursor = egui::pos2(250.0, 140.0);
        let source_point = cursor - (center + pan);
        let updated = anchored_pan(center, pan, cursor, 2.5);
        assert_eq!(center + updated + source_point * 2.5, cursor);
    }
}
