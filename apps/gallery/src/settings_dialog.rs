//! Settings as a card over the window: the groups listed on the left, the
//! chosen group's options on the right, and a search across all of them.

use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};

pub use nyx_ui::settings_card::{close, escape, fit, focus_search, is_open};

/// Each settings group's icon and one-line summary in the dialog's list.
const GROUPS: &[(&str, &str, &str)] = &[
    ("Library", "folder-pictures-symbolic", "Folders, rescans"),
    ("Gallery and viewer", "view-grid-symbolic", "Layout, photo info, slideshows"),
    ("Opening photos", "document-open-symbolic", "Built-in viewer or Photo Viewer"),
    ("Editing", "document-edit-symbolic", "Saving edits"),
];

type Shortcut = (&'static [&'static str], &'static str);

/// Every shortcut as key caps, for the card's Keyboard group: `Ctrl+A` becomes
/// two caps; alternatives (`←, →`) share one cap.
fn shortcuts() -> &'static [Shortcut] {
    static ALL: std::sync::OnceLock<Vec<Shortcut>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        crate::settings_view::SHORTCUTS
            .iter()
            .flat_map(|(_, keys)| keys.iter())
            .map(|(what, combo)| {
                let caps: Vec<&'static str> = if combo.contains(", ") {
                    vec![combo.replace(", ", "  ").leak()]
                } else {
                    combo.split('+').collect()
                };
                (&*caps.leak(), *what)
            })
            .collect()
    })
}

fn spec() -> Spec {
    Spec {
        groups: GROUPS,
        search_text: None,
        standard: Some(Standard {
            app: "photos",
            about: About {
                name: "Photos",
                icon: "io.github.design_nexus.nyx.Photos",
                version: env!("CARGO_PKG_VERSION"),
                description: "A fast, keyboard-first photo gallery.",
                licence: "MIT",
                website: "https://github.com/design-nexus/nyx-photos",
                issues: "https://github.com/design-nexus/nyx-photos/issues",
                credits: &[],
            },
            shortcuts: shortcuts(),
            reduce_motion: || crate::prefs::get().reduce_motion,
            set_reduce_motion: |on| {
                crate::prefs::update(|p| p.reduce_motion = on);
                crate::window::apply_motion();
            },
        }),
    }
}

/// Show the card over `overlay`, holding the settings page `page` (whose
/// groups are the children of `body`). It is rebuilt each time it opens.
pub fn open(overlay: &gtk::Overlay, window: &gtk::ApplicationWindow, page: &gtk::ScrolledWindow, body: &gtk::Box) {
    settings_card::open_page(overlay, window, spec(), page.upcast_ref(), body);
}
