//! The full-window photo viewer: a scrim over the gallery with the picture,
//! zoom and pan, a filmstrip of the photos around it, a slideshow, and the few actions
//! that make sense while looking at one photo. The controls fade out while the
//! pointer rests.

use crate::grid::{self, CellData};
use crate::models::PhotoItem;
use crate::stage::Stage;
use crate::widgets::{self, hbox, label};
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

type Handler = Rc<dyn Fn(Action)>;

/// How long the pointer rests before the controls fade.
const IDLE_AFTER: Duration = Duration::from_millis(2500);
/// Photos either side of the current one in the filmstrip.
pub const STRIP_SIDE: usize = 7;

#[derive(Clone, Copy)]
pub enum Action {
    Close,
    Prev,
    Next,
    Rotate,
    Reveal,
    ToggleFavorite,
    Edit,
    Play,
    Slideshow,
    SaveRotation,
    ResetRotation,
    Wallpaper,
    CopyImage,
    Share,
    /// Show the photo at this position in the gallery's order.
    Jump(usize),
    /// The stand-alone viewer: hand the photo to the gallery.
    OpenGallery,
}

/// Inside the gallery's window, or the stand-alone viewer's.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Gallery,
    Standalone,
}

struct Inner {
    root: gtk::Overlay,
    stage: Stage,
    name: gtk::Label,
    position: gtk::Label,
    play: gtk::Button,
    favorite: gtk::Button,
    slideshow: gtk::Button,
    rotation_note: gtk::Box,
    rotation_text: gtk::Label,
    rotation_save: gtk::Button,
    strip: gtk::Box,
    on_action: RefCell<Option<Handler>>,
    idle_timer: RefCell<Option<glib::SourceId>>,
    slideshow_timer: RefCell<Option<glib::SourceId>>,
    /// Where the pointer last was, to tell real movement from the events a redraw causes.
    pointer: Cell<(f64, f64)>,
    /// Stand-alone: the bar's parts, for the window's header to hold.
    header_parts: RefCell<Option<HeaderParts>>,
}

/// The stand-alone viewer's controls, placed in the window's 2.0 header: the button
/// back to the gallery, the photo's name and place in its folder, and the actions.
pub struct HeaderParts {
    pub start: gtk::Button,
    pub center: gtk::Box,
    pub end: gtk::Box,
}

#[derive(Clone)]
pub struct Lightbox(Rc<Inner>);

impl Lightbox {
    pub fn new(mode: Mode) -> Self {
        let standalone = mode == Mode::Standalone;
        let root = gtk::Overlay::new();
        root.add_css_class("lightbox");
        root.set_visible(false);

        // Top bar: over the picture in the gallery; in the stand-alone viewer, a
        // solid bar above it like the other apps' top bars.
        let top = hbox(if standalone { 4 } else { 8 });
        top.add_css_class(if standalone { "top-bar" } else { "lightbox-bar" });
        top.set_valign(gtk::Align::Start);
        let back = gtk::Button::new();
        if standalone {
            back.set_icon_name("view-grid-symbolic");
            back.add_css_class("bar-button");
            back.set_tooltip_text(Some("Open in the gallery (G)"));
            back.set_valign(gtk::Align::Center);
            back.set_visible(crate::cmd::present("nyx-photos"));
        } else {
            back.set_child(Some(&widgets::button_content("go-previous-symbolic", "Back to gallery")));
            back.set_tooltip_text(Some("Esc"));
        }
        top.append(&back);
        let name = label("", if standalone { "crumb" } else { "lightbox-title" });
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        name.set_margin_start(8);
        let position = label("", "dim mono");
        let titles = hbox(if standalone { 10 } else { 12 });
        titles.set_hexpand(true);
        titles.set_halign(gtk::Align::Start);
        if standalone {
            titles.add_css_class("viewer-title");
            name.set_margin_start(0);
            name.add_css_class("viewer-name");
            position.add_css_class("viewer-position");
        }
        titles.append(&name);
        titles.append(&position);
        top.append(&titles);
        let zoom_label = label("", "dim mono");
        zoom_label.set_valign(gtk::Align::Center);
        let favorite = widgets::icon_button("non-starred-symbolic", "Favorite (F)");
        let edit = widgets::icon_button("document-edit-symbolic", if standalone { "Edit in gallery (E)" } else { "Edit (E)" });
        edit.set_visible(!standalone || crate::cmd::present("nyx-photos"));
        let rotate = widgets::icon_button("object-rotate-right-symbolic", "Rotate 90° (R)");
        let slideshow = widgets::icon_button("media-playback-start-symbolic", "Slideshow (S)");
        let copy = widgets::icon_button("edit-copy-symbolic", "Copy image (Ctrl+C)");
        let share = widgets::icon_button("send-to-symbolic", "Share");
        let wallpaper = widgets::icon_button("preferences-desktop-wallpaper-symbolic", "Set as background (B)");
        wallpaper.set_visible(crate::actions::can_set_background());
        let reveal = widgets::icon_button("folder-open-symbolic", "Reveal in file manager (Ctrl+O)");
        let close = widgets::icon_button("window-close-symbolic", "Close (Esc)");
        // Stand-alone, the header gets these; Esc and the window's own controls close it.
        let end = hbox(if standalone { 4 } else { 8 });
        end.append(&zoom_label);
        for b in [&favorite, &edit, &rotate, &slideshow, &copy, &share, &wallpaper, &reveal] {
            if standalone {
                b.add_css_class("bar-button");
            }
            end.append(b);
        }
        if standalone {
            close.set_visible(false);
        } else {
            top.append(&end);
            top.append(&close);
        }

        let stage = Stage::new(zoom_label.clone());
        let prev = nav_button("go-previous-symbolic", "Previous (←)", gtk::Align::Start);
        let next = nav_button("go-next-symbolic", "Next (→)", gtk::Align::End);
        let overlay = stage.widget();
        overlay.add_overlay(&prev);
        overlay.add_overlay(&next);
        let play = gtk::Button::new();
        play.set_child(Some(&widgets::button_content("media-playback-start-symbolic", "Play video")));
        play.add_css_class("suggested-action");
        play.set_halign(gtk::Align::Center);
        play.set_valign(gtk::Align::Center);
        play.set_visible(false);
        overlay.add_overlay(&play);
        let header_parts = if standalone {
            top.remove(&back);
            top.remove(&titles);
            titles.set_hexpand(false);
            titles.set_halign(gtk::Align::Center);
            overlay.set_vexpand(true);
            root.set_child(Some(overlay));
            Some(HeaderParts { start: back.clone(), center: titles.clone(), end: end.clone() })
        } else {
            None
        };
        if !standalone {
            root.set_child(Some(overlay));
            root.add_overlay(&top);
        }

        // A turned view isn't saved: say so, and offer to.
        let rotation_note = hbox(8);
        rotation_note.add_css_class("rotation-note");
        rotation_note.set_halign(gtk::Align::Center);
        rotation_note.set_valign(gtk::Align::Start);
        rotation_note.set_margin_top(64);
        let rotation_text = label("", "");
        rotation_text.set_valign(gtk::Align::Center);
        let rotation_save = gtk::Button::with_label("Save to file");
        rotation_save.add_css_class("suggested-action");
        rotation_save.set_tooltip_text(Some("Ctrl+S"));
        let rotation_reset = gtk::Button::with_label("Undo");
        rotation_reset.add_css_class("flat");
        rotation_note.append(&rotation_text);
        rotation_note.append(&rotation_reset);
        rotation_note.append(&rotation_save);
        rotation_note.set_visible(false);
        root.add_overlay(&rotation_note);

        let strip = hbox(6);
        strip.add_css_class("filmstrip");
        strip.set_halign(gtk::Align::Center);
        strip.set_valign(gtk::Align::End);
        root.add_overlay(&strip);

        let inner = Rc::new(Inner {
            root,
            stage,
            name,
            position,
            play,
            favorite,
            slideshow,
            rotation_note,
            rotation_text,
            rotation_save,
            strip,
            on_action: RefCell::new(None),
            idle_timer: RefCell::new(None),
            slideshow_timer: RefCell::new(None),
            pointer: Cell::new((-1.0, -1.0)),
            header_parts: RefCell::new(header_parts),
        });
        let lb = Lightbox(inner);
        for (b, a) in [
            (&back, if standalone { Action::OpenGallery } else { Action::Close }),
            (&close, Action::Close),
            (&prev, Action::Prev),
            (&next, Action::Next),
            (&edit, Action::Edit),
            (&rotate, Action::Rotate),
            (&reveal, Action::Reveal),
            (&copy, Action::CopyImage),
            (&share, Action::Share),
            (&wallpaper, Action::Wallpaper),
            (&lb.0.slideshow, Action::Slideshow),
            (&lb.0.favorite, Action::ToggleFavorite),
            (&lb.0.play, Action::Play),
            (&lb.0.rotation_save, Action::SaveRotation),
            (&rotation_reset, Action::ResetRotation),
        ] {
            let me = lb.clone();
            b.connect_clicked(move |_| me.emit(a));
        }

        let motion = gtk::EventControllerMotion::new();
        let me = lb.clone();
        motion.connect_motion(move |_, x, y| {
            let (px, py) = me.0.pointer.replace((x, y));
            if (px - x).abs() + (py - y).abs() > 2.0 {
                me.wake();
            }
        });
        lb.0.root.add_controller(motion);
        lb
    }

    /// Stand-alone: the controls for the window's header (once).
    pub fn take_header_parts(&self) -> Option<HeaderParts> {
        self.0.header_parts.borrow_mut().take()
    }

    pub fn widget(&self) -> &gtk::Overlay {
        &self.0.root
    }

    pub fn is_open(&self) -> bool {
        self.0.root.is_visible()
    }

    pub fn on_action(&self, f: impl Fn(Action) + 'static) {
        *self.0.on_action.borrow_mut() = Some(Rc::new(f));
    }

    fn emit(&self, a: Action) {
        let handler = self.0.on_action.borrow().clone();
        if let Some(h) = handler {
            h(a);
        }
    }

    pub fn hide(&self) {
        self.stop_slideshow();
        self.0.stage.clear();
        self.0.root.set_visible(false);
        if let Some(t) = self.0.idle_timer.borrow_mut().take() {
            t.remove();
        }
        self.0.root.remove_css_class("idle");
        self.0.root.set_cursor(None);
    }

    /// Show `photo` (number `n` of `total`), turned by `rotation` degrees. `strip` is the
    /// photos around it, by their position in the gallery.
    pub fn show(&self, photo: &PhotoItem, rotation: u32, n: usize, total: usize, strip: Vec<(usize, CellData)>) {
        let i = &self.0;
        let opening = !i.root.is_visible();
        i.root.set_visible(true);
        i.name.set_text(&photo.file_name);
        i.position.set_text(&format!("{n} / {total}"));
        self.set_favorite(photo.is_favorite);
        let video = photo.is_video();
        i.play.set_visible(video);
        i.stage.show(photo.preview_path(), (!video).then_some(photo.path.as_str()), rotation);
        self.set_rotation_note(rotation, crate::edit::pipeline::kind_of(std::path::Path::new(&photo.path)).is_some());
        self.fill_strip(n - 1, strip);
        if opening {
            self.wake();
        }
    }

    fn set_rotation_note(&self, rotation: u32, savable: bool) {
        let i = &self.0;
        i.rotation_note.set_visible(rotation != 0);
        i.rotation_text.set_text(&format!("Turned {rotation}° in this view only"));
        i.rotation_save.set_visible(savable);
    }

    fn fill_strip(&self, current: usize, strip: Vec<(usize, CellData)>) {
        let s = &self.0.strip;
        while let Some(c) = s.first_child() {
            s.remove(&c);
        }
        s.set_visible(strip.len() > 1);
        for (pos, data) in strip {
            // An Image keeps to its pixel size; a Picture would grow to the thumbnail's.
            let pic = gtk::Image::new();
            pic.set_pixel_size(56);
            let b = gtk::Button::new();
            b.add_css_class("strip-item");
            if pos == current {
                b.add_css_class("current");
            }
            b.set_child(Some(&pic));
            b.set_focusable(false);
            let me = self.clone();
            b.connect_clicked(move |_| me.emit(Action::Jump(pos)));
            grid::thumbnail_texture(&data, move |tex| pic.set_paintable(Some(&tex)));
            s.append(&b);
        }
    }

    pub fn set_favorite(&self, on: bool) {
        self.0.favorite.set_icon_name(if on { "starred-symbolic" } else { "non-starred-symbolic" });
    }

    /// Show the controls and the pointer, and fade them again after a while.
    fn wake(&self) {
        let i = &self.0;
        i.root.remove_css_class("idle");
        i.root.set_cursor(None);
        if let Some(t) = i.idle_timer.borrow_mut().take() {
            t.remove();
        }
        let me = self.clone();
        let id = glib::timeout_add_local_once(IDLE_AFTER, move || {
            me.0.idle_timer.borrow_mut().take();
            me.0.root.add_css_class("idle");
            me.0.root.set_cursor_from_name(Some("none"));
        });
        *i.idle_timer.borrow_mut() = Some(id);
    }

    pub fn is_playing(&self) -> bool {
        self.0.slideshow_timer.borrow().is_some()
    }

    /// Move on to the next photo every `seconds` until stopped.
    pub fn start_slideshow(&self, seconds: u32) {
        self.stop_slideshow();
        let me = self.clone();
        let id = glib::timeout_add_local(Duration::from_secs(seconds.max(1) as u64), move || {
            me.emit(Action::Next);
            glib::ControlFlow::Continue
        });
        *self.0.slideshow_timer.borrow_mut() = Some(id);
        self.0.slideshow.set_icon_name("media-playback-pause-symbolic");
        self.0.slideshow.set_tooltip_text(Some("Stop the slideshow (S)"));
        self.0.root.add_css_class("idle");
        self.0.root.set_cursor_from_name(Some("none"));
    }

    pub fn stop_slideshow(&self) {
        if let Some(t) = self.0.slideshow_timer.borrow_mut().take() {
            t.remove();
        }
        self.0.slideshow.set_icon_name("media-playback-start-symbolic");
        self.0.slideshow.set_tooltip_text(Some("Slideshow (S)"));
    }

    pub fn zoom_by(&self, factor: f64) {
        self.0.stage.zoom_by(factor);
    }

    pub fn zoom_fit(&self) {
        self.0.stage.zoom_fit();
    }

    pub fn zoom_actual(&self) {
        self.0.stage.zoom_actual();
    }
}

fn nav_button(icon: &str, tip: &str, side: gtk::Align) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.add_css_class("lightbox-nav");
    b.set_tooltip_text(Some(tip));
    b.set_halign(side);
    b.set_valign(gtk::Align::Center);
    b
}
