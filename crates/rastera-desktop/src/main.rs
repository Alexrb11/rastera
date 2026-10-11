#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod batch;
mod catalog;
mod detail;
mod grid;
#[cfg(feature = "capture")]
mod navigation;
mod theme;

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
    #[cfg(feature = "capture")]
    let capture_size = std::env::var("RASTERA_CAPTURE_SIZE")
        .ok()
        .and_then(|value| {
            let (width, height) = value.split_once('x')?;
            Some([
                width.parse::<f32>().ok()?.clamp(760.0, 2560.0),
                height.parse::<f32>().ok()?.clamp(480.0, 1600.0),
            ])
        });
    #[cfg(not(feature = "capture"))]
    let capture_size: Option<[f32; 2]> = None;
    let icon = eframe::icon_data::from_png_bytes(include_bytes!(
        "../../../assets/branding/rastera-256.png"
    ))
    .expect("the embedded Rastera icon must be a valid PNG");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_app_id("io.github.Alexrb11.rastera")
            .with_icon(icon)
            .with_inner_size(capture_size.unwrap_or([1180.0, 780.0]))
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
