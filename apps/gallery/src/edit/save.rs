//! Saving an edit: either as a copy beside the original, or over the original (after
//! keeping a backup). Reads the full-size picture, so run it off the UI thread.

use super::EditState;
use super::pipeline::{self, Kind};
use crate::{cmd, paths};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Copy,
    Replace,
}

impl Mode {
    pub fn from_pref(s: &str) -> Mode {
        if s == "replace" { Mode::Replace } else { Mode::Copy }
    }
}

pub struct Saved {
    pub path: PathBuf,
    /// Where the original was kept, if it was replaced and backed up.
    pub backup: Option<PathBuf>,
}

pub struct Options {
    pub mode: Mode,
    pub quality: u8,
    pub backup: bool,
}

pub fn save(original: &Path, st: &EditState, opts: &Options) -> Result<Saved> {
    let kind = pipeline::kind_of(original).context("only JPG, PNG and WebP pictures can be edited")?;
    let src = pipeline::load(original)?;
    let img = pipeline::render(&src, st, true);
    let exif = pipeline::read_exif(original).and_then(|raw| pipeline::upright_exif(&raw, img.width(), img.height()));
    let bytes = pipeline::encode_with_exif(&img, kind, opts.quality, exif)?;
    write_result(original, &bytes, opts, kind)
}

fn write_result(original: &Path, bytes: &[u8], opts: &Options, _kind: Kind) -> Result<Saved> {
    match opts.mode {
        Mode::Copy => {
            let dest = edited_copy_path(original);
            cmd::atomic_write(&dest, bytes)?;
            Ok(Saved { path: dest, backup: None })
        }
        Mode::Replace => {
            let backup = if opts.backup { Some(backup_original(original)?) } else { None };
            let permissions = std::fs::metadata(original).map(|m| m.permissions()).ok();
            cmd::atomic_write(original, bytes)?;
            if let Some(p) = permissions {
                let _ = std::fs::set_permissions(original, p);
            }
            Ok(Saved { path: original.to_path_buf(), backup })
        }
    }
}

/// `name edited.ext` beside the original, or `name edited 2.ext`, `name edited 3.ext`… if taken.
pub fn edited_copy_path(original: &Path) -> PathBuf {
    let dir = original.parent().unwrap_or(Path::new("."));
    let stem = original.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into());
    let ext = original.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut n = 1;
    loop {
        let name = if n == 1 { format!("{stem} edited{ext}") } else { format!("{stem} edited {n}{ext}") };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Copy the original into the backups folder, named by the time so repeats don't collide.
pub fn backup_original(original: &Path) -> Result<PathBuf> {
    let dir = paths::backup_dir();
    std::fs::create_dir_all(&dir)?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = original.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into());
    let dest = dir.join(format!("{secs}-{name}"));
    std::fs::copy(original, &dest).with_context(|| format!("could not back up {}", original.display()))?;
    Ok(dest)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::edit::Rect;
    use image::{Rgba, RgbaImage};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nyx-photos-save-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path) {
        let img = RgbaImage::from_pixel(40, 20, Rgba([1, 2, 3, 255]));
        std::fs::write(path, pipeline::encode(&img, Kind::Png, 90).unwrap()).unwrap();
    }

    #[test]
    fn copies_get_a_free_name() {
        let dir = scratch("names");
        let a = dir.join("shot.png");
        assert_eq!(edited_copy_path(&a), dir.join("shot edited.png"));
        std::fs::write(dir.join("shot edited.png"), b"x").unwrap();
        assert_eq!(edited_copy_path(&a), dir.join("shot edited 2.png"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_a_copy_leaves_the_original_alone() {
        let dir = scratch("copy");
        let original = dir.join("a.png");
        write_png(&original);
        let before = std::fs::read(&original).unwrap();
        let mut st = EditState::default();
        st.rotate(1);
        st.set_crop(Some(Rect::new(0.0, 0.0, 1.0, 0.5)));
        let saved = save(&original, &st, &Options { mode: Mode::Copy, quality: 90, backup: true }).unwrap();
        assert_eq!(saved.path, dir.join("a edited.png"));
        assert_eq!(std::fs::read(&original).unwrap(), before);
        assert_eq!(image::open(&saved.path).unwrap().to_rgba8().dimensions(), (20, 20));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replacing_writes_over_the_original() {
        let dir = scratch("replace");
        let original = dir.join("a.png");
        write_png(&original);
        let mut st = EditState::default();
        st.rotate(1);
        let saved = save(&original, &st, &Options { mode: Mode::Replace, quality: 90, backup: false }).unwrap();
        assert_eq!(saved.path, original);
        assert!(saved.backup.is_none());
        assert_eq!(image::open(&original).unwrap().to_rgba8().dimensions(), (20, 40));
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(leftovers.len(), 1, "no temporary file left behind");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A 40×20 JPEG as a camera writes it: sideways (Orientation 6), with a date, the
    /// camera, a GPS position and a thumbnail.
    fn write_camera_jpeg(path: &Path) {
        use exif::{Field, In, Rational, Tag, Value};
        let f = |tag, ifd_num, value| Field { tag, ifd_num, value };
        let fields = [
            f(Tag::Make, In::PRIMARY, Value::Ascii(vec![b"Nyxcam".to_vec()])),
            f(Tag::Orientation, In::PRIMARY, Value::Short(vec![6])),
            f(Tag::DateTimeOriginal, In::PRIMARY, Value::Ascii(vec![b"2024:06:01 12:34:56".to_vec()])),
            f(Tag::GPSLatitudeRef, In::PRIMARY, Value::Ascii(vec![b"N".to_vec()])),
            f(Tag::GPSLatitude, In::PRIMARY, Value::Rational(vec![Rational::from((52, 1)), Rational::from((31, 1)), Rational::from((0, 1))])),
            f(Tag::MakerNote, In::PRIMARY, Value::Undefined(b"secret preview".to_vec(), 0)),
            f(Tag::Compression, In::THUMBNAIL, Value::Short(vec![6])),
        ];
        let thumb = pipeline::encode(&RgbaImage::from_pixel(8, 4, Rgba([9, 9, 9, 255])), Kind::Jpeg, 80).unwrap();
        let mut w = exif::experimental::Writer::new();
        for field in &fields {
            w.push_field(field);
        }
        w.set_jpeg(&thumb, In::THUMBNAIL);
        let mut raw = std::io::Cursor::new(Vec::new());
        w.write(&mut raw, false).unwrap();
        let img = RgbaImage::from_pixel(40, 20, Rgba([200, 100, 50, 255]));
        std::fs::write(path, pipeline::encode_with_exif(&img, Kind::Jpeg, 90, Some(raw.into_inner())).unwrap()).unwrap();
    }

    #[test]
    fn edits_keep_the_date_camera_and_place() {
        use exif::{In, Tag, Value};
        let dir = scratch("exif");
        let original = dir.join("camera.jpg");
        write_camera_jpeg(&original);
        let mut st = EditState::default();
        st.set_crop(Some(Rect::new(0.0, 0.0, 1.0, 0.5)));
        let saved = save(&original, &st, &Options { mode: Mode::Copy, quality: 90, backup: false }).unwrap();
        let raw = pipeline::read_exif(&saved.path).expect("the edit keeps EXIF");
        let e = exif::Reader::new().read_raw(raw).unwrap();
        let ascii = |tag| e.get_field(tag, In::PRIMARY).map(|f| f.display_value().to_string());
        assert_eq!(ascii(Tag::DateTimeOriginal).as_deref(), Some("2024-06-01 12:34:56"));
        assert_eq!(ascii(Tag::Make).as_deref(), Some("\"Nyxcam\""));
        assert!(e.get_field(Tag::GPSLatitude, In::PRIMARY).is_some());
        assert_eq!(e.get_field(Tag::Orientation, In::PRIMARY).map(|f| f.value.get_uint(0)), Some(Some(1)));
        assert!(e.get_field(Tag::MakerNote, In::PRIMARY).is_none());
        assert!(e.fields().all(|f| f.ifd_num == In::PRIMARY), "no thumbnail of the uncropped picture");
        // Upright (the original was sideways, 20×40 once turned), then cropped to the top half.
        let (w, h) = image::open(&saved.path).unwrap().to_rgba8().dimensions();
        assert_eq!((w, h), (20, 20));
        assert!(matches!(e.get_field(Tag::PixelXDimension, In::PRIMARY).map(|f| &f.value), Some(Value::Long(v)) if v == &[20]));
        // Opened again, it isn't turned a second time.
        assert_eq!(pipeline::load(&saved.path).unwrap().dimensions(), (20, 20));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn other_formats_are_refused() {
        let dir = scratch("refuse");
        let gif = dir.join("a.gif");
        std::fs::write(&gif, b"GIF89a").unwrap();
        let err = save(&gif, &EditState::default(), &Options { mode: Mode::Copy, quality: 90, backup: false });
        assert!(err.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
