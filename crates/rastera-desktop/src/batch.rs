use crossbeam_channel::{Receiver, bounded};
use eframe::egui;
use rastera_adapters::LocalBatch;
use rastera_core::{ErrorKind, ExportFormat, ExportPlan, ExportRecipe};
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
                BatchUpdate::Planned(result) => {
                    self.busy = false;
                    match result {
                        Ok(plan) => {
                            self.plan = Some(plan);
                            self.error = None;
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
    }

    #[cfg(feature = "capture")]
    pub fn capture_plan(&mut self, paths: Vec<PathBuf>, destination: PathBuf) {
        self.select(paths);
        self.destination = destination.to_string_lossy().into_owned();
        self.destination_path = Some(destination);
        self.prepare();
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let mut open = self.open;
        egui::Window::new("Exportar por lotes").open(&mut open).default_width(620.0).default_height(550.0)
            .default_pos(ctx.viewport_rect().center() - egui::vec2(310.0, 275.0))
            .vscroll(true).show(ctx, |ui| {
                ui.label(format!("{} originales seleccionados", self.paths.len()));
                let mut changed = false;
                ui.add_enabled_ui(!self.busy && self.dialog.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Destino");
                        if ui.add(egui::TextEdit::singleline(&mut self.destination).desired_width(350.0)).changed() {
                            self.destination_path = None;
                            changed = true;
                        }
                        if ui.button("Elegir carpeta").clicked() { self.pick_destination(ctx); }
                    });
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("batch-format").selected_text(match self.recipe.format {
                            ExportFormat::Png => "PNG", ExportFormat::Jpeg => "JPEG", ExportFormat::WebP => "WebP sin pérdidas",
                        }).show_ui(ui, |ui| {
                            for (format, label) in [(ExportFormat::Png, "PNG"), (ExportFormat::Jpeg, "JPEG"), (ExportFormat::WebP, "WebP sin pérdidas")] {
                                changed |= ui.selectable_value(&mut self.recipe.format, format, label).changed();
                            }
                        });
                        egui::ComboBox::from_id_salt("batch-rotation").selected_text(format!("Rotación: {}°", self.recipe.rotation)).show_ui(ui, |ui| {
                            for rotation in [0, 90, 180, 270] {
                                changed |= ui.selectable_value(&mut self.recipe.rotation, rotation, format!("{rotation}° horario")).changed();
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut self.resize, "Limitar lado mayor").changed();
                        changed |= ui.add_enabled(self.resize, egui::DragValue::new(&mut self.edge).range(1..=64000).suffix(" px")).changed();
                        ui.label("Mantiene proporción, sin ampliar.");
                    });
                    if self.recipe.format == ExportFormat::Jpeg {
                        changed |= ui.add(egui::Slider::new(&mut self.recipe.jpeg_quality, 1..=100).text("Calidad JPEG")).changed();
                        ui.horizontal(|ui| {
                            ui.label("Fondo para transparencias:");
                            changed |= ui.color_edit_button_srgb(&mut self.recipe.jpeg_background).changed();
                        });
                    }
                    ui.label("Se normaliza la orientación EXIF. Las salidas eliminan EXIF, GPS, XMP y otros metadatos.");
                    ui.label("Solo 8 bits sin ICC; se asume sRGB. Los espacios de color PNG no compatibles se rechazan.");
                    ui.checkbox(&mut self.acknowledged, "Acepto esta política de color y metadatos");
                    if ui.add_enabled(!self.paths.is_empty() && !self.destination.trim().is_empty(), egui::Button::new("Revisar plan")).clicked() { self.prepare(); }
                });
                if changed { self.plan = None; self.results.clear(); self.completed = 0; self.journal = None; }
                ui.separator();
                if let Some(plan) = &self.plan {
                    egui::ScrollArea::vertical().id_salt("batch-plan").max_height(180.0)
                        .show_rows(ui, 22.0, plan.items.len(), |ui, rows| {
                            for index in rows {
                                let item = &plan.items[index];
                                let status = self.results.get(index).and_then(Option::as_ref);
                                let name = item.destination.file_name().unwrap_or_default().to_string_lossy();
                                match status {
                                    Some(Ok(())) => { ui.add(egui::Label::new(egui::RichText::new(format!("{name} · Publicada")).color(egui::Color32::LIGHT_GREEN)).truncate()); }
                                    Some(Err(error)) => { ui.add(egui::Label::new(egui::RichText::new(format!("{name} · {error}")).color(egui::Color32::LIGHT_RED)).truncate()).on_hover_text(error); }
                                    None => { ui.add(egui::Label::new(format!("{name} · {} × {} px", item.output_size.0, item.output_size.1)).truncate()); }
                                }
                            }
                        });
                    if !self.busy && self.results.is_empty()
                        && ui.add_enabled(self.acknowledged && self.dialog.is_none(), egui::Button::new("Exportar copias")).clicked()
                    {
                        self.execute();
                    }
                }
                if self.busy {
                    ui.spinner();
                    if ui.button("Cancelar").clicked() { self.worker.cancel(); }
                }
                if !self.results.is_empty() {
                    let published = self.results.iter().filter(|result| matches!(result, Some(Ok(())))).count();
                    ui.add(egui::ProgressBar::new(self.completed as f32 / self.results.len() as f32).text(format!(
                        "{} / {} procesadas · {} publicadas{}", self.completed, self.results.len(), published,
                        if self.cancelled { " · cancelado" } else { "" })));
                }
                if let Some(error) = &self.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
                if let Some(path) = &self.journal { ui.label(format!("Diario: {}", path.display())); }
                ui.label("Nunca se reemplaza una salida existente. Cancelar conserva las copias ya publicadas.");
            });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settle(batch: &mut Batch) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while batch.busy {
            batch.receive();
            assert!(std::time::Instant::now() < deadline, "Batch timeout");
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
        settle(&mut batch);
        assert!(batch.error.is_none(), "{:?}", batch.error);
        assert_eq!(batch.plan.as_ref().unwrap().items[0].output_size, (20, 10));
        batch.execute();
        assert!(!batch.busy);
        assert!(!output.join("image-rastera.png").exists());
        batch.acknowledged = true;
        batch.execute();
        settle(&mut batch);
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
}
