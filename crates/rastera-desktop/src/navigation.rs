//! Development-only scripted navigation. Records UI readiness, never display latency or FPS.
use crate::grid::GridFrame;
use rastera_adapters::ThumbnailCacheStats;
use std::{
    fs::File,
    io::{BufWriter, Write},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub enum Action {
    Grid(usize),
    Viewer(usize),
}
struct Step {
    scene: &'static str,
    action: Action,
}
pub struct Observation {
    pub grid: GridFrame,
    pub viewer_ready: bool,
    pub viewer_error: bool,
    pub reserved: u64,
    pub peak: u64,
    pub textures: usize,
    pub cache: ThumbnailCacheStats,
    pub ui_elapsed: Duration,
}
pub struct NavigationBenchmark {
    csv: BufWriter<File>,
    steps: Vec<Step>,
    step: usize,
    dispatched: bool,
    started: Instant,
    frames: u64,
    ui_total: Duration,
    ui_max: Duration,
    before: Option<ThumbnailCacheStats>,
}
impl NavigationBenchmark {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("RASTERA_NAVIGATION_CSV")?;
        let result = (|| -> std::io::Result<Self> {
            let mut csv = BufWriter::new(File::create_new(path)?);
            writeln!(
                csv,
                "step,scene,requested_index,visible_first,visible_last,visible,ready,errors,status,ready_ms,ui_frames,ui_mean_ms,ui_max_ms,reserved_bytes,reserved_peak_bytes,thumbnail_textures,cache_hits,cache_misses,maintenance_runs,maintenance_ms"
            )?;
            csv.flush()?;
            Ok(Self {
                csv,
                steps: Vec::new(),
                step: 0,
                dispatched: false,
                started: Instant::now(),
                frames: 0,
                ui_total: Duration::ZERO,
                ui_max: Duration::ZERO,
                before: None,
            })
        })();
        match result {
            Ok(benchmark) => Some(benchmark),
            Err(error) => {
                eprintln!("No se pudo iniciar la medición: {error}");
                std::process::exit(1);
            }
        }
    }
    pub fn begin(
        &mut self,
        count: usize,
        folder_loading: bool,
        cache: ThumbnailCacheStats,
    ) -> Option<Action> {
        if folder_loading || count == 0 || self.dispatched {
            return None;
        }
        if !self.steps.is_empty() && self.step == self.steps.len() {
            return None;
        }
        if self.steps.is_empty() {
            let mut indices: Vec<_> = (0..count).step_by(20).collect();
            if indices.last() != Some(&(count - 1)) {
                indices.push(count - 1);
            }
            for &index in &indices {
                self.steps.push(Step {
                    scene: "grid_forward",
                    action: Action::Grid(index),
                });
            }
            for &index in indices.iter().rev().skip(1) {
                self.steps.push(Step {
                    scene: "grid_reverse",
                    action: Action::Grid(index),
                });
            }
            for index in 0..count.min(6) {
                self.steps.push(Step {
                    scene: "viewer",
                    action: Action::Viewer(index),
                });
            }
        }
        if self.step != 0 {
            self.started = Instant::now();
        }
        self.frames = 0;
        self.ui_total = Duration::ZERO;
        self.ui_max = Duration::ZERO;
        self.before = Some(cache);
        self.dispatched = true;
        Some(self.steps[self.step].action)
    }
    pub fn observe(&mut self, sample: Observation) -> std::io::Result<bool> {
        if !self.dispatched {
            if self.started.elapsed() > Duration::from_secs(30) {
                return Err(std::io::Error::other(
                    "La carpeta no quedó disponible en 30 segundos",
                ));
            }
            return Ok(false);
        }
        self.frames += 1;
        self.ui_total += sample.ui_elapsed;
        self.ui_max = self.ui_max.max(sample.ui_elapsed);
        let step = &self.steps[self.step];
        let requested = match step.action {
            Action::Grid(index) | Action::Viewer(index) => index,
        };
        let ready = match step.action {
            Action::Grid(index) => {
                sample.grid.visible > 0
                    && sample.grid.ready == sample.grid.visible
                    && sample.grid.first <= index
                    && sample.grid.last > index
            }
            Action::Viewer(_) => sample.viewer_ready || sample.viewer_error,
        };
        let timeout = self.started.elapsed() > Duration::from_secs(30);
        if !ready && !timeout {
            return Ok(false);
        }
        let errors = match step.action {
            Action::Grid(_) => sample.grid.errors,
            Action::Viewer(_) => usize::from(sample.viewer_error),
        };
        let status = if timeout {
            "timeout"
        } else if errors > 0 {
            "error"
        } else {
            "ready"
        };
        let before = self.before.expect("dispatched step has a baseline");
        writeln!(
            self.csv,
            "{},{},{},{},{},{},{},{},{},{:.3},{},{:.3},{:.3},{},{},{},{},{},{},{:.3}",
            self.step + 1,
            step.scene,
            requested,
            sample.grid.first,
            sample.grid.last,
            sample.grid.visible,
            sample.grid.ready,
            errors,
            status,
            self.started.elapsed().as_secs_f64() * 1000.0,
            self.frames,
            self.ui_total.as_secs_f64() * 1000.0 / self.frames as f64,
            self.ui_max.as_secs_f64() * 1000.0,
            sample.reserved,
            sample.peak,
            sample.textures,
            sample.cache.hits - before.hits,
            sample.cache.misses - before.misses,
            sample.cache.maintenance_runs - before.maintenance_runs,
            (sample.cache.maintenance_elapsed - before.maintenance_elapsed).as_secs_f64() * 1000.0
        )?;
        self.csv.flush()?;
        self.step += 1;
        self.dispatched = false;
        Ok(timeout || self.step == self.steps.len())
    }
}
