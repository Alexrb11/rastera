//! Reproducible, headless pipeline measurements; CSV output never replaces a file.
use rastera_adapters::{CachedThumbnails, LocalFolders, LocalImages};
use rastera_core::{FolderBackend, ImageBackend, Result};
use rastera_engine::{ImageLoader, memory::MemoryBudget};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 3 && args[0] == "--generate" {
        let count: usize = args[2].to_string_lossy().parse()?;
        if !(1..=10_000).contains(&count) {
            return Err("El corpus debe contener entre 1 y 10.000 archivos".into());
        }
        let root = PathBuf::from(&args[1]);
        fs::create_dir(&root)?; // Refuse an existing directory.
        let image = image::RgbImage::from_fn(64, 48, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 5) as u8, ((x + y) * 2) as u8])
        });
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image).write_to(&mut bytes, image::ImageFormat::Png)?;
        for index in 0..count {
            File::create_new(root.join(format!("image_{index:05}.png")))?
                .write_all(bytes.get_ref())?;
        }
        println!(
            "{count} PNG de 64×48 · {} bytes por archivo · contenido idéntico, rutas distintas",
            bytes.get_ref().len()
        );
        return Ok(());
    }
    if !(2..=3).contains(&args.len()) {
        return Err("Uso: measure <carpeta> <salida.csv> [repeticiones=5] | --generate <carpeta nueva> <cantidad>".into());
    }
    let root = fs::canonicalize(PathBuf::from(&args[0]))?;
    let output = PathBuf::from(&args[1]);
    let repetitions: usize = args.get(2).map_or(Ok(5), |v| v.to_string_lossy().parse())?;
    if !(1..=20).contains(&repetitions) {
        return Err("Usa de 1 a 20 repeticiones".into());
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let cache_root = tempfile::Builder::new()
        .prefix("rastera-measure-")
        .tempdir_in(parent)?;
    let mut csv = BufWriter::new(File::create_new(&output)?);
    writeln!(
        csv,
        "run,phase,files,source_bytes,first_page_ms,scan_ms,elapsed_ms,reserved_peak_bytes,reserved_after_bytes,cache_hits,cache_misses"
    )?;
    for run in 1..=repetitions {
        let scan = Instant::now();
        let mut first_page = None;
        let mut paths = Vec::new();
        LocalFolders.scan(&root, &Default::default(), &mut |page| {
            first_page.get_or_insert_with(|| scan.elapsed());
            paths.extend(page.into_iter().map(|entry| entry.path));
            Ok(())
        })?;
        let scan_elapsed = scan.elapsed();
        paths.sort();
        if paths.is_empty() {
            return Err("Carpeta sin imágenes".into());
        }
        let bytes: u64 = paths
            .iter()
            .map(|p| fs::metadata(p).map(|m| m.len()))
            .collect::<std::io::Result<Vec<_>>>()?
            .iter()
            .sum();
        // Unique cache per run: cold application cache, warm OS cache may still apply.
        let cache = Arc::new(CachedThumbnails::new(Some(
            cache_root.path().join(run.to_string()),
        )));
        for phase in ["cold", "warm"] {
            let before = cache.stats();
            let budget = MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET);
            let (loader, wake) = loader(cache.clone(), budget.clone());
            let start = Instant::now();
            for (index, path) in paths.iter().enumerate() {
                loader.request(index as u64, path.clone(), 256);
                receive(&loader, &wake)?.result?;
            }
            let elapsed = start.elapsed();
            drop(loader);
            let after = cache.stats();
            writeln!(
                csv,
                "{run},{phase},{},{},{:.3},{:.3},{:.3},{},{},{},{}",
                paths.len(),
                bytes,
                first_page.unwrap_or_default().as_secs_f64() * 1000.0,
                scan_elapsed.as_secs_f64() * 1000.0,
                elapsed.as_secs_f64() * 1000.0,
                budget.peak(),
                budget.used(),
                after.hits - before.hits,
                after.misses - before.misses
            )?;
        }
        // Measure preview and complete decoding of one input separately.
        for (phase, full) in [("preview", false), ("original", true)] {
            let budget = MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET);
            let (loader, wake) = loader(Arc::new(LocalImages), budget.clone());
            let start = Instant::now();
            if full {
                let info = LocalImages.inspect(&paths[0])?;
                loader.request_original(1, paths[0].clone(), info.stamp);
            } else {
                loader.request(1, paths[0].clone(), 4096);
            }
            let resident = receive(&loader, &wake)?.into_resident()?;
            let elapsed = start.elapsed();
            drop(resident);
            drop(loader);
            writeln!(
                csv,
                "{run},{phase},1,{},0,0,{:.3},{},{},0,0",
                fs::metadata(&paths[0])?.len(),
                elapsed.as_secs_f64() * 1000.0,
                budget.peak(),
                budget.used()
            )?;
        }
        csv.flush()?;
        println!("Repetición {run}/{repetitions}: {} imágenes", paths.len());
    }
    Ok(())
}
fn loader(backend: Arc<dyn ImageBackend>, budget: MemoryBudget) -> (ImageLoader, Receiver<()>) {
    let (tx, rx) = mpsc::sync_channel(1);
    (
        ImageLoader::new(
            backend,
            budget,
            Arc::new(move || {
                let _ = tx.try_send(());
            }),
        ),
        rx,
    )
}
fn receive(loader: &ImageLoader, wake: &Receiver<()>) -> Result<rastera_engine::LoadedImage> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(event) = loader.try_recv() {
            return Ok(event);
        }
        if wake
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .is_err()
        {
            loader.cancel();
            return Err(rastera_core::RasteraError::new(
                rastera_core::ErrorKind::Busy,
                "La carga superó 60 segundos",
            ));
        }
    }
}
