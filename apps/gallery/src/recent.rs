//! Photos the viewer opened recently (`~/.local/share/nyx-photos/recent.json`).

use crate::{cmd, paths};
use std::path::{Path, PathBuf};

const MAX: usize = 12;

fn file() -> PathBuf {
    paths::data_dir().join("recent.json")
}

fn load() -> Vec<PathBuf> {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str::<Vec<PathBuf>>(&t).ok()).unwrap_or_default()
}

fn save(list: &[PathBuf]) {
    if let Ok(text) = serde_json::to_string_pretty(list) {
        let _ = cmd::atomic_write(&file(), text);
    }
}

/// Newest first, without files that no longer exist.
pub fn list() -> Vec<PathBuf> {
    load().into_iter().filter(|p| p.is_file()).collect()
}

pub fn record(path: &Path) {
    let mut list = load();
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX);
    save(&list);
}

pub fn forget(path: &Path) {
    let mut list = load();
    list.retain(|p| p != path);
    save(&list);
}

pub fn clear() {
    save(&[]);
}
