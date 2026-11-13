//! Theming: the shared Nyx theme engine, given this app's stylesheet.

pub use nyx_theme::*;

use crate::prefs;

/// Icons the app brings (the icon themes have no screenshot, album, justified-rows or
/// check-in-a-circle icon everywhere), written once to its cache and added to the icon
/// theme. GTK fills every path of a symbolic icon, so lines are drawn as filled outlines.
const ICONS: &[(&str, &str)] = &[
    (
        "nyx-photos-screenshot-symbolic",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="#000"><path d="M3.2 3.2h4.8v1.6h-3.2v3.2h-1.6zM16 3.2h4.8v4.8h-1.6v-3.2h-3.2zM3.2 16h1.6v3.2h3.2v1.6h-4.8zM19.2 16h1.6v4.8h-4.8v-1.6h3.2z"/><path fill-rule="evenodd" d="M7.2 8.2h9.6v7.6h-9.6zM8.8 9.8v4.4h6.4v-4.4z"/></svg>"##,
    ),
    (
        "nyx-photos-album-symbolic",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="#000"><path fill-rule="evenodd" d="M6 6.2h12a3.3 3.3 0 0 1 3.3 3.3v8a3.3 3.3 0 0 1 -3.3 3.3h-12a3.3 3.3 0 0 1 -3.3 -3.3v-8a3.3 3.3 0 0 1 3.3 -3.3zM6 7.8h12a1.7 1.7 0 0 1 1.7 1.7v8a1.7 1.7 0 0 1 -1.7 1.7h-12a1.7 1.7 0 0 1 -1.7 -1.7v-8a1.7 1.7 0 0 1 1.7 -1.7z"/><path d="M6.5 3.2h11a0.8 0.8 0 0 1 0 1.6h-11a0.8 0.8 0 0 1 0 -1.6z"/><path d="M6.4 16.9l4.1-4.1l2.5 2.5l1.5-1.5l3.1 3.1z"/></svg>"##,
    ),
    (
        "nyx-photos-rows-symbolic",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="#000"><rect x="3" y="4" width="8" height="7" rx="1.4"/><rect x="12.5" y="4" width="8.5" height="7" rx="1.4"/><rect x="3" y="13" width="11" height="7" rx="1.4"/><rect x="15.5" y="13" width="5.5" height="7" rx="1.4"/></svg>"##,
    ),
    (
        "nyx-photos-check-circle-symbolic",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="#000"><path fill-rule="evenodd" d="M12 2.7a9.3 9.3 0 1 1 0 18.6a9.3 9.3 0 1 1 0 -18.6zM12 4.3a7.7 7.7 0 1 0 0 15.4a7.7 7.7 0 1 0 0 -15.4z"/><path d="M7.9 12.3l1.1-1.1l2 2l4-4l1.1 1.1l-5.1 5.1z"/></svg>"##,
    ),
];

fn install_icons() {
    let dir = nyx_core::paths::cache_dir().join("icons");
    for (name, svg) in ICONS {
        let file = dir.join(format!("{name}.svg"));
        if std::fs::read_to_string(&file).ok().as_deref() != Some(*svg) {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&file, svg);
        }
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

/// Install the stylesheet and the current palette, and follow Omarchy live.
pub fn install() {
    install_icons();
    nyx_theme::use_design_2();
    nyx_theme::install(Style {
        window_class: "photos-window",
        base_skip: &[
            ".settings-option",
        ],
        css: include_str!("style.css"),
        no_glow_css: "\n.theme-card.selected, .toast { box-shadow: 0 0 0 1px alpha(@theme_accent, 0.6); }\n\
             window.photos-window scale > trough > slider, window.photos-window .fader-scale > trough > slider \
             { box-shadow: 0 0 0 1px alpha(@theme_accent, 0.72); }\n",
        extra_colors: None,
        prefs: || {
            let p = prefs::get();
            ThemePrefs { mode: p.mode, theme: p.theme, glow: p.glow, reduce_motion: p.reduce_motion }
        },
        set_theme: Some(|mode, theme| {
            prefs::update(|p| {
                p.mode = mode;
                p.theme = theme;
            })
        }),
    });
}
