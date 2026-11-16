//! Things both the gallery and the stand-alone viewer do to a photo file: copy it to the
//! clipboard, move it to the trash, write a rotation into it, make it the background.

use crate::cmd;
use crate::edit::save;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::path::Path;

/// Omarchy's command for setting the desktop background (where there's no theme service).
pub const BACKGROUND_COMMAND: &str = "omarchy-theme-bg-set";

/// Put files on the clipboard. One picture also goes on as an image, upright, for
/// pasting into apps that take pictures. `done` gets a message to show.
pub fn copy(clipboard: &gdk::Clipboard, paths: Vec<String>, as_image: bool, done: impl FnOnce(String) + 'static) {
    if paths.is_empty() {
        return;
    }
    let files: Vec<gio::File> = paths.iter().map(gio::File::for_path).collect();
    let file_list = gdk::ContentProvider::for_value(&gdk::FileList::from_array(&files).to_value());
    if paths.len() > 1 || !as_image {
        clipboard.set_content(Some(&file_list)).ok();
        let n = paths.len();
        done(format!("Copied {n} {}", if n == 1 { "file" } else { "files" }));
        return;
    }
    let path = paths[0].clone();
    let clipboard = clipboard.clone();
    cmd::background(
        move || {
            let pb = gtk::gdk_pixbuf::Pixbuf::from_file(&path).ok()?;
            let pb = pb.apply_embedded_orientation().unwrap_or(pb);
            pb.save_to_bufferv("png", &[]).ok()
        },
        move |png: Option<Vec<u8>>| {
            let mut providers = vec![file_list];
            if let Some(png) = png {
                providers.insert(0, gdk::ContentProvider::for_bytes("image/png", &glib::Bytes::from_owned(png)));
            }
            clipboard.set_content(Some(&gdk::ContentProvider::new_union(&providers))).ok();
            done("Copied the image".into());
        },
    );
}

/// Move files to the trash. Returns the ones that went, and how many couldn't.
pub fn trash(paths: &[String]) -> (Vec<String>, usize) {
    let mut gone = Vec::new();
    let mut failed = 0;
    for p in paths {
        match gio::File::for_path(p).trash(gio::Cancellable::NONE) {
            Ok(()) => gone.push(p.clone()),
            Err(_) => failed += 1,
        }
    }
    (gone, failed)
}

/// The message after trashing.
pub fn trash_message(moved: usize, failed: usize) -> String {
    let plural = |n: usize| if n == 1 { "photo" } else { "photos" };
    match (moved, failed) {
        (n, 0) => format!("Moved {n} {} to the trash", plural(n)),
        (0, f) => format!("Couldn't move {f} {} to the trash", plural(f)),
        (n, f) => format!("Moved {n} {} to the trash. {f} couldn't be moved", plural(n)),
    }
}

/// Write a quarter-turn rotation into the file, off the UI thread, keeping a backup of
/// the original when the settings say so.
pub fn save_rotation(path: &Path, rotation: u32, done: impl FnOnce(anyhow::Result<save::Saved>) + 'static) {
    let p = crate::prefs::get();
    let opts = save::Options { mode: save::Mode::Replace, quality: p.jpeg_quality, backup: p.backup_originals };
    let st = crate::edit::EditState { rotation, ..Default::default() };
    let path = path.to_path_buf();
    cmd::background(move || save::save(&path, &st, &opts), done);
}

/// The message after a rotation was saved.
pub fn rotation_saved_message(saved: &save::Saved) -> &'static str {
    match saved.backup {
        Some(_) => "Saved the rotation. The original is backed up",
        None => "Saved the rotation",
    }
}

/// The Nyx Theme Engine (NyxOS), which keeps the wallpaper with the theme.
const THEME_BUS: &str = "io.github.design_nexus.nyx.Theme";

/// Whether the session bus has the theme service, running or ready to start.
fn theme_service() -> bool {
    thread_local! {
        static HAS: std::cell::OnceCell<bool> = const { std::cell::OnceCell::new() };
    }
    HAS.with(|h| {
        *h.get_or_init(|| {
            let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else { return false };
            let ask = |method: &str| {
                bus.call_sync(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    method,
                    None,
                    Some(glib::VariantTy::new("(as)").unwrap()),
                    gio::DBusCallFlags::NONE,
                    2000,
                    gio::Cancellable::NONE,
                )
                .ok()
                .and_then(|v| v.child_value(0).get::<Vec<String>>())
                .is_some_and(|names| names.iter().any(|n| n == THEME_BUS))
            };
            ask("ListNames") || ask("ListActivatableNames")
        })
    })
}

/// Whether the background can be set here.
pub fn can_set_background() -> bool {
    cmd::present(BACKGROUND_COMMAND) || theme_service()
}

/// Make a picture the desktop background. Returns the message to show.
pub fn set_background(path: &str) -> &'static str {
    // Omarchy keeps the background with its own theme; elsewhere the Theme Engine does.
    if cmd::present(BACKGROUND_COMMAND) {
        cmd::spawn(&[BACKGROUND_COMMAND, path]);
        return "Set as the desktop background";
    }
    if theme_service() {
        let done = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok().and_then(|bus| {
            bus.call_sync(
                Some(THEME_BUS),
                "/io/github/design_nexus/nyx/Theme",
                "io.github.design_nexus.nyx.Theme1",
                "SetWallpaper",
                Some(&(path,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                10_000,
                gio::Cancellable::NONE,
            )
            .map_err(|e| eprintln!("nyx-photos: SetWallpaper: {e}"))
            .ok()
        });
        return if done.is_some() { "Set as the desktop background" } else { "Couldn't set the background" };
    }
    "Setting the background needs the Nyx Theme Engine or Omarchy"
}
