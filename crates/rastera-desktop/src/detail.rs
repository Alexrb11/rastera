use eframe::egui::{self, Color32, Rect, TextureHandle};
use rastera_adapters::LocalImages;
use rastera_core::ImageInfo;
use rastera_engine::{ImageLoader, ResidentImage, memory::MemoryBudget};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

const TILE: u32 = 512;
const MAX_TILES: usize = 60;
const UPLOADS_PER_FRAME: usize = 4;
struct Tile {
    bytes: u64,
    texture: TextureHandle,
    uv: Rect,
    touched: u64,
}
pub struct DetailView {
    loader: ImageLoader,
    budget: MemoryBudget,
    source: Option<ResidentImage>,
    tiles: HashMap<(u32, u32), Tile>,
    generation: u64,
    clock: u64,
    upload_frame: u64,
    frame_uploads: usize,
    loading: bool,
    tiles_pending: bool,
    attempted: bool,
    pub error: Option<String>,
}
impl DetailView {
    pub fn new(ctx: &egui::Context, budget: MemoryBudget) -> Self {
        let wake = ctx.clone();
        Self {
            loader: ImageLoader::new(
                Arc::new(LocalImages),
                budget.clone(),
                Arc::new(move || wake.request_repaint()),
            ),
            budget,
            source: None,
            tiles: HashMap::new(),
            generation: 0,
            clock: 0,
            upload_frame: u64::MAX,
            frame_uploads: 0,
            loading: false,
            tiles_pending: false,
            attempted: false,
            error: None,
        }
    }
    pub fn clear(&mut self) {
        if self.loading {
            self.budget.set_foreground_loading(false);
        }
        self.generation += 1;
        self.loader.cancel();
        self.source = None;
        self.tiles.clear();
        self.loading = false;
        self.tiles_pending = false;
        self.attempted = false;
        self.error = None;
    }
    pub fn request(&mut self, path: PathBuf, info: &ImageInfo) {
        if self.attempted {
            return;
        }
        self.attempted = true;
        self.loading = true;
        self.budget.set_foreground_loading(true);
        self.generation += 1;
        self.loader
            .request_original(self.generation, path, info.stamp.clone());
    }
    pub fn receive(&mut self) {
        while let Some(event) = self.loader.try_recv() {
            if event.generation != self.generation {
                continue;
            }
            self.loading = false;
            self.budget.set_foreground_loading(false);
            match event.into_resident() {
                Ok(source) => self.source = Some(source),
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }
    pub fn busy(&self) -> bool {
        self.loading || self.tiles_pending
    }
    pub fn ready(&self) -> bool {
        self.source.is_some()
    }
    pub fn active(&self) -> bool {
        self.attempted
    }
    pub fn diagnostics(&self) -> (u64, rastera_engine::ImageLoadStats) {
        (
            self.tiles.values().map(|tile| tile.bytes).sum(),
            self.loader.stats(),
        )
    }

    /// Draw each visible region exactly once, including transparent fallback regions.
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        image_rect: Rect,
        canvas: Rect,
        preview: &TextureHandle,
    ) {
        let Some(source) = &self.source else { return };
        let image = &source.image;
        let visible = visible_tiles(image.width, image.height, image_rect, canvas);
        let mut wanted = visible.clone();
        wanted.sort_by(|a, b| {
            tile_rect(*a, image.width, image.height, image_rect)
                .center()
                .distance_sq(canvas.center())
                .total_cmp(
                    &tile_rect(*b, image.width, image.height, image_rect)
                        .center()
                        .distance_sq(canvas.center()),
                )
        });
        wanted.truncate(MAX_TILES);
        let pinned: HashSet<_> = wanted.iter().copied().collect();
        if self.upload_frame != ctx.cumulative_frame_nr() {
            self.upload_frame = ctx.cumulative_frame_nr();
            self.frame_uploads = 0;
        }
        self.tiles_pending = false;
        for coord in &wanted {
            self.clock += 1;
            if let Some(tile) = self.tiles.get_mut(coord) {
                tile.touched = self.clock;
                continue;
            }
            if self.frame_uploads == UPLOADS_PER_FRAME {
                self.tiles_pending = true;
                continue;
            }
            if self.tiles.len() == MAX_TILES {
                let oldest = self
                    .tiles
                    .iter()
                    .filter(|(key, _)| !pinned.contains(key))
                    .min_by_key(|(_, tile)| tile.touched)
                    .map(|(key, _)| *key);
                if let Some(oldest) = oldest {
                    self.tiles.remove(&oldest);
                } else {
                    continue;
                }
            }
            let (pixels, uv) = tile_pixels(image, *coord);
            let bytes = pixels.pixels.len() as u64 * 4;
            let texture = ctx.load_texture(
                format!("detail-{}-{}-{}", self.generation, coord.0, coord.1),
                pixels,
                egui::TextureOptions::LINEAR,
            );
            self.tiles.insert(
                *coord,
                Tile {
                    bytes,
                    texture,
                    uv,
                    touched: self.clock,
                },
            );
            self.frame_uploads += 1;
        }
        for coord in visible {
            let rect = tile_rect(coord, image.width, image.height, image_rect);
            if let Some(tile) = self.tiles.get(&coord) {
                painter.image(tile.texture.id(), rect, tile.uv, Color32::WHITE);
            } else {
                let uv = Rect::from_min_max(
                    ((rect.min - image_rect.min) / image_rect.size()).to_pos2(),
                    ((rect.max - image_rect.min) / image_rect.size()).to_pos2(),
                );
                painter.image(preview.id(), rect, uv, Color32::WHITE);
            }
        }
        if self.tiles_pending {
            ctx.request_repaint();
        }
    }
}
fn visible_tiles(width: u32, height: u32, image: Rect, canvas: Rect) -> Vec<(u32, u32)> {
    let clip = image.intersect(canvas);
    if !clip.is_positive() || !image.is_positive() {
        return Vec::new();
    }
    let start = (clip.min - image.min) / image.size();
    let end = (clip.max - image.min) / image.size();
    let left = (start.x * width as f32).floor().max(0.0) as u32 / TILE;
    let top = (start.y * height as f32).floor().max(0.0) as u32 / TILE;
    let right = ((end.x * width as f32).ceil() as u32)
        .min(width)
        .div_ceil(TILE);
    let bottom = ((end.y * height as f32).ceil() as u32)
        .min(height)
        .div_ceil(TILE);
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .collect()
}
fn tile_rect((column, row): (u32, u32), width: u32, height: u32, image: Rect) -> Rect {
    let min = egui::vec2(
        (column * TILE) as f32 / width as f32,
        (row * TILE) as f32 / height as f32,
    );
    let max = egui::vec2(
        ((column + 1) * TILE).min(width) as f32 / width as f32,
        ((row + 1) * TILE).min(height) as f32 / height as f32,
    );
    Rect::from_min_max(
        image.min + min * image.size(),
        image.min + max * image.size(),
    )
}
fn tile_pixels(
    image: &rastera_core::DecodedImage,
    (column, row): (u32, u32),
) -> (egui::ColorImage, Rect) {
    let core_x = column * TILE;
    let core_y = row * TILE;
    let end_x = (core_x + TILE).min(image.width);
    let end_y = (core_y + TILE).min(image.height);
    // A one-pixel sampling border prevents linear-filter seams between tiles.
    let x = core_x.saturating_sub(1);
    let y = core_y.saturating_sub(1);
    let width = (end_x + 1).min(image.width) - x;
    let height = (end_y + 1).min(image.height) - y;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in y..y + height {
        let offset = ((row as usize * image.width as usize) + x as usize) * 4;
        pixels.extend_from_slice(&image.rgba[offset..offset + width as usize * 4]);
    }
    let uv = Rect::from_min_max(
        egui::pos2(
            (core_x - x) as f32 / width as f32,
            (core_y - y) as f32 / height as f32,
        ),
        egui::pos2(
            (end_x - x) as f32 / width as f32,
            (end_y - y) as f32 / height as f32,
        ),
    );
    (
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &pixels),
        uv,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rastera_core::ImageBackend;
    fn settle(detail: &mut DetailView) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while detail.loading {
            detail.receive();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn visibility_and_border_crop_handle_edges_and_offscreen_images() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1025.0, 2.0));
        assert_eq!(
            visible_tiles(
                1025,
                2,
                rect,
                Rect::from_min_size(egui::pos2(512.0, 0.0), egui::vec2(512.0, 2.0))
            ),
            vec![(1, 0)]
        );
        assert!(visible_tiles(1025, 2, rect, rect.translate(egui::vec2(2000.0, 0.0))).is_empty());
        let image = rastera_core::DecodedImage {
            source_size: (1025, 2),
            width: 1025,
            height: 2,
            rgba: [10, 20, 30, 128].repeat(2050),
            info: rastera_core::ImageInfo {
                width: 1025,
                height: 2,
                kind: rastera_core::ImageKind::Png,
                stamp: rastera_core::FileStamp {
                    length: 0,
                    modified: None,
                },
                has_color_profile: false,
            },
        };
        let (pixels, uv) = tile_pixels(&image, (1, 0));
        assert_eq!(pixels.size, [514, 2]);
        assert_eq!(uv.min.x, 1.0 / 514.0);
        assert_eq!(uv.max.x, 513.0 / 514.0);
        assert!(pixels.pixels.iter().all(|pixel| pixel.a() == 128));
        let (pixels, uv) = tile_pixels(&image, (2, 0));
        assert_eq!(pixels.size, [2, 2]);
        assert_eq!(uv.min.x, 0.5);
        assert_eq!(uv.max.x, 1.0);
    }
    #[test]
    fn bounded_uploads_and_residency_are_released_when_detail_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.png");
        image::RgbaImage::from_pixel(64_000, 16, image::Rgba([20, 40, 60, 128]))
            .save(&path)
            .unwrap();
        let info = LocalImages.inspect(&path).unwrap();
        let ctx = egui::Context::default();
        let budget = MemoryBudget::new(rastera_core::CPU_IMAGE_BUDGET);
        let mut detail = DetailView::new(&ctx, budget.clone());
        detail.request(path, &info);
        settle(&mut detail);
        assert!(detail.ready(), "{:?}", detail.error);
        assert_eq!(budget.used(), 64_000 * 16 * 8);
        let preview = ctx.load_texture(
            "fallback",
            egui::ColorImage::filled([1, 1], Color32::TRANSPARENT),
            Default::default(),
        );
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64_000.0, 16.0));
        for frame in 0..20 {
            let output = ctx.run_ui(Default::default(), |ui| {
                let ctx = ui.ctx();
                detail.paint(
                    ctx,
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    rect,
                    &preview,
                );
            });
            let uploaded = output.textures_delta.set.len();
            let shapes = output.shapes.len();
            output.drop_without_applying_deltas();
            // First frame also hands off fallback and egui's font atlas.
            assert!(
                uploaded <= UPLOADS_PER_FRAME + 2 * usize::from(frame == 0),
                "{uploaded} uploads"
            );
            assert!(detail.tiles.len() <= MAX_TILES);
            assert_eq!(
                shapes, 125,
                "Each transparent region must be painted only once"
            );
        }
        assert_eq!(detail.tiles.len(), MAX_TILES);
        assert!(!detail.busy());
        detail.clear();
        assert!(detail.tiles.is_empty());
        assert!(!detail.active());
        assert_eq!(budget.used(), 0);
    }
}
