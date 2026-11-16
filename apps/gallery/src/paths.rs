//! Well-known locations. Every path honours the XDG overrides so the whole app
//! can be pointed at a scratch copy of `~/.config` for testing.

pub use nyx_core::paths::*;

use std::path::PathBuf;

pub fn albums_file() -> PathBuf {
    data_dir().join("albums.json")
}

pub fn favorites_file() -> PathBuf {
    data_dir().join("favorites.json")
}

/// Quarter turns the viewer shows photos with.
pub fn rotations_file() -> PathBuf {
    data_dir().join("rotations.json")
}

/// What the last scan found, so the next start shows it at once.
pub fn library_cache() -> PathBuf {
    cache_dir().join("library.json")
}

/// Originals kept before an edit replaced them.
pub fn backup_dir() -> PathBuf {
    data_dir().join("backups")
}

pub fn thumbnail_dir() -> PathBuf {
    cache_dir().join("thumbnails")
}
