//! The settings page: the app's own window settings, the library, and the keys.

use crate::widgets::{self, hbox, vbox};
use crate::{clamp, cmd, paths, prefs};
use gtk::prelude::*;
use std::rc::Rc;

const MAX_CONTENT_WIDTH: i32 = 920;

pub struct Info {
    pub items: usize,
    pub size: String,
    pub on_rescan: Rc<dyn Fn()>,
    /// Called after the library folders changed in the settings.
    pub on_folders: Rc<dyn Fn()>,
    /// Called after the thumbnail layout changed.
    pub on_layout: Rc<dyn Fn()>,
    /// Called after the photo info's place changed (a card or a panel).
    pub on_info_style: Rc<dyn Fn()>,
}

/// Every shortcut, by where it works. Shown in Settings and with `?`.
pub const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Gallery",
        &[
            ("Move between photos", "←, →, ↑, ↓"),
            ("Select a range", "Shift+←, Shift+→"),
            ("First and last photo", "Home, End"),
            ("A page up or down", "PgUp, PgDn"),
            ("Open in the viewer", "Space"),
            ("Favorite", "F"),
            ("Edit", "E"),
            ("Rotate the view", "R"),
            ("Rename", "F2"),
            ("Move to the trash", "Delete"),
            ("Copy the image", "Ctrl+C"),
            ("Show or hide the photo info", "I"),
            ("Select every photo", "Ctrl+A"),
            ("Bigger or smaller thumbnails", "Ctrl++, Ctrl+−"),
            ("Search", "Ctrl+F"),
            ("Collapse the sidebar", "Ctrl+B"),
            ("Reveal in the file manager", "Ctrl+O"),
            ("Show these shortcuts", "F1"),
        ],
    ),
    (
        "Viewer",
        &[
            ("Previous and next photo", "←, →"),
            ("Zoom in, out, fit, 100%", "+, −, 0, 1"),
            ("Slideshow", "S"),
            ("Save the rotation to the file", "Ctrl+S"),
            ("Set as the background", "W"),
            ("Back to the gallery", "Esc"),
        ],
    ),
    ("Window", &[("Close", "Ctrl+W")]),
];

/// Key caps for a shortcut: `Ctrl+A` joined with "+", `A, B` side by side.
pub fn keys_widget(keys: &str) -> gtk::Box {
    if keys.contains(", ") {
        let b = hbox(4);
        for k in keys.split(", ") {
            b.append(&widgets::key_combo(k));
        }
        b
    } else {
        widgets::key_combo(keys)
    }
}

pub fn build(info: Info) -> (gtk::ScrolledWindow, gtk::Box) {
    let body = vbox(0);
    body.add_css_class("settings-page");
    body.set_hexpand(true);

    let (on_layout, on_info_style) = (info.on_layout.clone(), info.on_info_style.clone());
    body.append(&library_group(info).wrapper);
    body.append(&viewing_group(on_layout, on_info_style).wrapper);
    body.append(&opening_group().wrapper);
    body.append(&editing_group().wrapper);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .child(&clamp::Clamp::new(&body, MAX_CONTENT_WIDTH))
        .build();
    (scroller, body)
}

fn library_group(info: Info) -> widgets::Group {
    let group = widgets::group("Library", "Photos and videos are looked for in these folders and their subfolders.");
    let folders = vbox(0);
    folders.add_css_class("plain-list");
    fill_folders(&folders, info.on_folders.clone());
    group.add(&folders);

    let add = gtk::Button::new();
    add.set_child(Some(&widgets::button_content("list-add-symbolic", "Add folder")));
    let (list, changed) = (folders.clone(), info.on_folders.clone());
    add.connect_clicked(move |_| {
        let (list, changed) = (list.clone(), changed.clone());
        crate::dialog::choose_folder("Add a library folder", Some(&crate::paths::home()), move |path| {
            let text = paths::pretty(&path);
            prefs::update(|p| {
                if p.folders.is_empty() {
                    // Keep the Pictures folder that was in use until now.
                    p.folders.push(paths::pretty(&crate::library::default_root()));
                }
                if !p.folders.contains(&text) {
                    p.folders.push(text.clone());
                }
            });
            fill_folders(&list, changed.clone());
            changed();
        });
    });
    group.add(&widgets::row("Add a folder", "Downloads, a camera card, a synced folder…", Some(add.upcast_ref())));

    let rescan = gtk::Button::new();
    rescan.set_child(Some(&widgets::button_content("view-refresh-symbolic", "Rescan")));
    let on_rescan = info.on_rescan;
    rescan.connect_clicked(move |_| on_rescan());
    let desc = format!("{} items, {}. New and deleted files are noticed by themselves.", info.items, info.size);
    group.add(&widgets::row("Scan the folders again", &desc, Some(rescan.upcast_ref())));
    group
}

/// One row per library folder, each with a remove button (not for the last one).
fn fill_folders(list: &gtk::Box, changed: Rc<dyn Fn()>) {
    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
    let roots = crate::library::configured_roots(&prefs::get().folders);
    let single = roots.len() == 1;
    for root in roots {
        let text = paths::pretty(&root);
        let remove = widgets::icon_button("list-remove-symbolic", "Stop looking in this folder");
        remove.set_sensitive(!single);
        let missing = if root.is_dir() { "" } else { "This folder doesn't exist." };
        let row = widgets::row(&text, missing, Some(remove.upcast_ref()));
        row.add_css_class("mono-title");
        let (l, c) = (list.clone(), changed.clone());
        remove.connect_clicked(move |_| {
            prefs::update(|p| {
                if p.folders.is_empty() {
                    p.folders.push(paths::pretty(&crate::library::default_root()));
                }
                p.folders.retain(|f| *f != text);
            });
            fill_folders(&l, c.clone());
            c();
        });
        list.append(&row);
    }
}

fn viewing_group(on_layout: Rc<dyn Fn()>, on_info_style: Rc<dyn Fn()>) -> widgets::Group {
    let group = widgets::group("Gallery and viewer", "");
    let p = prefs::get();
    let changed = on_layout.clone();
    let (layout, _) = widgets::choice_row(
        "Thumbnail layout",
        "Justified keeps each photo's shape in rows of equal height; squares line up in a grid. Also under the grid button at the bottom right.",
        vec![("justified".into(), "Justified".into()), ("square".into(), "Squares".into())],
        &p.layout,
        move |l| {
            prefs::update(|p| p.layout = l);
            on_layout();
        },
    );
    group.add(&layout);
    let (info, _) = widgets::choice_row(
        "Photo info shows as",
        "Press I or the Info button to show or hide it. A narrow window shows the card.",
        vec![("card".into(), "A card over the photo".into()), ("panel".into(), "A panel on the right".into())],
        &p.info_style,
        move |style| {
            prefs::update(|p| p.info_style = style);
            on_info_style();
        },
    );
    group.add(&info);
    let on_group = changed.clone();
    let (row, _) = widgets::choice_row("Group by", "How dates are grouped in the gallery.",
        vec![("days".into(), "Days".into()), ("months".into(), "Months".into()), ("years".into(), "Years".into()), ("all".into(), "No grouping".into())],
        &p.view_mode, move |id| { prefs::update(|p| p.view_mode = id); on_group(); });
    group.add(&row);
    let (row, _) = widgets::choice_row("Recent covers", "Photos added within this many days.",
        vec![("7".into(), "7 days".into()), ("30".into(), "30 days".into()), ("90".into(), "90 days".into())],
        &p.recent_days.to_string(), move |id| { prefs::update(|p| p.recent_days = id.parse().unwrap_or(30)); changed(); });
    group.add(&row);
    let (slides, _) = widgets::choice_row(
        "Slideshow speed",
        "How long each photo shows for. Press S in the viewer.",
        [2, 4, 6, 10, 20].iter().map(|n| (n.to_string(), format!("{n} seconds"))).collect(),
        &p.slideshow_seconds.to_string(),
        |v| prefs::update(|p| p.slideshow_seconds = v.parse().unwrap_or(4)),
    );
    group.add(&slides);
    group
}

fn opening_group() -> widgets::Group {
    let group = widgets::group("Opening photos", "What a double-click or Enter on a photo opens.");
    let installed = cmd::present("nyx-viewer");
    let desc = if installed {
        "Photo Viewer (nyx-viewer) opens the photo in its own light window, with buttons to bring it back here."
    } else {
        "Photo Viewer (nyx-viewer) isn't installed, so photos open in the built-in viewer."
    };
    let current = if installed { prefs::get().open_with } else { "builtin".into() };
    let (row, _) = widgets::choice_row(
        "Open photos in",
        desc,
        vec![("builtin".into(), "Built-in viewer".into()), ("viewer".into(), "Photo Viewer".into())],
        &current,
        |with| prefs::update(|p| p.open_with = with),
    );
    group.add(&row);
    group
}

fn editing_group() -> widgets::Group {
    let group = widgets::group("Editing", "Rotate, crop, resize, adjust and mark up photos. Press E on a photo.");
    let p = prefs::get();
    let (save, _) = widgets::choice_row(
        "Save button",
        "Save a copy keeps the original as it is. Replace original asks you to click twice.",
        vec![("copy".into(), "Save a copy".into()), ("replace".into(), "Replace original".into())],
        &p.save_mode,
        |mode| prefs::update(|p| p.save_mode = mode),
    );
    group.add(&save);

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 60.0, 100.0, 1.0);
    scale.set_value(p.jpeg_quality as f64);
    scale.set_draw_value(false);
    scale.set_width_request(180);
    let readout = gtk::Label::new(Some(&p.jpeg_quality.to_string()));
    readout.add_css_class("value-readout");
    readout.set_xalign(1.0);
    let r = readout.clone();
    scale.connect_value_changed(move |s| {
        let q = s.value().round() as u8;
        r.set_text(&q.to_string());
        prefs::update(|p| p.jpeg_quality = q);
    });
    let control = hbox(10);
    control.append(&scale);
    control.append(&readout);
    group.add(&widgets::row(
        "JPEG quality",
        "Higher keeps more detail and makes bigger files. PNG and WebP are saved without loss.",
        Some(control.upcast_ref()),
    ));

    let (backup, _) = widgets::switch_row(
        "Back up originals before replacing",
        "Keeps a copy in ~/.local/share/nyx-photos/backups.",
        p.backup_originals,
        |on| prefs::update(|p| p.backup_originals = on),
    );
    group.add(&backup);
    group
}

