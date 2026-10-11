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
    route_focus_requested: bool,
    shortcuts_open: bool,
    diagnostics_open: bool,
    detail: crate::detail::DetailView,
    batch: crate::batch::Batch,
    budget: MemoryBudget,
    catalog: crate::catalog::Catalog,
    metadata_dirty: bool,
    grid: crate::grid::ThumbnailGrid,
    grid_view: bool,
    viewer_filtered: bool,
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
    #[cfg(feature = "capture")]
    navigation: Option<crate::navigation::NavigationBenchmark>,
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
            route_focus_requested: false,
            shortcuts_open: false,
            diagnostics_open: false,
            detail: crate::detail::DetailView::new(&cc.egui_ctx, budget.clone()),
            batch: crate::batch::Batch::new(&cc.egui_ctx, budget.clone()),
            budget,
            catalog: crate::catalog::Catalog::new(&cc.egui_ctx),
            metadata_dirty: false,
            grid,
            grid_view: false,
            viewer_filtered: false,
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
            #[cfg(feature = "capture")]
            navigation: crate::navigation::NavigationBenchmark::from_env(),
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
        self.viewer_filtered = false;
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
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.shortcuts_open = true;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F4)) {
            self.open_route();
        }
        let panel_open = self.batch.open
            || self.catalog.editor_open
            || self.route_open
            || self.details_popup
            || self.shortcuts_open
            || self.diagnostics_open;
        if panel_open
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.batch.open = false;
            self.catalog.editor_open = false;
            self.route_open = false;
            self.details_popup = false;
            self.shortcuts_open = false;
            self.diagnostics_open = false;
        }
        if !panel_open
            && self.folder.is_some()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F))
        {
            self.grid_view = true;
            self.catalog.focus_search(ctx);
        }
        if !panel_open
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F5))
            && let Some(folder) = self.folder.clone()
        {
            self.open(folder);
        }
        if !panel_open
            && (self.path.is_some() || !self.grid.selected.is_empty())
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::E))
        {
            self.export_selection();
        }
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
        if !ctx.egui_wants_keyboard_input() && !panel_open && !self.batch.open {
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
            if !self.grid_view && ctx.input(|input| input.key_pressed(egui::Key::Home)) {
                self.zoom = None;
                self.pan = Vec2::ZERO;
            }
            if !self.grid_view
                && ctx.input(|input| {
                    input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)
                })
            {
                self.zoom =
                    Some((self.zoom.unwrap_or(self.current_scale) * 1.25).clamp(0.05, 16.0));
            }
            if !self.grid_view && ctx.input(|input| input.key_pressed(egui::Key::Minus)) {
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
            .viewer_entries()
            .iter()
            .position(|entry| Some(&entry.path) == self.path.as_ref());
        if let Some(current) = current {
            let next = current.checked_add_signed(delta);
            if let Some(path) = next
                .and_then(|index| self.viewer_entries().get(index))
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
        let paths = if !self.grid_view || self.grid.selected.is_empty() {
            self.path.iter().cloned().collect()
        } else {
            self.grid.selected.iter().cloned().collect()
        };
        self.batch.select(paths);
    }

    fn open_route(&mut self) {
        self.route_open = true;
        self.route_focus_requested = true;
        if let Some(path) = self.folder.as_ref().or(self.path.as_ref()) {
            self.path_input = theme::display_path(path);
        }
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
                if self.route_focus_requested {
                    response.request_focus();
                    self.route_focus_requested = false;
                }
                accepted = ui
                    .add_enabled(!self.path_input.trim().is_empty(), theme::primary("Abrir"))
                    .clicked()
                    || (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        && !self.path_input.trim().is_empty());
            });
        if accepted {
            self.open(PathBuf::from(self.path_input.trim().trim_matches('"')));
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
                            ("Ctrl+F", "Buscar en la carpeta"),
                            ("Ctrl+E", "Exportar la selección o imagen"),
                            ("F4 / F5", "Abrir una ruta / actualizar carpeta"),
                            ("Ctrl+clic", "Añadir o quitar de la selección"),
                            ("Shift+clic", "Seleccionar un rango"),
                            (
                                "Flechas / Home / End",
                                "Navegar por miniaturas / primera / última",
                            ),
                            ("RePág / AvPág", "Mover una página en Biblioteca"),
                            ("Shift+flechas", "Seleccionar un rango en Biblioteca"),
                            ("Ctrl+flechas / Espacio", "Mover foco / cambiar selección"),
                            ("Enter", "Abrir la miniatura en el visor"),
                            ("Esc", "Cerrar panel / deseleccionar / volver"),
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
            ui.add_space((ui.available_height() * 0.28).max(20.0));
            ui.heading("Tu biblioteca de imágenes");
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Abre una carpeta para explorar, organizar y exportar tus fotografías.",
                )
                .color(MUTED),
            );
            ui.add_space(16.0);
            if ui.add(theme::primary("Abrir carpeta…")).clicked() {
                self.pick_path(ui.ctx(), true);
            }
            if ui.button("Abrir una imagen…").clicked() {
                self.pick_file(ui.ctx());
            }
            ui.add_space(16.0);
            ui.label(
                RichText::new("También puedes arrastrar una imagen o carpeta a esta ventana.")
                    .small()
                    .color(MUTED),
            );
            ui.label(RichText::new("JPEG · PNG · WebP").small().color(MUTED));
        });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("Rastera").strong().color(MUTED));
            ui.add_space(8.0);
            ui.menu_button("Archivo", |ui| {
                if ui
                    .add(egui::Button::new("Abrir carpeta…").shortcut_text("Ctrl+Shift+O"))
                    .clicked()
                {
                    self.pick_path(ui.ctx(), true);
                    ui.close();
                }
                if ui
                    .add(egui::Button::new("Abrir imagen…").shortcut_text("Ctrl+O"))
                    .clicked()
                {
                    self.pick_file(ui.ctx());
                    ui.close();
                }
                if ui
                    .add(egui::Button::new("Abrir una ruta…").shortcut_text("F4"))
                    .clicked()
                {
                    self.open_route();
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.folder.is_some(),
                        egui::Button::new("Actualizar carpeta").shortcut_text("F5"),
                    )
                    .clicked()
                {
                    if let Some(folder) = self.folder.clone() {
                        self.open(folder);
                    }
                    ui.close();
                }
            });
            ui.menu_button("Ver", |ui| {
                if self.folder.is_some() {
                    if ui.selectable_label(self.grid_view, "Biblioteca").clicked() {
                        self.grid_view = true;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.path.is_some(),
                            egui::Button::selectable(!self.grid_view, "Visor"),
                        )
                        .clicked()
                    {
                        self.grid_view = false;
                        ui.close();
                    }
                    ui.separator();
                } else if ui.button("Ver carpeta").clicked()
                    && let Some(parent) = self.path.as_ref().and_then(|path| path.parent())
                {
                    self.open(parent.to_path_buf());
                    ui.close();
                }
                if ui.ctx().viewport_rect().width() >= 980.0 {
                    ui.checkbox(&mut self.inspector_open, "Panel de propiedades");
                } else if ui.button("Propiedades de imagen…").clicked() {
                    self.details_popup = true;
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
            ui.menu_button("Ayuda", |ui| {
                if ui
                    .add(egui::Button::new("Atajos de teclado…").shortcut_text("F1"))
                    .clicked()
                {
                    self.shortcuts_open = true;
                    ui.close();
                }
                ui.separator();
                ui.label(format!("Rastera {}", env!("CARGO_PKG_VERSION")));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.dialog.is_none(),
                        egui::Button::new("Abrir imagen…").frame(false),
                    )
                    .on_hover_text("Ctrl+O")
                    .clicked()
                {
                    self.pick_file(ui.ctx());
                }
                if ui
                    .add_enabled(
                        self.dialog.is_none(),
                        egui::Button::new("Abrir carpeta…").frame(false),
                    )
                    .on_hover_text("Ctrl+Shift+O")
                    .clicked()
                {
                    self.pick_path(ui.ctx(), true);
                }
            });
        });
        if self.path.is_some() || self.folder.is_some() {
            ui.separator();
            ui.horizontal(|ui| {
                if self.folder.is_some() {
                    if theme::tab(ui, self.grid_view, "Biblioteca").clicked() {
                        self.grid_view = true;
                    }
                    ui.add_enabled_ui(self.path.is_some(), |ui| {
                        if theme::tab(ui, !self.grid_view, "Visor").clicked() {
                            self.grid_view = false;
                        }
                    });
                    ui.separator();
                }
                if self.folder.is_none()
                    && ui.button("Ver carpeta").clicked()
                    && let Some(parent) = self.path.as_ref().and_then(|path| path.parent())
                {
                    self.open(parent.to_path_buf());
                }
                let location = self.folder.clone().or_else(|| {
                    self.path
                        .as_ref()
                        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
                });
                let parent = location
                    .as_ref()
                    .and_then(|path| path.parent())
                    .map(std::path::Path::to_path_buf);
                if ui
                    .add_enabled(parent.is_some(), egui::Button::new("↑").frame(false))
                    .on_hover_text("Subir a la carpeta superior")
                    .clicked()
                    && let Some(parent) = parent
                {
                    self.open(parent);
                }
                if ui
                    .add_enabled(self.folder.is_some(), egui::Button::new("↻").frame(false))
                    .on_hover_text("Actualizar carpeta · F5")
                    .clicked()
                    && let Some(folder) = self.folder.clone()
                {
                    self.open(folder);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let compact = ui.ctx().viewport_rect().width() < 980.0;
                    if theme::tab(ui, self.inspector_open && !compact, "Propiedades").clicked() {
                        if compact {
                            self.details_popup = true;
                        } else {
                            self.inspector_open = !self.inspector_open;
                        }
                    }
                    if let Some(location) = location {
                        let name = location.file_name().map_or_else(
                            || location.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        );
                        let parent = location
                            .parent()
                            .and_then(|parent| parent.file_name())
                            .map(|name| name.to_string_lossy().into_owned());
                        let label =
                            parent.map_or(name.clone(), |parent| format!("{parent}  /  {name}"));
                        let width = ui.available_width().max(60.0);
                        if ui
                            .add_sized(
                                [width, 28.0],
                                egui::Button::new(label).frame(false).truncate(),
                            )
                            .on_hover_text(format!(
                                "{}\nHaz clic para abrir otra ruta",
                                location.display()
                            ))
                            .clicked()
                        {
                            self.open_route();
                        }
                    }
                });
            });
        }
    }

    fn library(&mut self, ui: &mut egui::Ui) {
        self.catalog.filters(ui, &self.entries);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.menu_button("Selección", |ui| {
                if ui
                    .add_enabled(
                        !self.catalog.filtered.is_empty(),
                        egui::Button::new("Seleccionar todo").shortcut_text("Ctrl+A"),
                    )
                    .clicked()
                {
                    self.grid.select_all(&self.catalog.filtered);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.grid.selected.is_empty(),
                        egui::Button::new("Deseleccionar").shortcut_text("Esc"),
                    )
                    .clicked()
                {
                    self.grid.clear_selection();
                    ui.close();
                }
            });
            self.catalog.sort_control(ui);
            egui::ComboBox::from_id_salt("tile-density")
                .width(100.0)
                .selected_text(match self.grid.density {
                    0 => "Pequeñas",
                    2 => "Grandes",
                    _ => "Medianas",
                })
                .show_ui(ui, |ui| {
                    for (value, label) in [(0, "Pequeñas"), (1, "Medianas"), (2, "Grandes")] {
                        ui.selectable_value(&mut self.grid.density, value, label);
                    }
                })
                .response
                .on_hover_text("Tamaño de las miniaturas");
            let selected = self.grid.selected.len();
            if selected > 0 || self.batch.busy {
                ui.separator();
                let hidden = selected
                    - self
                        .catalog
                        .filtered
                        .iter()
                        .filter(|entry| self.grid.selected.contains(&entry.path))
                        .count();
                ui.label(RichText::new(format!("{selected} seleccionadas")).color(ACCENT));
                if hidden > 0 {
                    ui.label(
                        RichText::new(format!("{hidden} fuera del filtro"))
                            .small()
                            .color(MUTED),
                    )
                    .on_hover_text(
                        "La exportación también incluye las seleccionadas fuera del filtro.",
                    );
                }
                if ui
                    .add(theme::primary(if self.batch.busy {
                        "Ver lote".to_owned()
                    } else {
                        format!("Exportar ({selected})…")
                    }))
                    .clicked()
                {
                    if self.batch.busy {
                        self.batch.open = true;
                    } else {
                        self.export_selection();
                    }
                }
            }
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        if self.entries.is_empty() || self.catalog.filtered.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.25).max(20.0));
                ui.heading(if self.folder_loading {
                    "Leyendo la carpeta…"
                } else if self.entries.is_empty() {
                    "No hay imágenes en esta carpeta"
                } else {
                    "No hay coincidencias"
                });
                ui.label(
                    RichText::new(if self.entries.is_empty() {
                        "Rastera muestra archivos JPEG, PNG y WebP de esta carpeta."
                    } else {
                        "Prueba otro nombre, etiqueta o formato."
                    })
                    .color(MUTED),
                );
                ui.add_space(12.0);
                if !self.entries.is_empty() && ui.button("Restablecer filtros").clicked() {
                    self.catalog.clear_filters();
                }
                if self.entries.is_empty()
                    && !self.folder_loading
                    && ui.button("Abrir otra carpeta…").clicked()
                {
                    self.pick_path(ui.ctx(), true);
                }
            });
        } else if let Some(path) = self.grid.show(
            ui,
            &self.catalog.filtered,
            self.loading,
            self.path.as_deref(),
            &self.catalog,
            !(self.batch.open
                || self.catalog.editor_open
                || self.route_open
                || self.details_popup
                || self.shortcuts_open
                || self.diagnostics_open
                || self.dialog.is_some()),
        ) {
            self.viewer_filtered = true;
            if self.path.as_ref() != Some(&path) {
                self.open_image(path);
            }
            if self.grid.open_viewer {
                self.grid_view = false;
            }
            if self.grid.organize {
                self.catalog.open_editor();
            }
            if self.grid.export {
                self.export_selection();
            }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let busy =
                self.loading || self.folder_loading || self.grid.busy() || self.detail.busy();
            if busy {
                ui.add(egui::Spinner::new().size(12.0));
            }
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
            if self.folder.is_some() {
                ui.separator();
                let count = if self.catalog.filtered.len() == self.entries.len() {
                    format!("{} imágenes", self.entries.len())
                } else {
                    format!(
                        "{} de {} imágenes",
                        self.catalog.filtered.len(),
                        self.entries.len()
                    )
                };
                ui.label(RichText::new(count).small().color(MUTED));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.batch.busy && ui.small_button("Exportación en curso · Ver lote").clicked()
                {
                    self.batch.open = true;
                } else if ui.ctx().viewport_rect().width() >= 980.0 {
                    ui.label(
                        RichText::new(if self.grid_view {
                            "Ctrl+F  Buscar    ·    Doble clic  Abrir"
                        } else {
                            "Rueda  Zoom    ·    Arrastrar  Desplazar    ·    Home  Ajustar"
                        })
                        .small()
                        .color(MUTED),
                    );
                }
            });
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Propiedades").strong());
        ui.add_space(8.0);
        let Some(path) = self.path.clone() else {
            ui.label(RichText::new("Selecciona una imagen para ver sus propiedades.").color(MUTED));
            return;
        };
        ui.add(
            egui::Label::new(
                RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong(),
            )
            .wrap(),
        );
        if let Some(preview) = &self.preview {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                let size = preview.size
                    * (ui.available_width() / preview.size.x).min(130.0 / preview.size.y);
                ui.add(egui::Image::new((preview.texture.id(), size)));
            });
            ui.add_space(8.0);
            theme::eyebrow(ui, "IMAGEN");
            egui::Grid::new("image-properties")
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        (
                            "Dimensiones",
                            format!(
                                "{} × {} px",
                                preview.source_size.x as u32, preview.source_size.y as u32
                            ),
                        ),
                        ("Formato", preview.info.kind.to_string()),
                        ("Archivo", file_size(preview.info.stamp.length)),
                    ] {
                        ui.label(RichText::new(label).small().color(MUTED));
                        ui.label(value);
                        ui.end_row();
                    }
                });
            if preview.info.has_color_profile {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Color · ICC convertido a sRGB")
                        .small()
                        .color(MUTED),
                );
            }
            if self.detail.ready() {
                ui.label(
                    RichText::new("Detalle a resolución completa")
                        .small()
                        .color(ACCENT),
                );
            }
        } else if self.loading {
            ui.label(RichText::new("Cargando propiedades…").small().color(MUTED));
        }
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        theme::eyebrow(ui, "ORGANIZACIÓN");
        let annotation = self.catalog.annotation(&path).cloned().unwrap_or_default();
        let mut favorite = annotation.favorite;
        if ui
            .add_enabled(
                self.catalog.ready,
                egui::Checkbox::new(&mut favorite, "Favorita"),
            )
            .changed()
        {
            self.catalog.toggle_favorite(
                &path,
                self.preview.as_ref().map(|preview| preview.info.clone()),
            );
        }
        if annotation.tags.is_empty() {
            ui.label(RichText::new("Sin etiquetas").small().color(MUTED));
        } else {
            ui.label(RichText::new(annotation.tags.join(" · ")).color(MUTED));
        }
        if ui
            .add_enabled(
                self.catalog.ready,
                egui::Button::new("Editar organización…"),
            )
            .clicked()
        {
            self.catalog.open_editor();
        }
        ui.add_space(8.0);
        egui::CollapsingHeader::new("Ubicación del archivo").show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(theme::display_path(&path))
                        .small()
                        .color(MUTED),
                )
                .wrap(),
            );
            if ui.button("Copiar ruta").clicked() {
                ui.ctx().copy_text(theme::display_path(&path));
            }
        });
        if let Some(error) = &self.detail.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        if !self.grid_view && !self.viewer_entries().is_empty() {
            let count = self.viewer_entries().len();
            egui::CollapsingHeader::new(format!("En esta vista · {count}")).show(ui, |ui| {
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt("folder-list")
                    .max_height(180.0)
                    .show_rows(ui, 28.0, count, |ui, rows| {
                        for index in rows {
                            let entry = &self.viewer_entries()[index];
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
                                .clicked()
                            {
                                chosen = Some(entry.path.clone());
                            }
                        }
                    });
                if let Some(path) = chosen {
                    self.open_image(path);
                }
            });
        }
    }

    fn viewer_entries(&self) -> &[FolderEntry] {
        if self.viewer_filtered {
            &self.catalog.filtered
        } else {
            &self.entries
        }
    }

    fn viewer(&mut self, ui: &mut egui::Ui) {
        if self.path.is_none() && self.folder.is_none() && !self.folder_loading {
            self.welcome(ui);
            return;
        }
        let position = self
            .viewer_entries()
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
                        position.is_some_and(|index| index + 1 < self.viewer_entries().len()),
                        egui::Button::new("→"),
                    )
                    .on_hover_text("Imagen siguiente · Flecha derecha")
                    .clicked()
                {
                    self.step(1);
                }
                if let Some(position) = position {
                    ui.label(
                        RichText::new(format!(
                            "{} / {}",
                            position + 1,
                            self.viewer_entries().len()
                        ))
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
        painter.rect_filled(canvas, 0.0, theme::CANVAS);
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
        #[cfg(feature = "capture")]
        let ui_started = std::time::Instant::now();
        let ctx = ui.ctx().clone();
        ui.set_style(ctx.global_style());
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, theme::BACKGROUND);
        self.receive(&ctx);
        #[cfg(feature = "capture")]
        if let Some(benchmark) = &mut self.navigation {
            let action = benchmark.begin(
                self.entries.len(),
                self.folder_loading,
                self.grid.diagnostics().2,
            );
            if let Some(action) = action {
                match action {
                    crate::navigation::Action::Grid(index) => {
                        self.grid_view = true;
                        self.grid.benchmark_index = Some(index);
                    }
                    crate::navigation::Action::Viewer(index) => {
                        self.grid_view = false;
                        self.open_image(self.entries[index].path.clone());
                    }
                }
            }
        }
        self.shortcuts(&ctx);
        egui::Panel::top("workspace-header")
            .frame(theme::panel_frame())
            .show(ui, |ui| self.header(ui));
        egui::Panel::bottom("workspace-status")
            .frame(
                egui::Frame::NONE
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(12, 5)),
            )
            .show(ui, |ui| self.status_bar(ui));
        if self.error.is_some() || self.folder_error.is_some() || self.catalog.error.is_some() {
            egui::Panel::top("workspace-errors")
                .frame(
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(57, 32, 36))
                        .inner_margin(10),
                )
                .show(ui, |ui| {
                    for error in [&self.error, &self.folder_error, &self.catalog.error]
                        .into_iter()
                        .flatten()
                    {
                        ui.colored_label(Color32::from_rgb(255, 177, 170), error);
                    }
                });
        }
        let show_details =
            self.inspector_open && ctx.viewport_rect().width() >= 980.0 && self.path.is_some();
        if show_details {
            egui::Panel::right("workspace-inspector")
                .default_size(248.0)
                .size_range(220.0..=360.0)
                .resizable(true)
                .frame(theme::panel_frame())
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("inspector-scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.inspector(ui));
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BACKGROUND).inner_margin(12))
            .show(ui, |ui| {
                if self.grid_view && self.folder.is_some() {
                    self.library(ui);
                } else {
                    self.viewer(ui);
                }
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
                ui.label(format!("Mantenimiento de caché: {} recorridos · {:.2} ms acumulados · {} entradas examinadas · {} eliminadas",cache.maintenance_runs,cache.maintenance_elapsed.as_secs_f64()*1000.0,cache.maintenance_files,cache.evicted_files));
                ui.label("Los tiempos incluyen inspección, espera de memoria y procesamiento, hasta encolar el resultado. Los contadores abarcan esta sesión.");
            });
        }
        #[cfg(feature = "capture")]
        self.capture(&ctx);
        #[cfg(feature = "capture")]
        if let Some(benchmark) = &mut self.navigation {
            let (textures, _, cache) = self.grid.diagnostics();
            let sample = crate::navigation::Observation {
                grid: if self.grid_view {
                    self.grid.measurement
                } else {
                    Default::default()
                },
                viewer_ready: !self.loading && self.preview.is_some(),
                viewer_error: !self.loading && self.error.is_some(),
                reserved: self.budget.used(),
                peak: self.budget.peak(),
                textures,
                cache,
                ui_elapsed: ui_started.elapsed(),
            };
            match benchmark.observe(sample) {
                Ok(true) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Ok(false) => ctx.request_repaint_after(std::time::Duration::from_millis(10)),
                Err(error) => {
                    eprintln!("Medición de navegación falló: {error}");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
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
