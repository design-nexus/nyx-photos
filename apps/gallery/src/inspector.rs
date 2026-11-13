//! Photo info: a small card over the timeline (I), beside the picked photo. Its name and
//! when it was taken, then the camera, the size, where it was taken, its folder and its
//! albums, each with what you'd do about it (Map, Show in Files, Add to album).

use crate::models::PhotoItem;
use crate::widgets::{hbox, label, vbox};
use gtk::pango;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub const WIDTH: i32 = 300;
pub const WIDTH_NARROW: i32 = 280;

type Handler = Rc<dyn Fn(Action)>;

#[derive(Clone)]
pub enum Action {
    Open,
    Reveal,
    /// Show this album (the info panel's album chips).
    OpenAlbum(String),
}

/// Everything the card and the info panel need to draw themselves.
pub struct View {
    pub photo: Option<PhotoItem>,
    /// The albums the photo is in: (id, name).
    pub albums: Vec<(String, String)>,
    /// The photo's thumbnail, for the panel's preview and histogram.
    pub cell: Option<crate::grid::CellData>,
}

/// One line of the card: an icon, the fact, and its link.
struct Line {
    root: gtk::Box,
    text: gtk::Label,
}

#[derive(Clone)]
pub struct Inspector {
    pub root: gtk::Box,
    name: gtk::Label,
    taken: gtk::Label,
    camera: Rc<Line>,
    size: Rc<Line>,
    place: Rc<Line>,
    map: gtk::LinkButton,
    folder: Rc<Line>,
    albums: Rc<Line>,
    album_button: gtk::MenuButton,
    on_action: Rc<RefCell<Option<Handler>>>,
}

impl Inspector {
    pub fn new() -> Self {
        let root = vbox(0);
        root.add_css_class("info-pop");
        root.set_size_request(WIDTH, -1);
        root.set_halign(gtk::Align::Start);
        root.set_valign(gtk::Align::Start);

        let head = hbox(10);
        head.add_css_class("info-pop-head");
        let icon = gtk::Image::from_icon_name("image-x-generic-symbolic");
        icon.add_css_class("info-pop-icon");
        head.append(&icon);
        let titles = vbox(1);
        let name = label("", "info-pop-name");
        name.set_ellipsize(pango::EllipsizeMode::Middle);
        // Short natural widths keep the card at its own width; the text fills it.
        name.set_max_width_chars(8);
        let taken = label("", "info-pop-date");
        titles.append(&name);
        titles.append(&taken);
        head.append(&titles);
        head.set_cursor_from_name(Some("pointer"));
        head.set_tooltip_text(Some("Open (Enter)"));
        root.append(&head);

        let camera = Rc::new(line("camera-photo-symbolic"));
        let size = Rc::new(line("image-x-generic-symbolic"));
        let place = Rc::new(line("mark-location-symbolic"));
        let map = gtk::LinkButton::with_label("https://www.openstreetmap.org", "Map");
        map.add_css_class("info-link");
        map.set_tooltip_text(Some("Open in OpenStreetMap"));
        place.root.append(&map);
        let folder = Rc::new(line("folder-symbolic"));
        let reveal = gtk::Button::with_label("Show in Files");
        reveal.add_css_class("info-link");
        reveal.set_tooltip_text(Some("Show in the file manager (Ctrl+O)"));
        folder.root.append(&reveal);
        let albums = Rc::new(line("nyx-photos-album-symbolic"));
        let album_button = gtk::MenuButton::new();
        album_button.set_child(Some(&gtk::Label::new(Some("Add to album"))));
        album_button.add_css_class("info-link");
        album_button.set_tooltip_text(Some("Add to an album, or take it out of one"));
        albums.root.append(&album_button);
        for l in [&camera, &size, &place, &folder, &albums] {
            root.append(&l.root);
        }

        let inspector = Inspector {
            root,
            name,
            taken,
            camera,
            size,
            place,
            map,
            folder,
            albums,
            album_button,
            on_action: Rc::default(),
        };
        let click = gtk::GestureClick::new();
        let me = inspector.clone();
        click.connect_released(move |_, _, _, _| me.emit(Action::Open));
        head.add_controller(click);
        let me = inspector.clone();
        reveal.connect_clicked(move |_| me.emit(Action::Reveal));
        inspector
    }

    pub fn on_action(&self, f: impl Fn(Action) + 'static) {
        *self.on_action.borrow_mut() = Some(Rc::new(f));
    }

    /// The "Add to album" link; its menu is filled by the window.
    pub fn album_button(&self) -> &gtk::MenuButton {
        &self.album_button
    }

    fn emit(&self, action: Action) {
        let handler = self.on_action.borrow().clone();
        if let Some(h) = handler {
            h(action);
        }
    }

    pub fn set_narrow(&self, narrow: bool) {
        self.root.set_size_request(if narrow { WIDTH_NARROW } else { WIDTH }, -1);
    }

    pub fn update(&self, view: &View) {
        let Some(photo) = &view.photo else { return };
        self.name.set_text(&photo.file_name);
        self.name.set_tooltip_text(Some(&photo.file_name));
        self.taken.set_text(&taken_line(photo.taken));

        let device = match (&photo.camera_make, &photo.camera_model) {
            (Some(m), Some(model)) if model.contains(m.as_str()) => Some(model.clone()),
            (Some(m), Some(model)) => Some(format!("{m} {model}")),
            (None, Some(model)) => Some(model.clone()),
            (Some(m), None) => Some(m.clone()),
            _ => None,
        };
        let exposure: Vec<String> =
            [photo.focal_length.clone(), photo.aperture.clone(), photo.shutter_speed.clone()].into_iter().flatten().collect();
        let camera: Vec<String> = device.clone().into_iter().chain(exposure).collect();
        self.camera.root.set_visible(!camera.is_empty());
        self.camera.text.set_text(&camera.join(" · "));
        let full: Vec<String> = [device, photo.lens_model.clone(), photo.iso.clone()]
            .into_iter()
            .flatten()
            .collect();
        self.camera.root.set_tooltip_text(Some(&full.join(" · ")));

        let mut size = Vec::new();
        if photo.width > 0 {
            size.push(format!("{} × {}", photo.width, photo.height));
        }
        size.push(photo.file_size_formatted.clone());
        size.push(photo.format.clone());
        if let Some(d) = photo.duration {
            size.push(crate::models::format_duration(d));
        }
        self.size.text.set_text(&size.join(" · "));
        if photo.width > 0 {
            let mp = photo.width as f64 * photo.height as f64 / 1_000_000.0;
            self.size.root.set_tooltip_text(Some(&format!("{mp:.1} megapixels")));
        }

        match (photo.latitude, photo.longitude) {
            (Some(lat), Some(lon)) => {
                self.place.text.set_text(&format!(
                    "{:.2}° {}, {:.2}° {}",
                    lat.abs(),
                    if lat < 0.0 { "S" } else { "N" },
                    lon.abs(),
                    if lon < 0.0 { "W" } else { "E" }
                ));
                self.map.set_uri(&format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=15/{lat:.6}/{lon:.6}"));
                self.place.root.set_visible(true);
            }
            _ => self.place.root.set_visible(false),
        }

        let folder = std::path::Path::new(photo.folder());
        let pretty = crate::paths::pretty(folder);
        self.folder.text.set_text(pretty.strip_prefix("~/").unwrap_or(&pretty));
        self.folder.text.set_tooltip_text(Some(&pretty));

        let names: Vec<&str> = view.albums.iter().map(|(_, n)| n.as_str()).collect();
        self.albums.text.set_text(&if names.is_empty() { "In no album".to_string() } else { format!("In {}", names.join(", ")) });
        self.albums.text.set_tooltip_text((!names.is_empty()).then(|| names.join("\n")).as_deref());
    }
}

fn line(icon: &str) -> Line {
    let root = hbox(9);
    root.add_css_class("info-pop-line");
    let image = gtk::Image::from_icon_name(icon);
    image.add_css_class("info-pop-line-icon");
    root.append(&image);
    let text = label("", "info-pop-text");
    text.set_hexpand(true);
    text.set_ellipsize(pango::EllipsizeMode::End);
    text.set_max_width_chars(8);
    root.append(&text);
    Line { root, text }
}

/// When it was taken: "Sat 8 Nov 2026 · 17:42" (wall-clock seconds).
fn taken_line(taken: i64) -> String {
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    let (y, m, d) = crate::metadata::ymd(taken);
    let seconds = taken.rem_euclid(86400);
    format!(
        "{} {d} {} {y} · {:02}:{:02}",
        WEEKDAYS[taken.div_euclid(86400).rem_euclid(7) as usize],
        crate::metadata::month_abbr(m),
        seconds / 3600,
        seconds % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_date_reads_day_first() {
        assert_eq!(super::taken_line(crate::metadata::naive(2026, 11, 8, 17, 42, 0)), "Sun 8 Nov 2026 · 17:42");
    }
}
