use std::{
    fmt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

pub const MAX_PIXELS: u64 = 64_000_000;
pub const CPU_IMAGE_BUDGET: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Io,
    Unsupported,
    InvalidImage,
    TooLarge,
    Cancelled,
    Changed,
    Busy,
    Internal,
}

#[derive(Clone, Debug)]
pub struct RasteraError {
    pub kind: ErrorKind,
    pub message: String,
}

impl RasteraError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for RasteraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for RasteraError {}

pub type Result<T> = std::result::Result<T, RasteraError>;

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Acquire) {
            Err(RasteraError::new(
                ErrorKind::Cancelled,
                "Operación cancelada",
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    Jpeg,
    Png,
    WebP,
}

impl fmt::Display for ImageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::WebP => "WebP",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub length: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Debug)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub kind: ImageKind,
    pub stamp: FileStamp,
    pub has_color_profile: bool,
}

impl ImageInfo {
    pub fn pixels(&self) -> Result<u64> {
        let pixels = u64::from(self.width) * u64::from(self.height);
        if pixels == 0 || pixels > MAX_PIXELS {
            return Err(RasteraError::new(
                ErrorKind::TooLarge,
                "La imagen supera el límite de 64 millones de píxeles o tiene dimensiones vacías",
            ));
        }
        Ok(pixels)
    }

    /// Conservative reservation for decoding, conversion, preview and handoff.
    pub fn working_bytes(&self) -> Result<u64> {
        self.pixels()?
            .checked_mul(16)
            .ok_or_else(|| RasteraError::new(ErrorKind::TooLarge, "Dimensiones fuera de rango"))
    }
}

#[derive(Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub info: ImageInfo,
}

pub trait ImageBackend: Send + Sync + 'static {
    fn inspect(&self, path: &Path) -> Result<ImageInfo>;
    fn decode(
        &self,
        path: &Path,
        info: &ImageInfo,
        max_edge: u32,
        cancel: &CancellationToken,
    ) -> Result<DecodedImage>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_extreme_dimensions_before_allocation() {
        for (width, height) in [(0, 20), (u32::MAX, u32::MAX), (8_001, 8_000)] {
            let info = ImageInfo {
                width,
                height,
                kind: ImageKind::Png,
                stamp: FileStamp {
                    length: 0,
                    modified: None,
                },
                has_color_profile: false,
            };
            assert_eq!(info.working_bytes().unwrap_err().kind, ErrorKind::TooLarge);
        }
    }
}
