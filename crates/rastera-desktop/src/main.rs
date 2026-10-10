mod app;
mod batch;
mod catalog;
mod grid;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([760.0, 480.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        rastera_core::APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::RasteraApp::new(cc, initial)))),
    )
}
