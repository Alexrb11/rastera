mod app;
mod batch;
mod catalog;
mod detail;
mod grid;

fn main() -> eframe::Result {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--build-info")) {
        println!(
            "{{\"version\":\"{}\",\"capture\":{}}}",
            env!("CARGO_PKG_VERSION"),
            cfg!(feature = "capture")
        );
        return Ok(());
    }
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
