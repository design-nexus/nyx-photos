//! This app's own preferences (`~/.config/nyx-photos/settings.toml`).

pub use nyx_theme::ThemeMode;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub mode: ThemeMode,
    pub theme: String,
    pub glow: bool,
    pub reduce_motion: bool,
    /// The sidebar shows only icons, whatever the window width.
    pub sidebar_collapsed: bool,
    pub last_section: String,
    /// How the timeline is grouped: `years`, `months`, `days` or `all`.
    pub view_mode: String,
    /// Thumbnail size, 0.0 (small) to 1.0 (large).
    pub zoom: f64,
    pub inspector_open: bool,
    /// How the photo info shows: `card` (over the photos, beside the one it's about) or
    /// `panel` (down the right of the timeline).
    pub info_style: String,
    /// Where a double-clicked photo opens: `builtin` (the gallery's viewer) or `viewer`
    /// (Photo Viewer, `nyx-viewer`).
    pub open_with: String,
    /// What the editor's Save button does: `copy` or `replace`.
    pub save_mode: String,
    /// JPEG quality when saving an edit (1-100).
    pub jpeg_quality: u8,
    /// Keep a copy of the original before replacing it.
    pub backup_originals: bool,
    /// The library folders (`~/…` allowed). Empty means the Pictures folder.
    pub folders: Vec<String>,
    /// Thumbnail layout: `square` or `justified` (each photo keeps its shape).
    pub layout: String,
    /// Seconds each photo shows for in a slideshow.
    pub slideshow_seconds: u32,
    /// Moved to the 2.0 defaults once (justified thumbnails); after that the choice is
    /// the user's.
    pub design_2: bool,
    pub recent_days: u32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Omarchy,
            theme: "tokyo-night".into(),
            glow: true,
            reduce_motion: false,
            sidebar_collapsed: false,
            last_section: "all_photos".into(),
            view_mode: "days".into(),
            zoom: 0.4,
            inspector_open: false,
            info_style: "card".into(),
            open_with: "builtin".into(),
            save_mode: "copy".into(),
            jpeg_quality: 92,
            backup_originals: true,
            folders: Vec::new(),
            layout: "justified".into(),
            slideshow_seconds: 4,
            design_2: true,
            recent_days: 30,
        }
    }
}

nyx_core::prefs_store!(Prefs);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_in_the_built_in_viewer_unless_told_otherwise() {
        assert_eq!(Prefs::default().open_with, "builtin");
        let old: Prefs = toml::from_str("zoom = 0.5").unwrap();
        assert_eq!(old.open_with, "builtin");
        let set: Prefs = toml::from_str("open_with = \"viewer\"").unwrap();
        assert_eq!(set.open_with, "viewer");
    }

    #[test]
    fn settings_from_before_2_0_are_moved_once() {
        let old: Prefs = toml::from_str("layout = \"square\"\ndesign_2 = false").unwrap();
        assert!(!old.design_2);
        assert_eq!(Prefs::default().layout, "justified");
        assert!(Prefs::default().design_2);
    }
}
