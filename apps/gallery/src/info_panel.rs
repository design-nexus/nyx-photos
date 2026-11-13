//! Photo info as a panel down the right of the timeline, when Settings has it so
//! ("Photo info shows as"): a preview, the name and when it was taken; a card with its
//! size, the camera and a brightness histogram; where it was taken, with a small drawn
//! map and Open map; its folder with Show in Files, and its albums as chips beside
//! Add to album. The card over the photos (`inspector.rs`) is the other way to show it.

use crate::inspector::{Action, View};
use crate::models::PhotoItem;
use crate::widgets::{hbox, label, vbox};
use gtk::prelude::*;
use gtk::{gdk, pango};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub const WIDTH: i32 = 300;
const BINS: usize = 24;

type Handler = Rc<dyn Fn(Action)>;

/// A card row: an icon, a line and the quieter line under it.
struct Row {
    root: gtk::Box,
    main: gtk::Label,
    sub: gtk::Label,
}

#[derive(Clone)]
pub struct InfoPanel {
    pub root: gtk::Box,
    pages: gtk::Stack,
    preview: gtk::Picture,
    name: gtk::Label,
    taken: gtk::Label,
    size: Rc<Row>,
    camera: Rc<Row>,
    bins: Rc<RefCell<Option<[f64; BINS]>>>,
    histogram: gtk::DrawingArea,
    place_title: gtk::Label,
    place_card: gtk::Box,
    place: gtk::Label,
    map: gtk::LinkButton,
    map_area: gtk::LinkButton,
    folder: gtk::Label,
    chips: gtk::FlowBox,
    album_button: gtk::MenuButton,
    /// Which photo the thumbnail being loaded is for; older loads are dropped.
    loading: Rc<Cell<u64>>,
    on_action: Rc<RefCell<Option<Handler>>>,
}

impl InfoPanel {
    pub fn new() -> Self {
        let root = hbox(0);
        root.add_css_class("info-panel");
        // Its rows expand within it; the panel itself keeps its width.
        root.set_hexpand(false);
        root.append(&nyx_ui::frame::pane_line(gtk::Orientation::Vertical));
        let pages = gtk::Stack::new();
        pages.set_size_request(WIDTH, -1);
        pages.set_hhomogeneous(true);
        root.append(&pages);

        let column = vbox(0);
        let head = vbox(0);
        head.add_css_class("info-panel-head");
        let preview = gtk::Picture::new();
        preview.set_content_fit(gtk::ContentFit::Cover);
        preview.set_can_shrink(true);
        // The picture is an overlay, so its shape doesn't change the frame's height.
        let frame = gtk::Overlay::new();
        frame.add_css_class("info-panel-preview");
        frame.set_overflow(gtk::Overflow::Hidden);
        let space = gtk::Box::new(gtk::Orientation::Vertical, 0);
        space.set_size_request(-1, 150);
        frame.set_child(Some(&space));
        frame.add_overlay(&preview);
        frame.set_cursor_from_name(Some("pointer"));
        frame.set_tooltip_text(Some("Open (Enter)"));
        head.append(&frame);
        let name = label("", "info-panel-name");
        name.set_ellipsize(pango::EllipsizeMode::Middle);
        name.set_max_width_chars(8);
        head.append(&name);
        let taken = label("", "info-panel-date");
        head.append(&taken);
        column.append(&head);

        let body = vbox(0);
        body.add_css_class("info-panel-body");
        let card = vbox(0);
        card.add_css_class("info-panel-card");
        let size = Rc::new(row("image-x-generic-symbolic"));
        let camera = Rc::new(row("camera-photo-symbolic"));
        card.append(&size.root);
        card.append(&camera.root);
        let bins: Rc<RefCell<Option<[f64; BINS]>>> = Rc::default();
        let histogram = histogram(bins.clone());
        let hist_row = vbox(0);
        hist_row.add_css_class("info-panel-row");
        hist_row.add_css_class("histogram-row");
        hist_row.append(&histogram);
        card.append(&hist_row);
        body.append(&card);

        let place_title = label("Where", "info-panel-label");
        body.append(&place_title);
        let place_card = vbox(0);
        place_card.add_css_class("info-panel-card");
        let map_area = gtk::LinkButton::new("https://www.openstreetmap.org");
        map_area.add_css_class("info-map-button");
        map_area.set_overflow(gtk::Overflow::Hidden);
        map_area.set_child(Some(&map()));
        map_area.set_tooltip_text(Some("Open in OpenStreetMap"));
        place_card.append(&map_area);
        let place_row = hbox(10);
        place_row.add_css_class("info-panel-row");
        place_row.append(&row_icon("mark-location-symbolic"));
        let place = label("", "info-panel-key");
        place.set_hexpand(true);
        place.set_ellipsize(pango::EllipsizeMode::End);
        place.set_max_width_chars(8);
        place_row.append(&place);
        let map = gtk::LinkButton::with_label("https://www.openstreetmap.org", "Open map");
        map.add_css_class("info-link");
        map.set_tooltip_text(Some("Open in OpenStreetMap"));
        place_row.append(&map);
        place_card.append(&place_row);
        body.append(&place_card);

        body.append(&label("Folder and albums", "info-panel-label"));
        let folder_card = vbox(0);
        folder_card.add_css_class("info-panel-card");
        let folder_row = hbox(10);
        folder_row.add_css_class("info-panel-row");
        folder_row.append(&row_icon("folder-symbolic"));
        let folder = label("", "info-panel-key");
        folder.set_hexpand(true);
        folder.set_ellipsize(pango::EllipsizeMode::Start);
        folder.set_max_width_chars(8);
        folder_row.append(&folder);
        let reveal = gtk::Button::with_label("Show in Files");
        reveal.add_css_class("info-link");
        reveal.set_tooltip_text(Some("Show in the file manager (Ctrl+O)"));
        folder_row.append(&reveal);
        folder_card.append(&folder_row);
        let chips = gtk::FlowBox::new();
        chips.add_css_class("info-panel-row");
        chips.add_css_class("info-chips");
        chips.set_selection_mode(gtk::SelectionMode::None);
        chips.set_max_children_per_line(8);
        chips.set_column_spacing(6);
        chips.set_row_spacing(6);
        chips.set_homogeneous(false);
        let album_button = gtk::MenuButton::new();
        album_button.set_child(Some(&chip_content("list-add-symbolic", "Add to album")));
        album_button.add_css_class("info-chip");
        album_button.set_tooltip_text(Some("Add to an album, or take it out of one"));
        folder_card.append(&chips);
        body.append(&folder_card);

        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .vexpand(true)
            .child(&body)
            .build();
        column.append(&scroll);
        pages.add_named(&column, Some("photo"));
        let empty = nyx_ui::widgets::empty_state("image-x-generic-symbolic", "No photo picked", "Pick a photo to see its details.", None);
        empty.add_css_class("info-panel-empty");
        pages.add_named(&empty, Some("empty"));

        let panel = InfoPanel {
            root,
            pages,
            preview,
            name,
            taken,
            size,
            camera,
            bins,
            histogram,
            place_title,
            place_card,
            place,
            map,
            map_area,
            folder,
            chips,
            album_button,
            loading: Rc::default(),
            on_action: Rc::default(),
        };
        let click = gtk::GestureClick::new();
        let me = panel.clone();
        click.connect_released(move |_, _, _, _| me.emit(Action::Open));
        frame.add_controller(click);
        let me = panel.clone();
        reveal.connect_clicked(move |_| me.emit(Action::Reveal));
        panel
    }

    pub fn on_action(&self, f: impl Fn(Action) + 'static) {
        *self.on_action.borrow_mut() = Some(Rc::new(f));
    }

    /// The "Add to album" chip; its menu is filled by the window.
    pub fn album_button(&self) -> &gtk::MenuButton {
        &self.album_button
    }

    fn emit(&self, action: Action) {
        let handler = self.on_action.borrow().clone();
        if let Some(h) = handler {
            h(action);
        }
    }

    pub fn update(&self, view: &View) {
        let Some(photo) = &view.photo else {
            self.pages.set_visible_child_name("empty");
            return;
        };
        self.pages.set_visible_child_name("photo");
        self.name.set_text(&photo.file_name);
        self.name.set_tooltip_text(Some(&photo.file_name));
        self.taken.set_text(&taken_line(photo.taken));
        self.load_thumbnail(view.cell.as_ref());

        let mut size = Vec::new();
        if photo.width > 0 {
            size.push(format!("{} × {}", photo.width, photo.height));
            let mp = photo.width as f64 * photo.height as f64 / 1_000_000.0;
            size.push(format!("{mp:.1} MP"));
        }
        if let Some(d) = photo.duration {
            size.push(crate::models::format_duration(d));
        }
        let size = if size.is_empty() { "Unknown size".to_string() } else { size.join(" · ") };
        self.size.main.set_text(&size);
        self.size.sub.set_text(&format!("{} · {}", photo.file_size_formatted, photo.format));

        let (device, exposure) = camera_lines(photo);
        self.camera.root.set_visible(device.is_some() || !exposure.is_empty());
        self.camera.main.set_text(device.as_deref().unwrap_or("Camera"));
        self.camera.sub.set_text(&exposure);
        self.camera.sub.set_visible(!exposure.is_empty());
        self.camera.root.set_tooltip_text(photo.lens_model.as_deref());

        match (photo.latitude, photo.longitude) {
            (Some(lat), Some(lon)) => {
                self.place.set_text(&format!(
                    "{:.2}° {}, {:.2}° {}",
                    lat.abs(),
                    if lat < 0.0 { "S" } else { "N" },
                    lon.abs(),
                    if lon < 0.0 { "W" } else { "E" }
                ));
                let uri = format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=15/{lat:.6}/{lon:.6}");
                self.map.set_uri(&uri);
                self.map_area.set_uri(&uri);
                self.place_title.set_visible(true);
                self.place_card.set_visible(true);
            }
            _ => {
                self.place_title.set_visible(false);
                self.place_card.set_visible(false);
            }
        }

        let folder = std::path::Path::new(photo.folder());
        let pretty = crate::paths::pretty(folder);
        self.folder.set_text(pretty.strip_prefix("~/").unwrap_or(&pretty));
        self.folder.set_tooltip_text(Some(&pretty));

        while let Some(c) = self.chips.first_child() {
            if let Some(child) = c.downcast_ref::<gtk::FlowBoxChild>()
                && child.child().as_ref() == Some(self.album_button.upcast_ref())
            {
                child.set_child(None::<&gtk::Widget>);
            }
            self.chips.remove(&c);
        }
        for (id, name) in &view.albums {
            let chip = gtk::Button::new();
            chip.set_child(Some(&chip_content("nyx-photos-album-symbolic", name)));
            chip.add_css_class("info-chip");
            chip.add_css_class("accent");
            chip.set_tooltip_text(Some(&format!("Show {name}")));
            let (me, id) = (self.clone(), id.clone());
            chip.connect_clicked(move |_| me.emit(Action::OpenAlbum(id.clone())));
            self.chips.insert(&chip, -1);
        }
        self.chips.insert(&self.album_button, -1);
    }

    /// The preview and the histogram, from the thumbnail (made off the UI thread).
    fn load_thumbnail(&self, cell: Option<&crate::grid::CellData>) {
        let mine = self.loading.get() + 1;
        self.loading.set(mine);
        self.preview.set_paintable(None::<&gdk::Paintable>);
        *self.bins.borrow_mut() = None;
        self.histogram.queue_draw();
        let Some(cell) = cell else { return };
        let me = self.clone();
        crate::grid::thumbnail_texture(cell, move |tex| {
            if me.loading.get() != mine {
                return;
            }
            me.preview.set_paintable(Some(&tex));
            *me.bins.borrow_mut() = Some(brightness(&tex));
            me.histogram.queue_draw();
        });
    }
}

/// The camera ("Fujifilm X-T5 · 23 mm") and how it was set ("f/2 · 1/250 s · ISO 200").
fn camera_lines(photo: &PhotoItem) -> (Option<String>, String) {
    let device = match (&photo.camera_make, &photo.camera_model) {
        (Some(m), Some(model)) if model.contains(m.as_str()) => Some(model.clone()),
        (Some(m), Some(model)) => Some(format!("{m} {model}")),
        (None, Some(model)) => Some(model.clone()),
        (Some(m), None) => Some(m.clone()),
        _ => None,
    };
    let device = match (device, &photo.focal_length) {
        (Some(d), Some(f)) => Some(format!("{d} · {f}")),
        (None, Some(f)) => Some(f.clone()),
        (d, None) => d,
    };
    let exposure: Vec<String> =
        [photo.aperture.clone(), photo.shutter_speed.clone(), photo.iso.clone()]
            .into_iter()
            .flatten()
            .collect();
    (device, exposure.join(" · "))
}

fn row_icon(icon: &str) -> gtk::Image {
    let image = gtk::Image::from_icon_name(icon);
    image.add_css_class("info-panel-icon");
    image
}

fn row(icon: &str) -> Row {
    let root = hbox(10);
    root.add_css_class("info-panel-row");
    root.append(&row_icon(icon));
    let text = vbox(1);
    let main = label("", "info-panel-main");
    main.set_ellipsize(pango::EllipsizeMode::End);
    main.set_max_width_chars(8);
    main.set_hexpand(true);
    let sub = label("", "info-panel-sub");
    sub.set_ellipsize(pango::EllipsizeMode::End);
    sub.set_max_width_chars(8);
    text.append(&main);
    text.append(&sub);
    root.append(&text);
    Row { root, main, sub }
}

fn chip_content(icon: &str, text: &str) -> gtk::Box {
    let b = hbox(5);
    b.append(&gtk::Image::from_icon_name(icon));
    let l = gtk::Label::new(Some(text));
    l.set_ellipsize(pango::EllipsizeMode::End);
    l.set_max_width_chars(18);
    b.append(&l);
    b
}

/// The brightness histogram, as bars in the dim text colour.
fn histogram(bins: Rc<RefCell<Option<[f64; BINS]>>>) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.add_css_class("info-histogram");
    area.set_content_height(34);
    area.set_hexpand(true);
    area.set_tooltip_text(Some("Brightness, dark to light"));
    area.set_draw_func(move |area, cr, w, h| {
        let Some(bins) = *bins.borrow() else { return };
        let peak = bins.iter().cloned().fold(0.0, f64::max).max(1e-9);
        let fg = area.color();
        cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, fg.alpha() as f64);
        let gap = 2.0;
        let bar = (w as f64 - gap * (BINS - 1) as f64) / BINS as f64;
        for (i, v) in bins.iter().enumerate() {
            let bh = ((v / peak).sqrt() * h as f64).max(2.0);
            let x = i as f64 * (bar + gap);
            rounded_top(cr, x, h as f64 - bh, bar, bh, 1.5);
            let _ = cr.fill();
        }
    });
    area
}

fn rounded_top(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h);
    cr.move_to(x, y + h);
    cr.line_to(x, y + r);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
    cr.arc(x + w - r, y + r, r, 1.5 * std::f64::consts::PI, 0.0);
    cr.line_to(x + w, y + h);
    cr.close_path();
}

/// How many pixels fall in each brightness band, dark to light.
fn brightness(tex: &gdk::Texture) -> [f64; BINS] {
    let mut downloader = gdk::TextureDownloader::new(tex);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    let (w, h) = (tex.width() as usize, tex.height() as usize);
    let mut counts = [0f64; BINS];
    for row in bytes.chunks(stride).take(h) {
        for px in row[..w * 4].as_chunks::<4>().0 {
            let y = 0.2126 * px[0] as f64 + 0.7152 * px[1] as f64 + 0.0722 * px[2] as f64;
            counts[((y / 256.0) * BINS as f64) as usize] += 1.0;
        }
    }
    counts
}

/// A small drawn map with a pin: land, two roads and water in the theme's colours. It
/// shows that the photo has a place, not the place itself (that needs map tiles).
fn map() -> gtk::Overlay {
    let area = gtk::DrawingArea::new();
    area.add_css_class("info-map");
    area.set_content_height(64);
    area.set_hexpand(true);
    area.set_draw_func(|area, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        let swatch = |name: &str| {
            nyx_theme::swatches().into_iter().find(|(n, _)| n == name).and_then(|(_, c)| gdk::RGBA::parse(c.as_str()).ok())
        };
        let fg = area.color();
        let set = |c: &gdk::RGBA, a: f64| cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, a);
        if let Some(green) = swatch("green") {
            set(&green, 0.09);
            cr.paint().ok();
        }
        if let Some(blue) = swatch("blue") {
            set(&blue, 0.3);
            cr.rectangle(0.0, h * 0.7, w, h * 0.3);
            let _ = cr.fill();
        }
        set(&fg, 0.14);
        cr.set_line_width(4.0);
        cr.move_to(w * 0.5, -2.0);
        cr.line_to(w * 0.36, h + 2.0);
        let _ = cr.stroke();
        set(&fg, 0.1);
        cr.set_line_width(3.0);
        cr.move_to(w * 0.62, -2.0);
        cr.line_to(w * 0.86, h + 2.0);
        let _ = cr.stroke();
    });
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&area));
    let pin = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pin.add_css_class("info-map-pin");
    pin.set_halign(gtk::Align::Center);
    pin.set_valign(gtk::Align::Start);
    pin.set_margin_top(24);
    overlay.add_overlay(&pin);
    overlay
}

/// When it was taken: "Saturday 8 November 2026 · 17:42" (wall-clock seconds).
fn taken_line(taken: i64) -> String {
    const WEEKDAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December",
    ];
    let (y, m, d) = crate::metadata::ymd(taken);
    let seconds = taken.rem_euclid(86400);
    format!(
        "{} {d} {} {y} · {:02}:{:02}",
        WEEKDAYS[taken.div_euclid(86400).rem_euclid(7) as usize],
        MONTHS[(m as usize).clamp(1, 12) - 1],
        seconds / 3600,
        seconds % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_date_is_written_out() {
        assert_eq!(super::taken_line(crate::metadata::naive(2026, 11, 8, 17, 42, 0)), "Sunday 8 November 2026 · 17:42");
    }
}
