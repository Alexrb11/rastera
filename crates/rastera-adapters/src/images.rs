use std::{
    fs::{self, File},
    io::BufReader,
    path::Path,
};

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use rastera_core::{
    CancellationToken, DecodedImage, ErrorKind, FileStamp, ImageBackend, ImageInfo, ImageKind,
    RasteraError, Result,
};

#[derive(Default)]
pub struct LocalImages;

pub(crate) fn io_error(error: std::io::Error) -> RasteraError {
    RasteraError::new(
        ErrorKind::Io,
        format!("No se pudo acceder al archivo: {error}"),
    )
}

pub(crate) fn image_error(error: image::ImageError) -> RasteraError {
    RasteraError::new(
        ErrorKind::InvalidImage,
        format!("No se pudo leer la imagen: {error}"),
    )
}

pub(crate) fn stamp(metadata: fs::Metadata) -> FileStamp {
    FileStamp {
        length: metadata.len(),
        modified: metadata.modified().ok(),
    }
}

pub(crate) fn decoder(path: &Path) -> Result<(ImageInfo, Box<dyn ImageDecoder>)> {
    let file = File::open(path).map_err(io_error)?;
    let version = stamp(file.metadata().map_err(io_error)?);
    if version.length > 128 * 1024 * 1024 {
        return Err(RasteraError::new(
            ErrorKind::TooLarge,
            "El archivo supera el límite de entrada de 128 MiB",
        ));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(64_000);
    limits.max_image_height = Some(64_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    let mut reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(io_error)?;
    reader.limits(limits.clone());
    let format = reader.format();
    let mut decoder: Box<dyn ImageDecoder> = match format {
        Some(ImageFormat::Png) => {
            let decoder =
                image::codecs::png::PngDecoder::with_limits(reader.into_inner(), limits.clone())
                    .map_err(image_error)?;
            if decoder.is_apng().map_err(image_error)? {
                return Err(RasteraError::new(
                    ErrorKind::Unsupported,
                    "Las imágenes PNG animadas todavía no están soportadas",
                ));
            }
            Box::new(decoder)
        }
        Some(ImageFormat::WebP) => {
            let decoder =
                image::codecs::webp::WebPDecoder::new(reader.into_inner()).map_err(image_error)?;
            if decoder.has_animation() {
                return Err(RasteraError::new(
                    ErrorKind::Unsupported,
                    "Las imágenes WebP animadas todavía no están soportadas",
                ));
            }
            Box::new(decoder)
        }
        Some(ImageFormat::Jpeg) => Box::new(reader.into_decoder().map_err(image_error)?),
        _ => {
            return Err(RasteraError::new(
                ErrorKind::Unsupported,
                "Formato no compatible. Abre JPEG, PNG o WebP estáticos",
            ));
        }
    };
    let (width, height) = decoder.dimensions();
    let icc = decoder.icc_profile().map_err(image_error)?;
    if icc
        .as_ref()
        .is_some_and(|bytes| bytes.len() >= 4 * 1024 * 1024)
    {
        return Err(RasteraError::new(
            ErrorKind::TooLarge,
            "El perfil ICC supera el límite de 4 MiB",
        ));
    }
    let info = ImageInfo {
        width,
        height,
        kind: match format {
            Some(ImageFormat::Jpeg) => ImageKind::Jpeg,
            Some(ImageFormat::Png) => ImageKind::Png,
            _ => ImageKind::WebP,
        },
        stamp: version,
        has_color_profile: icc.is_some(),
    };
    info.pixels()?;
    decoder.set_limits(limits).map_err(image_error)?;
    Ok((info, decoder))
}

impl ImageBackend for LocalImages {
    fn inspect(&self, path: &Path) -> Result<ImageInfo> {
        decoder(path).map(|(info, _)| info)
    }

    fn decode(
        &self,
        path: &Path,
        expected: &ImageInfo,
        max_edge: u32,
        cancel: &CancellationToken,
    ) -> Result<DecodedImage> {
        cancel.check()?;
        if max_edge == 0 || max_edge > 4_096 {
            return Err(RasteraError::new(
                ErrorKind::TooLarge,
                "Tamaño de previsualización fuera de rango",
            ));
        }
        let (info, mut decoder) = decoder(path)?;
        if info.stamp != expected.stamp {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El archivo cambió durante la carga",
            ));
        }
        let orientation = decoder.orientation().map_err(image_error)?;
        let profile = crate::color::profile(path, &info, decoder.as_mut())?;
        let mut image = DynamicImage::from_decoder(decoder).map_err(image_error)?;
        cancel.check()?;
        image = crate::color::convert(image, profile.as_ref(), cancel)?;
        image.apply_orientation(orientation);
        if image.width() > max_edge || image.height() > max_edge {
            image = image.thumbnail(max_edge, max_edge);
        }
        cancel.check()?;
        let rgba = image.into_rgba8();
        if stamp(fs::metadata(path).map_err(io_error)?) != expected.stamp {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El archivo cambió durante la carga",
            ));
        }
        Ok(DecodedImage {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
            info,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_supported_formats_and_preserves_png_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let source = image::RgbaImage::from_pixel(20, 10, image::Rgba([40, 90, 160, 128]));
        for (name, format) in [
            ("test.png", ImageFormat::Png),
            ("test.webp", ImageFormat::WebP),
        ] {
            let path = dir.path().join(name);
            source.save_with_format(&path, format).unwrap();
            let info = LocalImages.inspect(&path).unwrap();
            let result = LocalImages
                .decode(&path, &info, 8, &CancellationToken::default())
                .unwrap();
            assert_eq!((result.width, result.height), (8, 4));
            assert_eq!(result.rgba[3], 128);
        }
        let path = dir.path().join("test.jpg");
        image::DynamicImage::ImageRgba8(source)
            .into_rgb8()
            .save(&path)
            .unwrap();
        assert_eq!(LocalImages.inspect(&path).unwrap().kind, ImageKind::Jpeg);
    }

    #[test]
    fn applies_jpeg_exif_orientation_exactly_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oriented.jpg");
        image::RgbImage::from_pixel(3, 2, image::Rgb([90, 80, 70]))
            .save(&path)
            .unwrap();
        let jpeg = fs::read(&path).unwrap();
        let exif: &[u8] = &[
            b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0,
            0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut oriented = jpeg[..2].to_vec();
        oriented.extend([0xff, 0xe1]);
        oriented.extend(((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend(exif);
        oriented.extend(&jpeg[2..]);
        fs::write(&path, oriented).unwrap();
        let info = LocalImages.inspect(&path).unwrap();
        let result = LocalImages
            .decode(&path, &info, 100, &CancellationToken::default())
            .unwrap();
        assert_eq!((result.width, result.height), (2, 3));
    }

    #[test]
    fn refuses_apng_instead_of_silently_showing_its_first_frame() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("animated.png");
        let mut encoder = png::Encoder::new(File::create(&path).unwrap(), 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(2, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255, 0, 0, 255]).unwrap();
        writer.write_image_data(&[0, 255, 0, 255]).unwrap();
        writer.finish().unwrap();
        assert_eq!(
            LocalImages.inspect(&path).unwrap_err().kind,
            ErrorKind::Unsupported
        );
    }

    #[test]
    fn refuses_changed_corrupt_and_cancelled_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        image::RgbaImage::new(2, 2).save(&path).unwrap();
        let info = LocalImages.inspect(&path).unwrap();
        let cancel = CancellationToken::default();
        cancel.cancel();
        assert_eq!(
            LocalImages
                .decode(&path, &info, 8, &cancel)
                .unwrap_err()
                .kind,
            ErrorKind::Cancelled
        );
        image::RgbaImage::new(3, 3).save(&path).unwrap();
        assert_eq!(
            LocalImages
                .decode(&path, &info, 8, &CancellationToken::default())
                .unwrap_err()
                .kind,
            ErrorKind::Changed
        );
        fs::write(&path, b"corrupt").unwrap();
        assert!(LocalImages.inspect(&path).is_err());
    }
}
