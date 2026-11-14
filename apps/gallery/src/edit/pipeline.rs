//! Applying an `EditState` to pixels: loading, the transforms, adjustments, markup
//! and encoding. The same function renders the small live preview and the final file.

use super::marks::{draw_on_image, pixelate};
use super::{EditState, Filter, Mark};
use anyhow::{Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::imageops::{self, FilterType};
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, RgbaImage};
use rayon::prelude::*;
use std::path::Path;

/// The formats that can be edited and saved again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Jpeg,
    Png,
    WebP,
}

pub fn kind_of(path: &Path) -> Option<Kind> {
    match path.extension()?.to_string_lossy().to_lowercase().as_str() {
        "jpg" | "jpeg" => Some(Kind::Jpeg),
        "png" => Some(Kind::Png),
        "webp" => Some(Kind::WebP),
        _ => None,
    }
}

/// Decode a picture and turn it upright by its EXIF orientation.
pub fn load(path: &Path) -> Result<RgbaImage> {
    let reader = ImageReader::open(path).with_context(|| format!("could not open {}", path.display()))?.with_guessed_format()?;
    let mut decoder = reader.into_decoder().context("could not read the picture")?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).context("could not decode the picture")?;
    img.apply_orientation(orientation);
    Ok(img.to_rgba8())
}

/// A copy no more than `max_edge` px on its longer side (the original if it is already smaller).
pub fn downscale(img: &RgbaImage, max_edge: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w.max(h) <= max_edge {
        return img.clone();
    }
    let k = max_edge as f64 / w.max(h) as f64;
    imageops::resize(img, ((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1), FilterType::Triangle)
}

/// Apply `st` to `src`. `export` also applies the output size; the preview leaves it out
/// (it works on a downscaled copy, and the size only matters in the saved file).
pub fn render(src: &RgbaImage, st: &EditState, export: bool) -> RgbaImage {
    let mut img = if st.flip_h { imageops::flip_horizontal(src) } else { src.clone() };
    if st.flip_v {
        imageops::flip_vertical_in_place(&mut img);
    }
    img = match st.rotation % 360 {
        90 => imageops::rotate90(&img),
        180 => imageops::rotate180(&img),
        270 => imageops::rotate270(&img),
        _ => img,
    };
    if let Some(c) = st.crop {
        let (w, h) = img.dimensions();
        let x = ((c.x * w as f64).round() as u32).min(w - 1);
        let y = ((c.y * h as f64).round() as u32).min(h - 1);
        let cw = ((c.w * w as f64).round() as u32).clamp(1, w - x);
        let ch = ((c.h * h as f64).round() as u32).clamp(1, h - y);
        img = imageops::crop_imm(&img, x, y, cw, ch).to_image();
    }
    if export && let Some((w, h)) = st.resize.filter(|&(w, h)| w > 0 && h > 0 && (w, h) != img.dimensions()) {
        img = imageops::resize(&img, w, h, FilterType::Lanczos3);
    }
    adjust(&mut img, st);
    for m in &st.marks {
        if let Mark::Pixelate { rect, size } = m {
            pixelate(&mut img, *rect, *size);
        }
    }
    draw_on_image(&mut img, &st.marks);
    img
}

/// Brightness, contrast, saturation and the filter, in one pass.
fn adjust(img: &mut RgbaImage, st: &EditState) {
    if !st.has_adjustments() {
        return;
    }
    let add = st.brightness as f32 * 2.55;
    let contrast = 1.0 + st.contrast as f32 / 100.0;
    let saturation = 1.0 + st.saturation as f32 / 100.0;
    let filter = st.filter;
    let luma = |v: [f32; 3]| 0.299 * v[0] + 0.587 * v[1] + 0.114 * v[2];
    img.par_chunks_mut(4 * 4096).for_each(|chunk| {
        for p in chunk.as_chunks_mut::<4>().0 {
            let mut v = [p[0] as f32, p[1] as f32, p[2] as f32];
            for c in &mut v {
                *c = (*c - 128.0) * contrast + 128.0 + add;
            }
            let gray = luma(v);
            for c in &mut v {
                *c = gray + (*c - gray) * saturation;
            }
            v = match filter {
                Filter::None => v,
                Filter::Mono => [luma(v); 3],
                Filter::Sepia => [
                    0.393 * v[0] + 0.769 * v[1] + 0.189 * v[2],
                    0.349 * v[0] + 0.686 * v[1] + 0.168 * v[2],
                    0.272 * v[0] + 0.534 * v[1] + 0.131 * v[2],
                ],
            };
            for (dst, c) in p.iter_mut().zip(v) {
                *dst = c.round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

/// Encode for saving. JPEG uses `quality` (1–100); WebP is lossless.
pub fn encode(img: &RgbaImage, kind: Kind, quality: u8) -> Result<Vec<u8>> {
    encode_with_exif(img, kind, quality, None)
}

/// `encode`, carrying `exif` (a TIFF-format EXIF block, see `upright_exif`).
pub fn encode_with_exif(img: &RgbaImage, kind: Kind, quality: u8, exif: Option<Vec<u8>>) -> Result<Vec<u8>> {
    fn write(mut enc: impl ImageEncoder, exif: Option<Vec<u8>>, buf: &[u8], w: u32, h: u32, color: ExtendedColorType) -> Result<()> {
        if let Some(exif) = exif {
            // Every format here takes EXIF; one that didn't would save without it.
            let _ = enc.set_exif_metadata(exif);
        }
        enc.write_image(buf, w, h, color)?;
        Ok(())
    }
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    match kind {
        Kind::Jpeg => {
            let rgb: Vec<u8> = img.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
            write(JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100)), exif, &rgb, w, h, ExtendedColorType::Rgb8)?;
        }
        Kind::Png => write(PngEncoder::new(&mut out), exif, img.as_raw(), w, h, ExtendedColorType::Rgba8)?,
        Kind::WebP => write(WebPEncoder::new_lossless(&mut out), exif, img.as_raw(), w, h, ExtendedColorType::Rgba8)?,
    }
    Ok(out)
}

/// The picture's EXIF block (TIFF format), if it has one.
pub fn read_exif(path: &Path) -> Option<Vec<u8>> {
    let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    reader.into_decoder().ok()?.exif_metadata().ok().flatten()
}

/// The EXIF to save with an edit of a picture: the main image's fields (date taken,
/// camera, GPS, …) with Orientation reset, since the pixels are upright now, and the
/// new size. The embedded thumbnail and maker notes are left out: they can show the
/// picture as it was before a crop.
pub fn upright_exif(raw: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    use exif::{Field, In, Tag, Value};
    let parsed = exif::Reader::new().read_raw(raw.to_vec()).ok()?;
    let replaced = [Tag::Orientation, Tag::MakerNote, Tag::PixelXDimension, Tag::PixelYDimension, Tag::ImageWidth, Tag::ImageLength];
    let mut fields: Vec<Field> =
        parsed.fields().filter(|f| f.ifd_num == In::PRIMARY && !replaced.contains(&f.tag)).cloned().collect();
    let field = |tag, value| Field { tag, ifd_num: In::PRIMARY, value };
    fields.push(field(Tag::Orientation, Value::Short(vec![1])));
    fields.push(field(Tag::PixelXDimension, Value::Long(vec![width])));
    fields.push(field(Tag::PixelYDimension, Value::Long(vec![height])));
    let mut writer = exif::experimental::Writer::new();
    for f in &fields {
        writer.push_field(f);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    writer.write(&mut out, parsed.little_endian()).ok()?;
    Some(out.into_inner())
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::edit::{Colour, Rect};
    use image::Rgba;

    /// 40×20 with a red pixel at the top left and blue at the top right.
    fn sample() -> RgbaImage {
        let mut img = RgbaImage::from_pixel(40, 20, Rgba([100, 100, 100, 255]));
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        img.put_pixel(39, 0, Rgba([0, 0, 255, 255]));
        img
    }

    #[test]
    fn nothing_asked_changes_nothing() {
        let src = sample();
        assert_eq!(render(&src, &EditState::default(), true), src);
    }

    #[test]
    fn rotating_turns_the_picture_clockwise() {
        let mut st = EditState::default();
        st.rotate(1);
        let out = render(&sample(), &st, false);
        assert_eq!(out.dimensions(), (20, 40));
        assert_eq!(out.get_pixel(19, 0).0, [255, 0, 0, 255], "top left goes to top right");
        assert_eq!(out.get_pixel(19, 39).0, [0, 0, 255, 255], "top right goes to bottom right");
    }

    #[test]
    fn flipping_what_is_shown_mirrors_it_even_when_turned() {
        let mut st = EditState::default();
        st.flip_shown_horizontally();
        let out = render(&sample(), &st, false);
        assert_eq!(out.get_pixel(39, 0).0, [255, 0, 0, 255]);
        let mut st = EditState::default();
        st.rotate(1);
        st.flip_shown_horizontally();
        let turned = render(&sample(), &st, false);
        let mut plain = EditState::default();
        plain.rotate(1);
        let expected = imageops::flip_horizontal(&render(&sample(), &plain, false));
        assert_eq!(turned, expected);
    }

    #[test]
    fn cropping_takes_the_asked_part() {
        let mut st = EditState::default();
        st.set_crop(Some(Rect::new(0.5, 0.0, 0.5, 0.5)));
        let out = render(&sample(), &st, false);
        assert_eq!(out.dimensions(), (20, 10));
        assert_eq!(out.get_pixel(19, 0).0, [0, 0, 255, 255]);
    }

    #[test]
    fn resizing_applies_to_the_export_only() {
        let mut st = EditState::default();
        st.resize = Some((10, 5));
        assert_eq!(render(&sample(), &st, true).dimensions(), (10, 5));
        assert_eq!(render(&sample(), &st, false).dimensions(), (40, 20));
    }

    #[test]
    fn adjustments_move_the_pixels_the_right_way() {
        let src = RgbaImage::from_pixel(8, 8, Rgba([100, 120, 140, 255]));
        let mut st = EditState::default();
        st.brightness = 20;
        assert!(render(&src, &st, false).get_pixel(0, 0)[0] > 100);
        st.brightness = 0;
        st.saturation = -100;
        let p = render(&src, &st, false).get_pixel(0, 0).0;
        assert!(p[0] == p[1] && p[1] == p[2]);
        st.saturation = 0;
        st.filter = Filter::Mono;
        let p = render(&src, &st, false).get_pixel(0, 0).0;
        assert!(p[0] == p[1] && p[1] == p[2]);
        st.filter = Filter::Sepia;
        let p = render(&src, &st, false).get_pixel(0, 0).0;
        assert!(p[0] > p[1] && p[1] > p[2], "warm: {p:?}");
        assert_eq!(p[3], 255);
    }

    #[test]
    fn markup_and_blur_land_in_the_rendered_picture() {
        let mut st = EditState::default();
        st.marks.push(Mark::Pixelate { rect: Rect::new(0.0, 0.0, 0.5, 1.0), size: 0.5 });
        st.marks.push(Mark::Rect { rect: Rect::new(0.6, 0.2, 0.3, 0.6), colour: Colour::rgb(0.0, 1.0, 0.0), size: 0.1 });
        let out = render(&sample(), &st, false);
        assert_ne!(out.get_pixel(0, 0).0, [255, 0, 0, 255], "the red pixel is blurred away");
        assert!((0..20).any(|y| (24..36).any(|x| out.get_pixel(x, y).0[1] > 200)), "a green outline");
    }

    #[test]
    fn downscaling_keeps_the_shape() {
        let big = RgbaImage::new(400, 200);
        assert_eq!(downscale(&big, 100).dimensions(), (100, 50));
        assert_eq!(downscale(&big, 1000).dimensions(), (400, 200));
    }

    #[test]
    fn formats_round_trip() {
        let src = sample();
        for kind in [Kind::Png, Kind::WebP] {
            let bytes = encode(&src, kind, 90).unwrap();
            let back = image::load_from_memory(&bytes).unwrap().to_rgba8();
            assert_eq!(back, src, "{kind:?} is lossless");
        }
        let jpeg = encode(&src, Kind::Jpeg, 90).unwrap();
        assert_eq!(image::load_from_memory(&jpeg).unwrap().dimensions_pair(), (40, 20));
    }

    #[test]
    fn loads_a_file_from_disk() {
        let dir = std::env::temp_dir().join(format!("nyx-photos-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.png");
        std::fs::write(&path, encode(&sample(), Kind::Png, 90).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), sample());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn knows_the_editable_formats() {
        assert_eq!(kind_of(Path::new("a/B.JPG")), Some(Kind::Jpeg));
        assert_eq!(kind_of(Path::new("a.webp")), Some(Kind::WebP));
        assert_eq!(kind_of(Path::new("a.gif")), None);
        assert_eq!(kind_of(Path::new("noext")), None);
    }

    trait Pair {
        fn dimensions_pair(&self) -> (u32, u32);
    }
    impl Pair for DynamicImage {
        fn dimensions_pair(&self) -> (u32, u32) {
            (self.width(), self.height())
        }
    }
}
