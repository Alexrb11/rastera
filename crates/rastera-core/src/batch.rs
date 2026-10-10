use crate::{CancellationToken, ErrorKind, ImageInfo, RasteraError, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
    WebP,
}
impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportRecipe {
    pub format: ExportFormat,
    pub max_edge: Option<u32>,
    pub rotation: u16,
    pub jpeg_quality: u8,
    pub jpeg_background: [u8; 3],
}
impl Default for ExportRecipe {
    fn default() -> Self {
        Self {
            format: ExportFormat::Png,
            max_edge: Some(1920),
            rotation: 0,
            jpeg_quality: 90,
            jpeg_background: [255; 3],
        }
    }
}
impl ExportRecipe {
    pub fn validate(&self) -> Result<()> {
        if self.max_edge.is_some_and(|edge| edge == 0 || edge > 64_000)
            || ![0, 90, 180, 270].contains(&self.rotation)
            || !(1..=100).contains(&self.jpeg_quality)
        {
            return Err(RasteraError::new(
                ErrorKind::Unsupported,
                "Parámetros de exportación fuera de rango",
            ));
        }
        Ok(())
    }
    pub fn dimensions(&self, mut width: u32, mut height: u32) -> (u32, u32) {
        if self.rotation == 90 || self.rotation == 270 {
            std::mem::swap(&mut width, &mut height);
        }
        if let Some(edge) = self.max_edge
            && width.max(height) > edge
        {
            let ratio = f64::from(edge) / f64::from(width.max(height));
            width = (f64::from(width) * ratio).round().max(1.0) as u32;
            height = (f64::from(height) * ratio).round().max(1.0) as u32;
        }
        (width, height)
    }
}
#[derive(Clone, Debug)]
pub struct ExportItem {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub info: ImageInfo,
    pub output_size: (u32, u32),
    pub source_hash: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct ExportPlan {
    pub items: Vec<ExportItem>,
    pub recipe: ExportRecipe,
}
#[derive(Clone, Debug)]
pub struct RecoveryReport {
    pub journal: PathBuf,
    pub total: usize,
    pub published: usize,
    pub conflicts: Vec<String>,
    pub pending: ExportPlan,
}
pub trait ExportSession: Send {
    fn journal_path(&self) -> &Path;
    fn export(
        &mut self,
        index: usize,
        item: &ExportItem,
        recipe: &ExportRecipe,
        cancel: &CancellationToken,
    ) -> Result<()>;
    fn record(&mut self, index: usize, state: &str, message: &str) -> Result<()>;
    fn finish(&mut self, cancelled: bool) -> Result<()>;
}
pub trait BatchBackend: Send + Sync + 'static {
    fn recover(&self, _journal: &Path, _cancel: &CancellationToken) -> Result<RecoveryReport> {
        Err(RasteraError::new(
            ErrorKind::Unsupported,
            "Este backend no admite recuperación",
        ))
    }
    fn plan(
        &self,
        sources: &[PathBuf],
        destination: &Path,
        recipe: ExportRecipe,
        cancel: &CancellationToken,
    ) -> Result<ExportPlan>;
    fn start(&self, plan: &ExportPlan) -> Result<Box<dyn ExportSession>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_preserves_ratio_does_not_upscale_and_rotates_first() {
        let recipe = ExportRecipe {
            rotation: 90,
            max_edge: Some(100),
            ..Default::default()
        };
        assert_eq!(recipe.dimensions(400, 200), (50, 100));
        assert_eq!(recipe.dimensions(40, 20), (20, 40));
        assert_eq!(recipe.dimensions(64000, 1), (1, 100));
        assert!(
            ExportRecipe {
                rotation: 45,
                ..recipe
            }
            .validate()
            .is_err()
        );
    }
}
