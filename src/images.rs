use crate::{output::Output, Error, Result};
use image::{imageops::FilterType, DynamicImage, ImageFormat, ImageReader};
use std::{fs::File, io::BufWriter, path::Path};

pub const FORMATS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "ico", "tif", "tiff", "pnm", "ppm", "pgm", "pbm",
    "pam", "tga", "qoi",
];

#[derive(Debug, Clone)]
pub struct Options {
    pub quality: u8,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub background: [u8; 3],
    pub overwrite: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            quality: 90,
            width: None,
            height: None,
            background: [255; 3],
            overwrite: false,
        }
    }
}

pub fn convert(input: &Path, output: &Path, options: &Options) -> Result<()> {
    if !(1..=100).contains(&options.quality)
        || options.width == Some(0)
        || options.height == Some(0)
    {
        return Err(Error::Invalid(
            "Quality must be 1..100 and dimensions must be positive".into(),
        ));
    }
    if !FORMATS.contains(&crate::extension(output)?.as_str()) {
        return Err(Error::Invalid("Unsupported image output format".into()));
    }
    let format = ImageFormat::from_path(output)?;
    let mut reader = ImageReader::open(input)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = image::ImageDecoder::orientation(&mut decoder)?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    if options.width.is_some() || options.height.is_some() {
        let w = options.width.unwrap_or(u32::MAX);
        let h = options.height.unwrap_or(u32::MAX);
        if options.width.is_some_and(|v| v > 32768) || options.height.is_some_and(|v| v > 32768) {
            return Err(Error::Invalid("Maximum resize dimension is 32768".into()));
        }
        img = img.resize(
            w.min(img.width()),
            h.min(img.height()),
            FilterType::Lanczos3,
        );
    }
    if format == ImageFormat::Ico && (img.width() > 256 || img.height() > 256) {
        img = img.resize(256, 256, FilterType::Lanczos3);
    }
    if matches!(format, ImageFormat::Jpeg | ImageFormat::Pnm) {
        img = flatten(&img, options.background);
        if matches!(crate::extension(output)?.as_str(), "pgm" | "pbm") {
            img = img.grayscale();
        }
    }
    let pending = Output::new(input, output, options.overwrite)?;
    {
        let mut writer = BufWriter::new(File::create(pending.path())?);
        if format == ImageFormat::Jpeg {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, options.quality)
                .encode_image(&img)?;
        } else if format == ImageFormat::Pnm {
            // The path selects the PNM subtype (PBM/PGM/PPM/PAM).
            img.save(pending.path())?;
        } else {
            img.write_to(&mut writer, format)?;
        }
        std::io::Write::flush(&mut writer)?;
    }
    pending.commit()
}

fn flatten(img: &DynamicImage, background: [u8; 3]) -> DynamicImage {
    let rgba = img.to_rgba8();
    DynamicImage::ImageRgb8(image::RgbImage::from_fn(
        rgba.width(),
        rgba.height(),
        |x, y| {
            let p = rgba.get_pixel(x, y).0;
            let a = u32::from(p[3]);
            image::Rgb(std::array::from_fn(|i| {
                ((u32::from(p[i]) * a + u32::from(background[i]) * (255 - a) + 127) / 255) as u8
            }))
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transparent_pixels_use_background() {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([0, 0, 0, 0]),
        ));
        assert_eq!(
            flatten(&img, [255, 128, 12]).to_rgb8().get_pixel(0, 0).0,
            [255, 128, 12]
        );
    }
}
