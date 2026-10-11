use crossbeam_channel::{Receiver, bounded};
use eframe::egui;
use rastera_adapters::LocalBatch;
use rastera_core::{ErrorKind, ExportFormat, ExportPlan, ExportRecipe, RecoveryReport};
use rastera_engine::{BatchUpdate, BatchWorker, memory::MemoryBudget};
use std::{path::PathBuf, sync::Arc};

pub struct Batch {
    worker: BatchWorker,
    pub open: bool,
    pub busy: bool,
    generation: u64,
    paths: Vec<PathBuf>,
    destination: String,
    destination_path: Option<PathBuf>,
    dialog: Option<Receiver<Option<PathBuf>>>,
    recovery_dialog: bool,
    recovery: Option<RecoveryReport>,
    recipe: ExportRecipe,
    resize: bool,
    edge: u32,
    acknowledged: bool,
    plan: Option<ExportPlan>,
    results: Vec<Option<std::result::Result<(), String>>>,
    completed: usize,
    cancelled: bool,
    pub error: Option<String>,
    journal: Option<PathBuf>,
}

impl Batch {
    pub fn new(ctx: &egui::Context, budget: MemoryBudget) -> Self {
        let ctx = ctx.clone();
        Self {
            worker: BatchWorker::new(
                Arc::new(LocalBatch),
                budget,
                Arc::new(move || ctx.request_repaint()),
            ),
            open: false,
            busy: false,
            generation: 0,
            paths: Vec::new(),
            destination: String::new(),
            destination_path: None,
            dialog: None,
            recovery_dialog: false,
            recovery: None,
            recipe: ExportRecipe::default(),
            resize: true,
            edge: 1920,
            acknowledged: false,
            plan: None,
            results: Vec::new(),
            completed: 0,
            cancelled: false,
            error: None,
            journal: None,
        }
    }
    pub fn select(&mut self, mut paths: Vec<PathBuf>) {
        self.open = true;
        if self.busy {
            return;
        }
        paths.sort();
        paths.dedup();
        if paths == self.paths && self.plan.is_some() && !self.results.is_empty() {
            return;
        }
        self.paths = paths;
        self.recovery = None;
        self.plan = None;
        self.results.clear();
        self.completed = 0;
        self.error = None;
        self.journal = None;
        self.acknowledged = false;
    }
    fn prepare(&mut self) {
        self.journal = None;
        self.cancelled = false;
        self.generation += 1;
        self.recipe.max_edge = self.resize.then_some(self.edge);
        let destination = self
            .destination_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(self.destination.trim()));
        match self.worker.plan(
            self.generation,
            self.paths.clone(),
            destination,
            self.recipe.clone(),
        ) {
            Ok(()) => {
                self.busy = true;
                self.error = None;
                self.plan = None;
                self.results.clear();
                self.completed = 0;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    pub fn receive(&mut self) {
        if let Some(dialog) = &self.dialog {
            match dialog.try_recv() {
                Ok(path) => {
                    self.dialog = None;
                    if let Some(path) = path {
                        if self.recovery_dialog {
                            self.recover(path);
                            return;
                        }
                        self.destination = path.to_string_lossy().into_owned();
                        self.destination_path = Some(path);
                        self.plan = None;
                    }
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => self.dialog = None,
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }
        while let Some(event) = self.worker.try_recv() {
            if event.generation != self.generation {
                continue;
            }
            match event.update {
                BatchUpdate::Recovered(result) => {
                    self.busy = false;
                    match result {
                        Ok(report) => {
                            self.recipe = report.pending.recipe.clone();
                            self.resize = self.recipe.max_edge.is_some();
                            self.edge = self.recipe.max_edge.unwrap_or(1920);
                            self.paths = report
                                .pending
                                .items
                                .iter()
                                .map(|item| item.source.clone())
                                .collect();
                            if let Some(parent) = report.journal.parent() {
                                self.destination = parent.to_string_lossy().into_owned();
                                self.destination_path = Some(parent.to_path_buf());
                            }
                            self.plan =
                                (!report.pending.items.is_empty()).then(|| report.pending.clone());
                            self.journal = Some(report.journal.clone());
                            self.recovery = Some(report);
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                BatchUpdate::Planned(result) => {
                    self.busy = false;
                    match result {
                        Ok(plan) => {
                            self.plan = Some(plan);
                            self.error = None;
                            #[cfg(feature = "capture")]
                            if std::env::var("RASTERA_CAPTURE_TUTORIAL").as_deref() == Ok("results")
                                && std::env::var_os("RASTERA_CAPTURE_TO").is_some()
                            {
                                self.acknowledged = true;
                                self.execute();
                            }
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                BatchUpdate::Started(path) => self.journal = Some(path),
                BatchUpdate::Item { index, result } => {
                    let result = if let Err(error) = &result
                        && error.kind == ErrorKind::Published
                    {
                        self.error = Some(error.to_string());
                        Ok(())
                    } else {
                        result
                    };
                    if let Some(slot) = self.results.get_mut(index) {
                        if let Err(error) = &result
                            && error.kind == ErrorKind::Cancelled
                        {
                            self.cancelled = true;
                        }
                        *slot = Some(result.map_err(|error| error.to_string()));
                        self.completed += 1;
                    }
                }
                BatchUpdate::Finished { cancelled, result } => {
                    self.busy = false;
                    self.cancelled = cancelled;
                    if let Err(error) = result {
                        self.error = Some(error.to_string());
                    }
                }
            }
        }
    }
    fn execute(&mut self) {
        if self.busy || !self.acknowledged || !self.results.is_empty() || self.dialog.is_some() {
            return;
        }
        let Some(plan) = self.plan.clone() else {
            return;
        };
        self.results = vec![None; plan.items.len()];
        self.completed = 0;
        self.cancelled = false;
        match self.worker.run(self.generation, plan) {
            Ok(()) => self.busy = true,
            Err(error) => {
                self.error = Some(error.to_string());
                self.results.clear();
            }
        }
    }
    fn pick_destination(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        let (tx, rx) = bounded(1);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let selected = rfd::FileDialog::new()
                .set_title("Carpeta de exportación")
                .pick_folder();
            let _ = tx.send(selected);
            ctx.request_repaint();
        });
        self.dialog = Some(rx);
        self.recovery_dialog = false;
    }
    fn recover(&mut self, path: PathBuf) {
        if self.busy {
            return;
        }
        self.generation += 1;
        self.plan = None;
        self.recovery = None;
        self.results.clear();
        self.completed = 0;
        self.cancelled = false;
        self.acknowledged = false;
        self.error = None;
        self.journal = None;
        match self.worker.recover(self.generation, path) {
            Ok(()) => self.busy = true,
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn pick_journal(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() || self.busy {
            return;
        }
        let (tx, rx) = bounded(1);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let selected = rfd::FileDialog::new()
                .set_title("Recuperar diario de exportación")
                .add_filter("Diario SQLite", &["sqlite"])
                .pick_file();
            let _ = tx.send(selected);
            ctx.request_repaint();
        });
        self.dialog = Some(rx);
        self.recovery_dialog = true;
    }

    #[cfg(feature = "capture")]
    pub fn capture_plan(&mut self, paths: Vec<PathBuf>, destination: PathBuf) {
        self.select(paths);
        self.destination = destination.to_string_lossy().into_owned();
        self.destination_path = Some(destination);
        if std::env::var_os("RASTERA_CAPTURE_TUTORIAL").is_some() {
            self.recipe.format = ExportFormat::Jpeg;
            self.recipe.jpeg_quality = 90;
            self.edge = 1000;
            self.acknowledged = true;
        }
        if std::env::var("RASTERA_CAPTURE_TUTORIAL").as_deref() != Ok("configure") {
            self.prepare();
        }
    }
    #[cfg(feature = "capture")]
    pub fn capture_recovery(&mut self, journal: PathBuf) {
        self.open = true;
        self.recover(journal);
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        use crate::theme;
        let mut open = self.open;
        let mut close_requested = false;
        egui::Window::new("Exportar copias").open(&mut open).collapsible(false)
            .default_width(640.0).default_height((ctx.viewport_rect().height() - 100.0).clamp(320.0, 600.0)).max_height((ctx.viewport_rect().height() - 80.0).max(240.0))
            .default_pos(egui::pos2((ctx.viewport_rect().width() - 640.0).max(0.0) * 0.5, 42.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().id_salt("export-body")
                    .max_height((ctx.viewport_rect().height() - 210.0).clamp(200.0, 470.0))
                    .auto_shrink([false, true]).show(ui, |ui| {
                let step = if !self.results.is_empty() { 3 } else if self.plan.is_some() { 2 } else { 1 };
                ui.horizontal_wrapped(|ui| {
                    for (number, label) in [(1, "Configurar"), (2, "Revisar"), (3, "Resultados")] {
                        ui.label(egui::RichText::new(format!("{number}  {label}")).strong()
                            .color(if step == number { theme::ACCENT } else { theme::MUTED }));
                        if number != 3 { ui.label(egui::RichText::new(" / ").color(theme::BORDER)); }
                    }
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{} {} de origen", self.paths.len(), if self.paths.len() == 1 { "imagen" } else { "imágenes" }));
                    if ui.add_enabled(!self.busy && self.dialog.is_none(), egui::Button::new("Recuperar un lote…")).clicked() {
                        self.pick_journal(ctx);
                    }
                });
                if self.paths.is_empty() && self.recovery.is_none() {
                    ui.label(egui::RichText::new("Selecciona imágenes en Biblioteca o recupera un lote anterior.").color(theme::MUTED));
                }
                ui.add_space(8.0);
                let mut changed = false;
                ui.label(egui::RichText::new(format!("{} · {} · {}",
                    match self.recipe.format {
                        ExportFormat::Jpeg => format!("JPEG · Calidad {}", self.recipe.jpeg_quality),
                        ExportFormat::Png => "PNG".to_owned(),
                        ExportFormat::WebP => "WebP sin pérdidas".to_owned(),
                    },
                    if self.resize { format!("Lado mayor {} px", self.edge) } else { "Tamaño original".to_owned() },
                    if self.recipe.rotation == 0 { "Sin rotación".to_owned() } else { format!("Rotación {}°", self.recipe.rotation) }
                )).small().color(theme::MUTED));
                ui.add_enabled_ui(!self.busy && self.dialog.is_none() && self.results.is_empty(), |ui| {
                    egui::CollapsingHeader::new("Opciones de exportación")
                        .id_salt(("export-settings", step)).default_open(step == 1).show(ui, |ui| {
                    theme::eyebrow(ui, "DESTINO DE LAS COPIAS");
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 155.0).max(100.0);
                        if ui.add(egui::TextEdit::singleline(&mut self.destination)
                            .hint_text("Elige otra carpeta para las copias").desired_width(width)).changed() {
                            self.destination_path = None; changed = true;
                        }
                        if ui.button("Elegir carpeta…").clicked() { self.pick_destination(ctx); }
                    });
                    ui.add_space(12.0);
                    theme::eyebrow(ui, "FORMATO Y TAMAÑO");
                    ui.horizontal_wrapped(|ui| {
                        for (format, label) in [(ExportFormat::Png, "PNG"), (ExportFormat::Jpeg, "JPEG"), (ExportFormat::WebP, "WebP sin pérdidas")] {
                            changed |= ui.selectable_value(&mut self.recipe.format, format, label).changed();
                        }
                        egui::ComboBox::from_id_salt("batch-rotation")
                            .selected_text(if self.recipe.rotation == 0 { "Sin rotación".to_owned() } else { format!("Rotar {}°", self.recipe.rotation) })
                            .show_ui(ui, |ui| {
                                for rotation in [0, 90, 180, 270] {
                                    changed |= ui.selectable_value(&mut self.recipe.rotation, rotation,
                                        if rotation == 0 { "Sin rotación".to_owned() } else { format!("{rotation}° horario") }).changed();
                                }
                            });
                    });
                    ui.horizontal_wrapped(|ui| {
                        changed |= ui.checkbox(&mut self.resize, "Limitar lado mayor").changed();
                        changed |= ui.add_enabled(self.resize, egui::DragValue::new(&mut self.edge).range(1..=64000).suffix(" px")).changed();
                        ui.label(egui::RichText::new("Sin ampliar · Mantiene proporción").small().color(theme::MUTED));
                    });
                    if self.recipe.format == ExportFormat::Jpeg {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Calidad JPEG");
                            changed |= ui.add(egui::DragValue::new(&mut self.recipe.jpeg_quality).range(1..=100)).changed();
                            ui.label(egui::RichText::new("Mayor calidad genera archivos más grandes").small().color(theme::MUTED));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Fondo para transparencias");
                            changed |= ui.color_edit_button_srgb(&mut self.recipe.jpeg_background).changed();
                        });
                    }
                    ui.add_space(12.0);
                    egui::CollapsingHeader::new("Color y metadatos de las copias").show(ui, |ui| {
                        ui.label("Se normaliza la orientación EXIF y se convierte a sRGB de 8 bits. Las copias eliminan los metadatos originales, incluidos EXIF, GPS y XMP.");
                        ui.label("Se admiten ICC RGB de matriz/curvas. Sin ICC se asume sRGB; HDR y otros perfiles se rechazan.");
                    });

                    if changed {
                        self.plan = None; self.results.clear(); self.completed = 0; self.journal = None; self.recovery = None;
                    }
                    ui.add_space(4.0);
                    });
                    if self.results.is_empty() {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Las copias se orientan y convierten a sRGB. No conservan los metadatos originales.").small().color(theme::MUTED));
                        ui.checkbox(&mut self.acknowledged, "Acepto la conversión de color y eliminación de metadatos");
                    }
                });
                if let Some(report) = &self.recovery {
                    ui.separator();
                    ui.label(format!("Recuperación: {} verificadas · {} pendientes · {} conflictos",
                        report.published, report.pending.items.len(), report.conflicts.len()));
                    egui::ScrollArea::vertical().id_salt("recovery-conflicts").max_height(100.0)
                        .show_rows(ui, 24.0, report.conflicts.len(), |ui, rows| {
                            for index in rows {
                                ui.add(egui::Label::new(egui::RichText::new(&report.conflicts[index])
                                    .color(egui::Color32::LIGHT_RED)).truncate()).on_hover_text(&report.conflicts[index]);
                            }
                        });
                    ui.label(egui::RichText::new("Solo se exportarán los pendientes comprobados. Los conflictos se conservan.").small().color(theme::MUTED));
                }
                if let Some(plan) = &self.plan {
                    ui.add_space(8.0); ui.separator(); ui.add_space(8.0);
                    theme::eyebrow(ui, if self.results.is_empty() { "REVISA LAS SALIDAS" } else { "RESULTADO POR IMAGEN" });
                    ui.label(egui::RichText::new(format!("{} {} · {}", plan.items.len(), if plan.items.len() == 1 { "copia" } else { "copias" },
                        plan.items.first().and_then(|item| item.destination.parent())
                            .map_or_else(String::new, theme::display_path))).small().color(theme::MUTED));
                    egui::ScrollArea::vertical().id_salt("batch-plan").max_height(160.0).auto_shrink([false, true])
                        .show_rows(ui, 26.0, plan.items.len(), |ui, rows| {
                            for index in rows {
                                let item = &plan.items[index];
                                let status = self.results.get(index).and_then(Option::as_ref);
                                let name = item.destination.file_name().unwrap_or_default().to_string_lossy();
                                let (state, color) = match status {
                                    Some(Ok(())) => ("Publicada".to_owned(), egui::Color32::from_rgb(149, 207, 166)),
                                    Some(Err(_)) => ("Sin publicar".to_owned(), egui::Color32::LIGHT_RED),
                                    None => (format!("{} × {} px", item.output_size.0, item.output_size.1), theme::MUTED),
                                };
                                ui.horizontal(|ui| {
                                    let width = (ui.available_width() - 130.0).max(80.0);
                                    ui.allocate_ui_with_layout(egui::vec2(width, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        ui.set_min_width(width);
                                        ui.add(egui::Label::new(name.as_ref()).truncate())
                                    }).inner
                                        .on_hover_text(format!("{} → {}", theme::display_path(&item.source), theme::display_path(&item.destination)));
                                    let response = ui.label(egui::RichText::new(state).small().color(color));
                                    if let Some(Err(error)) = status { response.on_hover_text(error); }
                                });
                            }
                        });
                    ui.add_space(8.0);
                }
                if self.busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if self.results.is_empty() { "Comprobando archivos…" } else { "Exportando copias…" });
                    });
                    ui.label(egui::RichText::new("Puedes cerrar este panel y seguir navegando.").small().color(theme::MUTED));
                }
                if !self.results.is_empty() {
                    let published = self.results.iter().filter(|result| matches!(result, Some(Ok(())))).count();
                    let failed = self.results.iter().filter(|result| matches!(result, Some(Err(_)))).count();
                    ui.add(egui::ProgressBar::new(self.completed as f32 / self.results.len() as f32)
                        .fill(theme::ACCENT).desired_height(6.0).corner_radius(2));
                    ui.label(egui::RichText::new(format!("{} / {} procesadas", self.completed, self.results.len())).small().color(theme::MUTED));
                    ui.label(format!("{published} publicadas · {failed} sin publicar{}",
                        if self.cancelled { " · Lote cancelado" } else if !self.busy { " · Finalizado" } else { "" }));
                    if !self.busy && ui.button("Preparar otro lote con estas imágenes").clicked() {
                        self.plan = None; self.results.clear(); self.completed = 0; self.recovery = None;
                        self.journal = None; self.error = None; self.acknowledged = false;
                    }
                }
                if let Some(error) = &self.error {
                    ui.add_space(8.0);
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                if let Some(path) = &self.journal {
                    egui::CollapsingHeader::new("Registro de la exportación").show(ui, |ui| {
                        ui.add(egui::Label::new(theme::display_path(path)).wrap());
                        if ui.button("Copiar ruta del registro").clicked() { ctx.copy_text(theme::display_path(path)); }
                    });
                }
                #[cfg(feature = "capture")]
                if matches!(std::env::var("RASTERA_CAPTURE_TUTORIAL").as_deref(), Ok("review" | "results")) && self.plan.is_some() {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if self.busy {
                        if ui.button("Cancelar lote").clicked() { self.worker.cancel(); }
                    } else if self.results.is_empty() {
                        if let Some(plan) = &self.plan {
                            let count = plan.items.len();
                            if ui.add_enabled(self.acknowledged && self.dialog.is_none(), theme::primary(format!("Exportar {count} {}", if count == 1 { "copia" } else { "copias" })))
                                .on_hover_text(if self.acknowledged { "Crear las copias revisadas" } else { "Acepta la política de color y metadatos para continuar" }).clicked() { self.execute(); }
                        } else if ui.add_enabled(!self.paths.is_empty() && !self.destination.trim().is_empty() && self.dialog.is_none(),
                            theme::primary("Revisar salidas")).on_hover_text("Comprueba nombres y dimensiones antes de exportar").clicked() { self.prepare(); }
                    }
                    if ui.button(if self.busy { "Ocultar panel" } else { "Cerrar" }).clicked() { close_requested = true; }
                });
                ui.label(egui::RichText::new("Los originales se conservan. No se reemplazan archivos.").small().color(theme::MUTED));
            });
        self.open = open && !close_requested;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn settle(batch: &mut Batch, operation: &str) {
        // These integration tests use durable SQLite commits and file syncs on shared CI runners.
        // Keep an absolute deadline so a stalled worker still fails, regardless of partial progress.
        let started = std::time::Instant::now();
        let deadline = started + std::time::Duration::from_secs(30);
        while batch.busy {
            batch.receive();
            if !batch.busy {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Batch timeout during {operation} after {:?}: generation={}, completed={}, \
                 planned_items={}, results={:?}, journal={:?}, cancelled={}, error={:?}",
                started.elapsed(),
                batch.generation,
                batch.completed,
                batch.plan.as_ref().map_or(0, |plan| plan.items.len()),
                batch.results,
                batch.journal,
                batch.cancelled,
                batch.error,
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn reviewed_batch_requires_policy_acceptance_and_retains_its_report() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("image.png");
        image::RgbaImage::from_pixel(80, 40, image::Rgba([40, 90, 160, 128]))
            .save(&source)
            .unwrap();
        let output = dir.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let budget = MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET);
        let mut batch = Batch::new(&egui::Context::default(), budget.clone());
        batch.select(vec![source.clone()]);
        batch.destination_path = Some(output.clone());
        batch.destination = output.to_string_lossy().into_owned();
        batch.edge = 20;
        batch.prepare();
        settle(&mut batch, "plan review");
        assert!(batch.error.is_none(), "{:?}", batch.error);
        assert_eq!(batch.plan.as_ref().unwrap().items[0].output_size, (20, 10));
        batch.execute();
        assert!(!batch.busy);
        assert!(!output.join("image-rastera.png").exists());
        batch.acknowledged = true;
        batch.execute();
        settle(&mut batch, "reviewed export");
        assert_eq!(batch.completed, 1);
        assert!(matches!(batch.results[0], Some(Ok(()))));
        assert!(batch.journal.as_ref().unwrap().exists());
        assert_eq!(
            image::image_dimensions(output.join("image-rastera.png")).unwrap(),
            (20, 10)
        );
        batch.open = false;
        batch.select(vec![source]);
        assert!(batch.open);
        assert_eq!(batch.completed, 1);
        assert_eq!(budget.used(), 0);
    }
    #[test]
    fn recovery_reviews_pending_items_and_requires_acceptance_before_resume() {
        use rastera_core::BatchBackend;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("imagen.png");
        image::RgbaImage::new(8, 4).save(&source).unwrap();
        let output = dir.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let plan = LocalBatch
            .plan(
                &[source],
                &output,
                ExportRecipe::default(),
                &Default::default(),
            )
            .unwrap();
        let session = LocalBatch.start(&plan).unwrap();
        let journal = session.journal_path().to_path_buf();
        drop(session);
        let mut batch = Batch::new(
            &egui::Context::default(),
            MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET),
        );
        batch.recover(journal.clone());
        settle(&mut batch, "journal recovery");
        assert!(batch.error.is_none(), "{:?}", batch.error);
        assert_eq!(batch.recovery.as_ref().unwrap().pending.items.len(), 1);
        batch.execute();
        assert!(!batch.busy);
        assert!(!plan.items[0].destination.exists());
        batch.acknowledged = true;
        batch.execute();
        settle(&mut batch, "resumed export");
        assert_eq!(batch.completed, 1);
        assert_ne!(batch.journal.as_ref().unwrap(), &journal);
        assert!(plan.items[0].destination.exists());
    }
}
