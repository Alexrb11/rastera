use crate::images::{image_error, io_error};
use image::{DynamicImage, ImageDecoder};
use moxcms::{
    ColorProfile, DataColorSpace, Layout, ParsingOptions, RenderingIntent, TransferCharacteristics,
    TransformOptions,
};
use rastera_core::{CancellationToken, ErrorKind, ImageInfo, ImageKind, RasteraError, Result};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

fn unsupported(message: impl Into<String>) -> RasteraError {
    RasteraError::new(ErrorKind::Unsupported, message)
}
pub(crate) fn profile(
    path: &Path,
    info: &ImageInfo,
    decoder: &mut dyn ImageDecoder,
) -> Result<Option<ColorProfile>> {
    if info.kind == ImageKind::Jpeg {
        ensure_jpeg_rgb(path)?;
    }
    let color = decoder.color_type();
    if !matches!(
        color,
        image::ColorType::L8
            | image::ColorType::La8
            | image::ColorType::Rgb8
            | image::ColorType::Rgba8
    ) {
        return Err(unsupported("Solo se admiten imágenes SDR de 8 bits"));
    }
    let icc = decoder.icc_profile().map_err(image_error)?;
    if info.kind == ImageKind::Png {
        let png = png::Decoder::new_with_limits(
            BufReader::new(File::open(path).map_err(io_error)?),
            png::Limits {
                bytes: 16 * 1024 * 1024,
            },
        );
        let reader = png
            .read_info()
            .map_err(|e| unsupported(format!("No se pudo leer el color PNG: {e}")))?;
        let header = reader.info();
        if header.coding_independent_code_points.is_some()
            || (icc.is_none()
                && header.srgb.is_none()
                && (header.chrm_chunk.is_some()
                    || header
                        .gama_chunk
                        .is_some_and(|g| (g.into_value() - 0.45455).abs() > 0.0001)))
        {
            return Err(unsupported(
                "Este PNG declara un espacio de color todavía no compatible",
            ));
        }
    }
    let Some(icc) = icc else { return Ok(None) };
    if !matches!(color, image::ColorType::Rgb8 | image::ColorType::Rgba8) {
        return Err(unsupported(
            "Solo se admiten perfiles ICC RGB sobre imágenes RGB de 8 bits",
        ));
    }
    let profile = ColorProfile::new_from_slice_with_options(
        &icc,
        ParsingOptions {
            max_profile_size: 4 * 1024 * 1024,
            max_allowed_clut_size: 4096,
            max_allowed_trc_size: 4096,
        },
    )
    .map_err(|e| unsupported(format!("Perfil ICC inválido o fuera de límites: {e}")))?;
    let (r, g, b) = (
        profile.red_colorant,
        profile.green_colorant,
        profile.blue_colorant,
    );
    let determinant = r.x * (g.y * b.z - g.z * b.y) - g.x * (r.y * b.z - r.z * b.y)
        + b.x * (r.y * g.z - r.z * g.y);
    if !determinant.is_finite()
        || determinant.abs() < 1e-8
        || profile.white_point.y <= 0.0
        || profile.color_space != DataColorSpace::Rgb
        || profile.pcs != DataColorSpace::Xyz
        || profile.red_trc.is_none()
        || profile.green_trc.is_none()
        || profile.blue_trc.is_none()
        || profile.lut_a_to_b_perceptual.is_some()
        || profile.lut_a_to_b_colorimetric.is_some()
        || profile.lut_a_to_b_saturation.is_some()
        || profile.lut_b_to_a_perceptual.is_some()
        || profile.lut_b_to_a_colorimetric.is_some()
        || profile.lut_b_to_a_saturation.is_some()
        || profile
            .cicp
            .as_ref()
            .is_some_and(|c| !matches!(c.transfer_characteristics, TransferCharacteristics::Srgb))
    {
        return Err(unsupported(
            "Solo se admiten perfiles ICC RGB SDR de matriz y curvas, sin LUT",
        ));
    }
    // Validate the transform before any pixels are decoded, including batch planning.
    transform(&profile)?;
    Ok(Some(profile))
}
fn ensure_jpeg_rgb(path: &Path) -> Result<()> {
    let mut reader = BufReader::new(File::open(path).map_err(io_error)?);
    let mut pair = [0; 2];
    reader.read_exact(&mut pair).map_err(io_error)?;
    if pair != [0xff, 0xd8] {
        return Err(unsupported("Cabecera JPEG inválida"));
    }
    for _ in 0..10_000 {
        reader.read_exact(&mut pair[..1]).map_err(io_error)?;
        if pair[0] != 0xff {
            return Err(unsupported("Marcador JPEG inválido"));
        }
        loop {
            reader.read_exact(&mut pair[..1]).map_err(io_error)?;
            if pair[0] != 0xff {
                break;
            }
        }
        let marker = pair[0];
        if matches!(marker, 0xd9 | 0xda | 0x00) {
            return Err(unsupported("JPEG sin cabecera de componentes"));
        }
        if matches!(marker, 0xd8 | 0x01 | 0xd0..=0xd7) {
            continue;
        }
        reader.read_exact(&mut pair).map_err(io_error)?;
        let length = u16::from_be_bytes(pair);
        if length < 2 {
            return Err(unsupported("Segmento JPEG inválido"));
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if length < 8 {
                return Err(unsupported("Cabecera JPEG incompleta"));
            }
            let mut frame = [0; 6];
            reader.read_exact(&mut frame).map_err(io_error)?;
            if frame[0] != 8 || !matches!(frame[5], 1 | 3) {
                return Err(unsupported(
                    "JPEG CMYK, YCCK y profundidades distintas de 8 bits todavía no se admiten",
                ));
            }
            return Ok(());
        }
        reader
            .seek(SeekFrom::Current(i64::from(length) - 2))
            .map_err(io_error)?;
    }
    Err(unsupported("Demasiados segmentos JPEG"))
}
fn transform(profile: &ColorProfile) -> Result<std::sync::Arc<moxcms::Transform8BitExecutor>> {
    profile
        .create_transform_8bit(
            Layout::Rgba,
            &ColorProfile::new_srgb(),
            Layout::Rgba,
            TransformOptions {
                rendering_intent: RenderingIntent::RelativeColorimetric,
                allow_use_cicp_transfer: false,
                ..Default::default()
            },
        )
        .map_err(|e| unsupported(format!("No se pudo convertir el perfil ICC: {e}")))
}
pub(crate) fn convert(
    image: DynamicImage,
    profile: Option<&ColorProfile>,
    cancel: &CancellationToken,
) -> Result<DynamicImage> {
    let Some(profile) = profile else {
        return Ok(image);
    };
    let transform = transform(profile)?;
    let mut rgba = image.into_rgba8();
    // Bounded scratch buffer; alpha is copied explicitly and cancellation is checked between blocks.
    let mut scratch = vec![0; (rgba.as_raw().len()).min(64 * 1024)];
    for block in rgba.as_mut().chunks_mut(64 * 1024) {
        cancel.check()?;
        let output = &mut scratch[..block.len()];
        transform
            .transform(block, output)
            .map_err(|e| unsupported(format!("Falló la conversión ICC: {e}")))?;
        for (src, dst) in block
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0.iter_mut())
        {
            dst[3] = src[3];
        }
        block.copy_from_slice(output);
    }
    Ok(DynamicImage::ImageRgba8(rgba))
}
pub(crate) fn srgb_profile() -> Result<Vec<u8>> {
    ColorProfile::new_srgb()
        .encode()
        .map_err(|e| unsupported(format!("No se pudo crear el perfil sRGB: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CachedThumbnails, LocalBatch, LocalImages};
    use image::ImageEncoder;
    use rastera_core::{BatchBackend, ExportFormat, ExportRecipe, ImageBackend};

    fn reference_p3(pixel: [u8; 3]) -> [u8; 3] {
        // Independent D65 Display P3 -> sRGB matrix and IEC sRGB transfer function.
        let linear = pixel.map(|v| {
            let x = f64::from(v) / 255.0;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        });
        [
            [1.224745, -0.224904, 0.0],
            [-0.042058, 1.042081, 0.0],
            [-0.019642, -0.078655, 1.098537],
        ]
        .map(|row| {
            let x = row
                .iter()
                .zip(linear)
                .map(|(m, x)| m * x)
                .sum::<f64>()
                .clamp(0.0, 1.0);
            let encoded = if x <= 0.0031308 {
                12.92 * x
            } else {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            };
            (encoded * 255.0).round() as u8
        })
    }
    fn png_profile(path: &Path, profile: Vec<u8>, pixel: [u8; 4]) {
        let mut encoder = image::codecs::png::PngEncoder::new(File::create(path).unwrap());
        encoder.set_icc_profile(profile).unwrap();
        encoder
            .write_image(
                &pixel.repeat(32 * 16),
                32,
                16,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
    }
    #[test]
    fn converts_display_p3_before_resizing_and_preserves_alpha_in_viewer_cache_and_exports() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("p3.png");
        let output = dir.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let pixel = [180, 80, 60, 128];
        let expected = reference_p3([pixel[0], pixel[1], pixel[2]]);
        png_profile(
            &source,
            ColorProfile::new_display_p3().encode().unwrap(),
            pixel,
        );
        let info = LocalImages.inspect(&source).unwrap();
        assert!(info.has_color_profile);
        for backend in [
            &LocalImages as &dyn ImageBackend,
            &CachedThumbnails::new(Some(dir.path().join("cache"))),
        ] {
            let image = backend
                .decode(&source, &info, 8, &Default::default())
                .unwrap();
            assert_eq!((image.width, image.height), (8, 4));
            assert_eq!(image.rgba[3], 128);
            for (actual, expected) in image.rgba[..3].iter().zip(expected) {
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "{:?} vs {:?}",
                    &image.rgba[..3],
                    expected
                );
            }
        }
        for format in [ExportFormat::Png, ExportFormat::WebP, ExportFormat::Jpeg] {
            let recipe = ExportRecipe {
                format,
                max_edge: Some(8),
                jpeg_quality: 100,
                ..Default::default()
            };
            let plan = LocalBatch
                .plan(
                    std::slice::from_ref(&source),
                    &output,
                    recipe,
                    &Default::default(),
                )
                .unwrap();
            let mut session = LocalBatch.start(&plan).unwrap();
            session
                .export(0, &plan.items[0], &plan.recipe, &Default::default())
                .unwrap();
            let image = image::open(&plan.items[0].destination)
                .unwrap()
                .into_rgba8();
            let result = image.get_pixel(0, 0).0;
            for (channel, expected) in expected.iter().enumerate() {
                let expected = if format == ExportFormat::Jpeg {
                    ((u32::from(*expected) * 128 + 255 * 127 + 127) / 255) as u8
                } else {
                    *expected
                };
                assert!(
                    result[channel].abs_diff(expected) <= 3,
                    "{format:?}: {result:?} vs {expected}"
                );
            }
            assert_eq!(
                result[3],
                if format == ExportFormat::Jpeg {
                    255
                } else {
                    128
                }
            );
            let (info, mut decoder) = crate::images::decoder(&plan.items[0].destination).unwrap();
            let bytes = decoder.icc_profile().unwrap().unwrap();
            assert_eq!(bytes, srgb_profile().unwrap());
            // All exported formats are readable through the same validated color pipeline.
            let roundtrip = LocalImages
                .decode(&plan.items[0].destination, &info, 8, &Default::default())
                .unwrap();
            assert!(roundtrip.rgba[0].abs_diff(result[0]) <= 1);
        }
    }
    #[test]
    fn reads_icc_in_jpeg_and_webp_and_refuses_hdr_invalid_and_cancelled_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let icc = ColorProfile::new_display_p3().encode().unwrap();
        for format in [ExportFormat::Jpeg, ExportFormat::WebP] {
            let path = dir.path().join(format!("input.{}", format.extension()));
            let file = File::create(&path).unwrap();
            match format {
                ExportFormat::Jpeg => {
                    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 100);
                    encoder.set_icc_profile(icc.clone()).unwrap();
                    encoder
                        .write_image(
                            &[180, 80, 60].repeat(32 * 16),
                            32,
                            16,
                            image::ExtendedColorType::Rgb8,
                        )
                        .unwrap();
                }
                ExportFormat::WebP => {
                    let mut encoder = image::codecs::webp::WebPEncoder::new_lossless(file);
                    encoder.set_icc_profile(icc.clone()).unwrap();
                    encoder
                        .write_image(
                            &[180, 80, 60, 128].repeat(32 * 16),
                            32,
                            16,
                            image::ExtendedColorType::Rgba8,
                        )
                        .unwrap();
                }
                _ => unreachable!(),
            }
            let info = LocalImages.inspect(&path).unwrap();
            let image = LocalImages
                .decode(&path, &info, 8, &Default::default())
                .unwrap();
            for (actual, expected) in image.rgba[..3].iter().zip(reference_p3([180, 80, 60])) {
                assert!(actual.abs_diff(expected) <= 3);
            }
        }
        let path = dir.path().join("bad.png");
        for profile in [
            vec![1; 128],
            ColorProfile::new_display_p3_pq().encode().unwrap(),
        ] {
            png_profile(&path, profile, [180, 80, 60, 128]);
            let info = LocalImages.inspect(&path).unwrap();
            assert_eq!(
                LocalImages
                    .decode(&path, &info, 8, &Default::default())
                    .unwrap_err()
                    .kind,
                ErrorKind::Unsupported
            );
        }
        let cancel = CancellationToken::default();
        cancel.cancel();
        let image = DynamicImage::ImageRgba8(image::RgbaImage::new(32, 16));
        assert_eq!(
            convert(image, Some(&ColorProfile::new_display_p3()), &cancel)
                .unwrap_err()
                .kind,
            ErrorKind::Cancelled
        );
    }
    #[test]
    fn rejects_four_component_jpeg_headers_before_treating_them_as_rgb() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cmyk.jpg");
        // Real SOF structure: four components, even though a codec can expose converted RGB.
        std::fs::write(
            &path,
            [
                0xff, 0xd8, 0xff, 0xc0, 0, 20, 8, 0, 1, 0, 1, 4, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11,
                0, 4, 0x11, 0,
            ],
        )
        .unwrap();
        assert_eq!(
            ensure_jpeg_rgb(&path).unwrap_err().kind,
            ErrorKind::Unsupported
        );
    }
    #[test]
    fn rejects_oversized_and_degenerate_profiles_before_decoding_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.png");
        png_profile(&path, vec![0; 4 * 1024 * 1024], [20, 40, 60, 255]);
        assert_eq!(
            LocalImages.inspect(&path).unwrap_err().kind,
            ErrorKind::TooLarge
        );
        let mut profile = ColorProfile::new_srgb();
        profile.green_colorant = profile.red_colorant;
        png_profile(&path, profile.encode().unwrap(), [20, 40, 60, 255]);
        let info = LocalImages.inspect(&path).unwrap();
        assert_eq!(
            LocalImages
                .decode(&path, &info, 8, &Default::default())
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
    }
}
