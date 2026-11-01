//! The main window: the header (the app and where you are, the sidebar toggle,
//! what to do with the selection, search, info, the menu and settings), the
//! sidebar, the gallery with the info panel docked beside it, a quiet foot (counts,
//! grouping, thumbnail size), the viewer and toasts. Settings opens as a card over the window (see
//! `settings_dialog`).

use crate::editor::{Editor, Outcome};
use crate::grid::{CellData, Gallery};
use crate::inspector::{self, Inspector};
use crate::lightbox::{self, Lightbox};
use crate::state::{LIBRARY_SECTIONS, MEDIA_SECTIONS, State};
use crate::widgets::{self, hbox, label, vbox};
use crate::{cmd, dialog, library, paths, prefs, settings_dialog, settings_view, theme};
use gtk::prelude::*;
use nyx_ui::chrome;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const APP_NAME: &str = "Photos";
const VIEWER: &str = "nyx-viewer";
/// How long after the last change in a watched folder it is scanned again.
const RESCAN_AFTER: Duration = Duration::from_millis(1500);
/// How long a toast with an Undo button stays.
const UNDO_TOAST: Duration = Duration::from_millis(8000);

/// What a context-menu item does when clicked.
type MenuAction = Box<dyn Fn(&Rc<App>)>;
const SETTINGS_PAGE: &str = "settings";
/// Below this the sidebar always shows only icons.
const NARROW_BELOW: i32 = 980;

pub struct App {
    window: gtk::ApplicationWindow,
    overlay: gtk::Overlay,
    state: RefCell<State>,
    gallery: Gallery,
    inspector: Inspector,
    lightbox: Lightbox,
    editor: Editor,
    pages: gtk::Stack,
    gallery_page: gtk::Box,
    page: RefCell<String>,
    // Header
    header: chrome::AppHeader,
    /// "3 selected" and its actions, in the header's middle.
    selection_pill: gtk::Box,
    header_actions: gtk::Box,
    info_button: gtk::Button,
    /// The thumbnail-size slider: inline when there is room, in a popover when narrow.
    zoom_inline: gtk::Box,
    zoom_menu: gtk::MenuButton,
    /// The inline size slider; the popover's follows it.
    zoom_scale: gtk::Scale,
    deselect: gtk::Button,
    /// "3 selected", beside Deselect.
    selection_chip: gtk::Label,
    /// Favorite, Edit, Rotate, Trash and the album menu: what to do with the selection.
    quick_actions: gtk::Box,
    fav_button: gtk::Button,
    edit_button: gtk::Button,
    rotate_button: gtk::Button,
    trash_button: gtk::Button,
    album_menu: gtk::MenuButton,
    more_menu: gtk::MenuButton,
    // Status bar
    status_items: gtk::Label,
    status_selection: gtk::Label,
    // Sidebar
    nav: gtk::Box,
    nav_items: RefCell<HashMap<String, gtk::Button>>,
    albums_box: gtk::Box,
    search: gtk::SearchEntry,
    collapse_button: gtk::Button,
    compact_items: RefCell<Vec<gtk::Widget>>,
    counts: RefCell<HashMap<String, gtk::Label>>,
    compact: Cell<bool>,
    narrow: Cell<bool>,
    // Misc
    toast: RefCell<Option<gtk::Revealer>>,
    scan_depth: Cell<usize>,
    /// A folder opened from the command line that isn't in the library: shown instead
    /// of the library until the library is asked for again.
    extra_root: RefCell<Option<PathBuf>>,
    scanning: Cell<bool>,
    /// Another scan was asked for while one ran.
    rescan_again: Cell<bool>,
    /// Watching the library folders for new, changed and deleted files.
    monitors: RefCell<Vec<gio::FileMonitor>>,
    rescan_timer: RefCell<Option<glib::SourceId>>,
    finding_duplicates: Cell<bool>,
    /// When the saved favourites, albums and rotations were last read.
    saved_stamp: RefCell<Vec<Option<std::time::SystemTime>>>,
    /// Open the editor on the file named on the command line, once it has been found.
    edit_when_opened: Cell<bool>,
    /// A file to open in the viewer once the next scan has found it.
    open_after_scan: RefCell<Option<PathBuf>>,
}

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
}

fn app() -> Option<Rc<App>> {
    APP.with(|a| a.borrow().clone())
}

/// Read the photo library (the control interface uses it), once the window is up.
pub fn with_library<R>(f: impl FnOnce(&library::Library) -> R) -> Option<R> {
    app().map(|a| f(&a.state.borrow().lib))
}

/// Show a photo in the viewer, as if it had been opened from the command line.
pub fn open_photo(path: PathBuf) -> bool {
    match app() {
        Some(a) => {
            a.open_path(path);
            true
        }
        None => false,
    }
}

pub fn window() -> Option<gtk::ApplicationWindow> {
    app().map(|a| a.window.clone())
}

/// Called when Reduce motion changes: the toolkit follows the setting by itself, but
/// the page crossfade is ours.
pub fn apply_motion() {
    if let Some(a) = app() {
        a.pages.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 160 });
    }
}

pub fn present(application: &gtk::Application, section: Option<&str>, open: Option<PathBuf>, edit: bool) {
    if let Some(a) = app() {
        if let Some(s) = section {
            a.navigate(s);
        }
        if let Some(path) = open {
            a.edit_when_opened.set(edit);
            a.open_path(path);
        }
        a.window.present();
        return;
    }
    theme::install();
    // Settings saved before 2.0 move to its defaults once: thumbnails keep their shape.
    if !prefs::get().design_2 {
        prefs::update(|p| {
            p.layout = "justified".into();
            p.design_2 = true;
        });
    }
    let roots = library::configured_roots(&prefs::get().folders);
    // A file outside the library: show its folder instead.
    let extra = open.as_ref().filter(|f| !roots.iter().any(|r| f.starts_with(r))).and_then(|f| f.parent().map(PathBuf::from));
    let a = App::build(application, roots);
    if let Some(dir) = extra {
        a.scan_depth.set(1);
        a.state.borrow_mut().lib.roots = vec![dir.clone()];
        *a.extra_root.borrow_mut() = Some(dir);
    } else {
        a.load_cached_library();
    }
    APP.with(|slot| *slot.borrow_mut() = Some(a.clone()));
    let p = prefs::get();
    let start = section.map(String::from).unwrap_or_else(|| nyx_theme::start_page("all_photos", &p.last_section));
    a.navigate(&start);
    a.window.present();
    a.edit_when_opened.set(edit && open.is_some());
    match open {
        // Already known from the last scan: open it now.
        Some(path) if a.state.borrow().lib.photos.iter().any(|p| std::path::Path::new(&p.path) == path) => a.open_path(path),
        other => *a.open_after_scan.borrow_mut() = other,
    }
    a.rescan(true);
}

impl App {
    fn build(application: &gtk::Application, roots: Vec<PathBuf>) -> Rc<App> {
        let p = prefs::get();
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title(APP_NAME)
            .default_width(1120)
            .default_height(800)
            .build();
        window.add_css_class("photos-window");
        // No client-side titlebar: Hyprland manages the window.
        window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
        nyx_ui::keys::install(&window, nyx_ui::keys::Common::default());
        window.set_icon_name(Some("io.github.design_nexus.nyx.Photos"));

        let mut state = State::new(library::Library::load(roots), p.view_mode.clone());
        state.recompute();

        let gallery = Gallery::new();
        gallery.set_zoom(p.zoom);
        gallery.set_justified(p.layout == "justified");
        let inspector = Inspector::new();
        let lightbox = Lightbox::new(lightbox::Mode::Gallery);
        let editor = Editor::new(
            |message| {
                if let Some(a) = app() {
                    a.show_toast(message);
                }
            },
            |outcome| {
                if let Some(a) = app() {
                    a.on_editor_outcome(outcome);
                }
            },
        );

        // ----- Sidebar -----
        let nav = vbox(0);
        nav.add_css_class("settings-navigation");
        nav.set_hexpand(false);
        // The sidebar's toggle and search live in the header.
        let collapse_button = nyx_ui::widgets::bar_button("sidebar-show-symbolic", "Collapse the sidebar (Ctrl+B)");
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search photos  (Ctrl+F)"));
        search.add_css_class("bar-search");
        search.set_width_chars(22);
        search.set_valign(gtk::Align::Center);

        let list = vbox(0);
        let albums_box = vbox(0);
        let nav_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            // Scrolls with wheel, trackpad and keyboard; no visible scrollbar.
            .vscrollbar_policy(gtk::PolicyType::External)
            .vexpand(true)
            .child(&list)
            .build();
        nav.append(&nav_scroll);

        // ----- Gallery page -----
        let gallery_page = vbox(0);
        gallery_page.set_hexpand(true);
        let header_actions = hbox(2);
        let deselect = nyx_ui::widgets::bar_button("window-close-symbolic", "Deselect (Esc)");
        // The info panel is docked beside the grid, on the same surface.
        let gallery_row = hbox(0);
        gallery_row.set_vexpand(true);
        gallery.widget().set_hexpand(true);
        gallery_row.append(gallery.widget());
        inspector.root.set_hexpand(false);
        gallery_row.append(&inspector.root);
        gallery_page.append(&gallery_row);

        // The foot, quiet text without a bar: what's shown on the left; how it's grouped
        // and how big the thumbnails are on the right (filled in by `build_toolbar`).
        let status = hbox(14);
        status.add_css_class("photos-foot");
        let status_items = label("", "foot-text");
        status_items.set_ellipsize(gtk::pango::EllipsizeMode::End);
        status_items.set_xalign(0.0);
        let status_selection = label("", "foot-text");
        status.append(&status_items);
        status.append(&status_selection);
        let spacer = hbox(0);
        spacer.set_hexpand(true);
        status.append(&spacer);
        gallery_page.append(&status);

        // ----- Pages and the info drawer -----
        let pages = gtk::Stack::new();
        pages.add_css_class("settings-content");
        pages.set_hexpand(true);
        pages.set_transition_type(gtk::StackTransitionType::Crossfade);
        pages.set_transition_duration(if p.reduce_motion { 0 } else { 160 });
        // The bar and status bar must not set the window's minimum width, or a half-screen
        // window could never get narrow enough to switch to the compact layout.
        let gallery_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&gallery_page)
            .build();
        pages.add_named(&gallery_scroll, Some("gallery"));

        let column = vbox(0);
        column.append(&pages);
        let content = gtk::Overlay::new();
        content.set_child(Some(&column));
        content.set_hexpand(true);
        inspector.root.set_halign(gtk::Align::End);
        inspector.root.set_valign(gtk::Align::Fill);

        let body = hbox(0);
        body.set_vexpand(true);
        body.append(&nav);
        body.append(&content);

        // The header: the app and the section, the sidebar toggle; the selection in the
        // middle; then album actions, search, info, the menu and Settings.
        let header = chrome::app_header(&window, "image-x-generic-symbolic", "Photos");
        header.start.append(&collapse_button);
        let selection_pill = hbox(2);
        selection_pill.add_css_class("selection-pill");
        header.center.append(&selection_pill);
        header.end.set_spacing(4);
        header.end.append(&header_actions);
        header.end.append(&search);
        let info_button = nyx_ui::widgets::bar_button("dialog-information-symbolic", "Photo info (I)");
        header.end.append(&info_button);
        let more_menu = gtk::MenuButton::new();
        more_menu.add_css_class("bar-button");
        header.end.append(&more_menu);
        let gear = chrome::settings_button(|| {});
        header.end.append(&gear);
        let frame = vbox(0);
        frame.add_css_class("window-frame");
        frame.append(&header.root);
        frame.append(&body);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&frame));
        overlay.add_overlay(lightbox.widget());
        overlay.add_overlay(editor.widget());
        window.set_child(Some(&overlay));

        let app = Rc::new(App {
            window,
            overlay,
            state: RefCell::new(state),
            gallery,
            inspector,
            lightbox,
            editor,
            pages,
            gallery_page,
            page: RefCell::new(String::new()),
            header,
            selection_pill,
            header_actions,
            info_button,
            zoom_inline: hbox(6),
            zoom_menu: gtk::MenuButton::new(),
            zoom_scale: gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.02),
            deselect,
            selection_chip: label("", "selection-chip"),
            quick_actions: hbox(4),
            fav_button: widgets::icon_button("non-starred-symbolic", "Favorite (F)"),
            edit_button: widgets::icon_button("document-edit-symbolic", "Edit (E)"),
            rotate_button: widgets::icon_button("object-rotate-right-symbolic", "Rotate the view 90° (R)"),
            trash_button: widgets::icon_button("user-trash-symbolic", "Move to trash (Delete)"),
            album_menu: gtk::MenuButton::new(),
            more_menu,
            status_items,
            status_selection,
            nav,
            nav_items: RefCell::default(),
            albums_box,
            search,
            collapse_button,
            compact_items: RefCell::default(),
            counts: RefCell::default(),
            compact: Cell::new(false),
            narrow: Cell::new(false),
            toast: RefCell::new(None),
            scan_depth: Cell::new(8),
            extra_root: RefCell::new(None),
            scanning: Cell::new(false),
            rescan_again: Cell::new(false),
            monitors: RefCell::default(),
            rescan_timer: RefCell::new(None),
            finding_duplicates: Cell::new(false),
            saved_stamp: RefCell::new(library::saved_stamp()),
            edit_when_opened: Cell::new(false),
            open_after_scan: RefCell::new(None),
        });
        app.build_toolbar(&status);
        app.build_nav(&list);
        app.wire(&nav_scroll);
        let a = app.clone();
        gear.connect_clicked(move |_| a.show_settings());
        app
    }

    // ----- Building -----

    /// The selection's actions in the header's pill, and the foot's grouping menu and
    /// size slider (`foot`).
    fn build_toolbar(self: &Rc<Self>, foot: &gtk::Box) {
        let p = prefs::get();
        // The grid button in the foot: how the timeline is grouped, and whether photos
        // keep their shape. Built each time it opens, so it shows what Settings changed.
        let group_menu = gtk::MenuButton::new();
        group_menu.set_icon_name("view-grid-symbolic");
        group_menu.set_tooltip_text(Some("Grouping and layout"));
        group_menu.add_css_class("foot-button");
        group_menu.set_valign(gtk::Align::Center);
        let a = self.clone();
        group_menu.set_create_popup_func(move |button| {
            let p = prefs::get();
            let b = a.clone();
            let mode = widgets::segmented(
                &[("years", "Years"), ("months", "Months"), ("days", "Days"), ("all", "All")],
                &p.view_mode,
                move |m| {
                    prefs::update(|p| p.view_mode = m.clone());
                    b.state.borrow_mut().view_mode = m;
                    b.refresh(true);
                },
            );
            let b = a.clone();
            let layout = widgets::segmented(&[("justified", "Justified"), ("square", "Squares")], &p.layout, move |l| {
                prefs::update(|p| p.layout = l.clone());
                b.gallery.set_justified(l == "justified");
            });
            let grid_menu = vbox(8);
            grid_menu.add_css_class("grid-menu");
            grid_menu.append(&label("GROUP BY", "menu-heading"));
            grid_menu.append(&mode);
            grid_menu.append(&label("LAYOUT", "menu-heading"));
            grid_menu.append(&layout);
            let pop = gtk::Popover::new();
            pop.set_child(Some(&grid_menu));
            button.set_popover(Some(&pop));
        });

        // What to do with the selection: how many, then the actions, in the header.
        let pill = &self.selection_pill;
        self.selection_chip.set_visible(false);
        self.selection_chip.set_valign(gtk::Align::Center);
        pill.append(&self.selection_chip);
        self.deselect.set_visible(false);
        let a = self.clone();
        self.deselect.connect_clicked(move |_| {
            a.state.borrow_mut().clear_selection();
            a.refresh_selection(false);
        });

        let q = &self.quick_actions;
        q.add_css_class("quick-actions");
        q.set_valign(gtk::Align::Center);
        for (b, run) in [
            (&self.fav_button, (|a: &Rc<App>| a.toggle_favorite()) as fn(&Rc<App>)),
            (&self.edit_button, |a| a.open_editor()),
            (&self.rotate_button, |a| a.rotate()),
            (&self.trash_button, |a| a.delete_selected()),
        ] {
            let a = self.clone();
            b.connect_clicked(move |_| run(&a));
            b.add_css_class("pill-button");
            q.append(b);
        }
        self.trash_button.add_css_class("danger");
        let album = &self.album_menu;
        album.set_icon_name("folder-new-symbolic");
        album.set_tooltip_text(Some("Add to an album"));
        album.add_css_class("pill-button");
        album.set_valign(gtk::Align::Center);
        let a = self.clone();
        album.set_create_popup_func(move |mb| a.fill_album_menu(mb));
        q.insert_child_after(album, Some(&self.fav_button));
        pill.append(q);
        pill.append(&self.deselect);

        let more = &self.more_menu;
        more.set_icon_name("view-more-symbolic");
        more.set_tooltip_text(Some("More (F10)"));
        more.set_valign(gtk::Align::Center);
        let a = self.clone();
        more.set_create_popup_func(move |mb| a.fill_more_menu(mb));

        let apply_zoom = |v: f64| {
            prefs::update(|p| p.zoom = v);
            if let Some(a) = app() {
                a.gallery.set_zoom(v);
            }
        };
        let make_scale = |width: i32| {
            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.02);
            scale.set_value(p.zoom);
            scale.set_draw_value(false);
            scale.set_width_request(width);
            scale.set_tooltip_text(Some("Thumbnail size"));
            scale
        };
        let inline = self.zoom_scale.clone();
        inline.set_value(p.zoom);
        inline.set_draw_value(false);
        inline.set_width_request(120);
        inline.set_tooltip_text(Some("Thumbnail size (Ctrl+wheel)"));
        let popover_scale = make_scale(180);
        // Moving either slider moves the other; setting an equal value does nothing, so it settles.
        for (this, other) in [(&inline, &popover_scale), (&popover_scale, &inline)] {
            let other = other.clone();
            this.connect_value_changed(move |s| {
                if (other.value() - s.value()).abs() > 1e-6 {
                    other.set_value(s.value());
                }
                apply_zoom(s.value());
            });
        }
        let zoom = &self.zoom_inline;
        zoom.add_css_class("zoom-control");
        zoom.append(&inline);
        foot.append(&group_menu);
        foot.append(zoom);

        self.zoom_menu.set_icon_name("zoom-in-symbolic");
        self.zoom_menu.set_tooltip_text(Some("Thumbnail size"));
        self.zoom_menu.add_css_class("icon-button");
        self.zoom_menu.add_css_class("flat");
        self.zoom_menu.set_valign(gtk::Align::Center);
        self.zoom_menu.set_visible(false);
        let pop = gtk::Popover::new();
        let pop_box = hbox(8);
        pop_box.append(&gtk::Image::from_icon_name("zoom-out-symbolic"));
        pop_box.append(&popover_scale);
        pop_box.append(&gtk::Image::from_icon_name("zoom-in-symbolic"));
        pop.set_child(Some(&pop_box));
        self.zoom_menu.set_popover(Some(&pop));
        foot.append(&self.zoom_menu);

        let a = self.clone();
        self.info_button.connect_clicked(move |_| a.toggle_inspector());
    }

    /// A sidebar entry. `count` is `Some` for an entry that shows a number (which may
    /// not be known yet).
    fn nav_item(self: &Rc<Self>, list: &gtk::Box, id: &str, icon: &str, text: &str, count: Option<Option<usize>>) {
        let button = gtk::Button::new();
        button.add_css_class("nav-item");
        let content = hbox(12);
        content.append(&gtk::Image::from_icon_name(icon));
        let l = label(text, "nav-label");
        l.set_hexpand(true);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        self.compact_items.borrow_mut().push(l.clone().upcast());
        content.append(&l);
        if let Some(n) = count {
            let c = label(&n.map(|n| n.to_string()).unwrap_or_default(), "nav-count mono");
            self.compact_items.borrow_mut().push(c.clone().upcast());
            self.counts.borrow_mut().insert(id.to_string(), c.clone());
            content.append(&c);
        }
        button.set_child(Some(&content));
        button.set_tooltip_text(Some(text));
        let (a, id2) = (self.clone(), id.to_string());
        button.connect_clicked(move |_| a.navigate(&id2));
        list.append(&button);
        self.nav_items.borrow_mut().insert(id.to_string(), button);
    }

    fn make_group_label(&self, text: &str) -> gtk::Label {
        let g = label(&text.to_uppercase(), "nav-group");
        self.compact_items.borrow_mut().push(g.clone().upcast());
        g
    }


    fn build_nav(self: &Rc<Self>, list: &gtk::Box) {
        self.compact_items.borrow_mut().clear();
        self.counts.borrow_mut().clear();
        self.nav_items.borrow_mut().clear();
        // One list, no heading: the library and its views first, then the media kinds,
        // then the tools.
        for id in ["all_photos", "favorites", "recent", "videos", "screenshots", "photos", "folders", "duplicates"] {
            if let Some((id, title, icon)) = LIBRARY_SECTIONS.iter().chain(MEDIA_SECTIONS).find(|(s, _, _)| *s == id) {
                let n = self.state.borrow().count(id);
                self.nav_item(list, id, icon, title, Some(n));
            }
        }
        // Albums: a heading with an add button, then one item per album.
        let head = hbox(0);
        head.add_css_class("nav-group-row");
        let g = self.make_group_label("Albums");
        g.set_hexpand(true);
        head.append(&g);
        let add = gtk::Button::from_icon_name("list-add-symbolic");
        add.add_css_class("nav-add");
        add.set_tooltip_text(Some("New album"));
        add.set_valign(gtk::Align::Center);
        let a = self.clone();
        add.connect_clicked(move |_| a.new_album());
        head.append(&add);
        // The heading's text hides when compact; the add button stays, centred.
        list.append(&head);
        list.append(&self.albums_box);
        self.build_albums();
    }

    fn build_albums(self: &Rc<Self>) {
        // Forget the old album items before they are removed.
        self.compact_items.borrow_mut().retain(|w| !w.is_ancestor(&self.albums_box));
        self.counts.borrow_mut().retain(|_, l| !l.is_ancestor(&self.albums_box));
        while let Some(c) = self.albums_box.first_child() {
            self.albums_box.remove(&c);
        }
        let albums: Vec<(String, String)> =
            self.state.borrow().lib.albums.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
        {
            let mut items = self.nav_items.borrow_mut();
            items.retain(|k, _| self.state.borrow().lib.album(k).is_some() || !k.starts_with("album_") && k != "highlight_album");
        }
        for (id, name) in albums {
            let n = self.state.borrow().count(&id);
            self.nav_item(&self.albums_box, &id, "folder-symbolic", &name, Some(n));
            self.album_cover(&id);
            // Right-click an album for its menu.
            if let Some(button) = self.nav_items.borrow().get(&id) {
                let click = gtk::GestureClick::new();
                click.set_button(gdk::BUTTON_SECONDARY);
                let (a, id) = (self.clone(), id.clone());
                click.connect_pressed(move |g, _, x, y| {
                    if let Some(w) = g.widget() {
                        a.show_album_menu(&id, &w, x, y);
                    }
                });
                button.add_controller(click);
                button.set_tooltip_text(Some(&format!("{name} — right-click to rename or delete")));
            }
        }
        let compact = self.compact.get();
        for w in self.compact_items.borrow().iter() {
            w.set_visible(!compact);
        }
        centre_icons(self.nav.upcast_ref(), compact);
        self.mark_active_nav();
    }

    /// An album's row shows its first photo, small and rounded, in place of the icon.
    fn album_cover(self: &Rc<Self>, id: &str) {
        let first = {
            let s = self.state.borrow();
            s.lib.album(id).and_then(|a| {
                a.photo_ids.iter().find_map(|pid| s.lib.photos.iter().find(|p| &p.id == pid)).map(cell_data)
            })
        };
        let (Some(cell), Some(button)) = (first, self.nav_items.borrow().get(id).cloned()) else { return };
        let Some(icon) = button.child().and_then(|c| c.first_child()) else { return };
        let cover = gtk::Picture::new();
        cover.add_css_class("album-cover");
        cover.set_content_fit(gtk::ContentFit::Cover);
        cover.set_size_request(18, 18);
        cover.set_overflow(gtk::Overflow::Hidden);
        cover.set_valign(gtk::Align::Center);
        let weak = cover.downgrade();
        crate::grid::thumbnail_texture(&cell, move |tex| {
            if let Some(cover) = weak.upgrade() {
                cover.set_paintable(Some(&tex));
            }
        });
        if let Some(content) = icon.parent().and_downcast::<gtk::Box>() {
            content.insert_child_after(&cover, Some(&icon));
            content.remove(&icon);
        }
    }

    fn wire(self: &Rc<Self>, _nav_scroll: &gtk::ScrolledWindow) {
        let w = &self.window;

        let a = self.clone();
        self.collapse_button.connect_clicked(move |_| a.toggle_sidebar());
        let a = self.clone();
        self.search.connect_search_changed(move |e| {
            a.state.borrow_mut().query = e.text().to_string();
            a.refresh(true);
        });
        let a = self.clone();
        self.search.connect_activate(move |_| a.leave_search());

        // Keys. The capture phase sees them before the focused widget does.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let a = self.clone();
        keys.connect_key_pressed(move |_, key, _, mods| a.on_key(key, mods));
        w.add_controller(keys);

        // Thumbnails and the viewer.
        let a = self.clone();
        self.gallery.on_click(move |idx, mods, n| a.on_cell_click(idx, mods, n));
        // The star on a thumbnail favourites that photo only, whatever is selected.
        let a = self.clone();
        self.gallery.on_favorite(move |idx| {
            let id = a.state.borrow().photo(idx).map(|p| p.id.clone());
            if let Some(id) = id {
                a.toggle_favorites(vec![id]);
            }
        });
        let a = self.clone();
        self.gallery.on_context(move |idx, anchor, x, y| a.show_context_menu(idx, anchor, x, y));
        // Dragging a selected photo takes the whole selection along.
        let a = self.clone();
        self.gallery.on_drag(move |idx| {
            let s = a.state.borrow();
            let Some(id) = s.photo(idx).map(|p| p.id.clone()) else { return Vec::new() };
            let ids = if s.selected.contains(&id) { s.selected.clone() } else { vec![id] };
            s.photos_by_id(&ids).iter().map(|p| p.path.clone()).collect()
        });
        let a = self.clone();
        self.gallery.on_zoom(move |step| a.zoom_thumbnails(step));
        let a = self.clone();
        self.inspector.on_action(move |action| a.on_inspector(action));
        let a = self.clone();
        self.lightbox.on_action(move |action| a.on_lightbox(action));

        // Narrow windows (a tiled half-screen) get an icon-only sidebar.
        let a = self.clone();
        w.connect_default_width_notify(move |_| a.apply_width());
        let a = self.clone();
        w.connect_realize(move |_| a.apply_width());
        // Tiled windows are resized by the compositor without touching the
        // default size: an invisible layer over the whole window reports each
        // real size change, and the layout follows on the next frame.
        if let Some(overlay) = w.child().and_downcast::<gtk::Overlay>() {
            let probe = gtk::DrawingArea::new();
            probe.set_can_target(false);
            probe.set_can_focus(false);
            overlay.add_overlay(&probe);
            overlay.set_measure_overlay(&probe, false);
            let a = self.clone();
            probe.connect_resize(move |_, _, _| {
                let a = a.clone();
                // After this layout pass, when the window's width is the new one.
                glib::idle_add_local_once(move || a.apply_width());
            });
        }
        // A slow fallback, in case a resize slips by.
        let a = self.clone();
        glib::timeout_add_local(Duration::from_millis(1500), move || {
            a.apply_width();
            glib::ControlFlow::Continue
        });
        // The stand-alone viewer may have changed favourites or rotations meanwhile.
        let a = self.clone();
        w.connect_is_active_notify(move |w| {
            if !w.is_active() {
                return;
            }
            let now = library::saved_stamp();
            if *a.saved_stamp.borrow() != now {
                *a.saved_stamp.borrow_mut() = now;
                a.state.borrow_mut().lib.reload_saved();
                a.build_albums();
                a.refresh(false);
            }
        });
        w.connect_close_request(|_| {
            prefs::flush();
            glib::Propagation::Proceed
        });
        self.apply_inspector_visibility();
    }

    // ----- Keys -----

    fn typing(&self) -> bool {
        GtkWindowExt::focus(&self.window).is_some_and(|f| f.dynamic_cast_ref::<gtk::Editable>().is_some())
    }

    fn on_key(self: &Rc<Self>, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let stop = glib::Propagation::Stop;
        if self.editor.is_open() {
            if ctrl && matches!(key, gdk::Key::q | gdk::Key::w) {
                self.window.close();
                return stop;
            }
            return self.editor.handle_key(key, mods);
        }
        if settings_dialog::is_open() {
            match key {
                gdk::Key::Escape => settings_dialog::escape(),
                gdk::Key::f if ctrl => settings_dialog::focus_search(),
                gdk::Key::q | gdk::Key::w if ctrl => self.window.close(),
                _ => return glib::Propagation::Proceed,
            }
            return stop;
        }
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        if ctrl {
            let browsing = !self.typing() && self.on_gallery();
            match key {
                gdk::Key::f | gdk::Key::k => self.focus_search(),
                gdk::Key::b => self.toggle_sidebar(),
                gdk::Key::q | gdk::Key::w => self.window.close(),
                gdk::Key::o => self.reveal(),
                gdk::Key::a if browsing && !self.lightbox.is_open() => {
                    self.state.borrow_mut().select_all();
                    self.refresh_selection(false);
                }
                gdk::Key::c if browsing => self.copy_image(),
                gdk::Key::s if browsing => self.save_rotation(),
                gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add if browsing && !self.lightbox.is_open() => {
                    self.zoom_thumbnails(0.1)
                }
                gdk::Key::minus | gdk::Key::KP_Subtract if browsing && !self.lightbox.is_open() => self.zoom_thumbnails(-0.1),
                gdk::Key::comma => self.show_settings(),
                _ => return glib::Propagation::Proceed,
            }
            return stop;
        }
        if key == gdk::Key::Escape {
            self.escape();
            return stop;
        }
        if key == gdk::Key::F1 {
            dialog::show_shortcuts(settings_view::SHORTCUTS);
            return stop;
        }
        if self.typing() {
            return glib::Propagation::Proceed;
        }
        if key == gdk::Key::question {
            dialog::show_shortcuts(settings_view::SHORTCUTS);
            return stop;
        }
        if self.lightbox.is_open() {
            // Any key but the slideshow's own stops the slideshow.
            if key != gdk::Key::s && self.lightbox.is_playing() {
                self.lightbox.stop_slideshow();
            }
            match key {
                gdk::Key::Left => self.step_viewer(-1),
                gdk::Key::Right => self.step_viewer(1),
                gdk::Key::s => self.toggle_slideshow(),
                gdk::Key::w => self.set_background(),
                gdk::Key::Delete | gdk::Key::KP_Delete => self.delete_selected(),
                gdk::Key::F2 => self.rename(),
                gdk::Key::space | gdk::Key::Return | gdk::Key::KP_Enter => self.close_viewer(),
                gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => self.lightbox.zoom_by(1.25),
                gdk::Key::minus | gdk::Key::KP_Subtract => self.lightbox.zoom_by(0.8),
                gdk::Key::_0 | gdk::Key::KP_0 => self.lightbox.zoom_fit(),
                gdk::Key::_1 | gdk::Key::KP_1 => self.lightbox.zoom_actual(),
                gdk::Key::r => self.rotate(),
                gdk::Key::e => self.open_editor(),
                gdk::Key::f | gdk::Key::period => self.toggle_favorite(),
                _ => return glib::Propagation::Proceed,
            }
            return stop;
        }
        if !self.on_gallery() {
            return glib::Propagation::Proceed;
        }
        match key {
            gdk::Key::Left => self.move_cursor(-1, shift),
            gdk::Key::Right => self.move_cursor(1, shift),
            gdk::Key::Up => self.move_vertical(false, 1, shift),
            gdk::Key::Down => self.move_vertical(true, 1, shift),
            gdk::Key::Page_Up => self.move_vertical(false, self.gallery.rows_per_page(), shift),
            gdk::Key::Page_Down => self.move_vertical(true, self.gallery.rows_per_page(), shift),
            gdk::Key::Home => self.move_to(0, shift),
            gdk::Key::End => self.move_to(isize::MAX, shift),
            gdk::Key::Delete | gdk::Key::KP_Delete => self.delete_selected(),
            gdk::Key::F2 => self.rename(),
            gdk::Key::space | gdk::Key::Return | gdk::Key::KP_Enter => self.open_viewer(),
            gdk::Key::i => self.toggle_inspector(),
            gdk::Key::r => self.rotate(),
            gdk::Key::e => self.open_editor(),
            gdk::Key::f | gdk::Key::period => self.toggle_favorite(),
            _ => return glib::Propagation::Proceed,
        }
        stop
    }

    fn escape(self: &Rc<Self>) {
        if self.lightbox.is_open() {
            self.close_viewer();
        } else if !self.search.text().is_empty() {
            self.search.set_text("");
            self.leave_search();
        } else if self.typing() {
            self.leave_search();
        } else {
            self.state.borrow_mut().clear_selection();
            self.refresh_selection(false);
        }
    }

    fn on_gallery(&self) -> bool {
        !settings_dialog::is_open()
    }

    // ----- Navigation -----

    pub fn navigate(self: &Rc<Self>, id: &str) {
        let id = if id == "timeline" { "all_photos" } else { id };
        if id == SETTINGS_PAGE {
            self.show_settings();
            if !self.page.borrow().is_empty() {
                return;
            }
        }
        let id = if id == SETTINGS_PAGE { "all_photos" } else { id };
        self.lightbox.hide();
        {
            let known = self.state.borrow().lib.album(id).is_some()
                || LIBRARY_SECTIONS.iter().chain(MEDIA_SECTIONS).any(|(s, _, _)| *s == id);
            let id = if known { id } else { "all_photos" };
            self.state.borrow_mut().section = id.to_string();
            *self.page.borrow_mut() = id.to_string();
            self.pages.set_visible_child_name("gallery");
            if id == "duplicates" && self.state.borrow().duplicates.is_none() {
                self.find_duplicates();
            }
            self.refresh(true);
            prefs::update(|p| p.last_section = id.to_string());
        }
        self.mark_active_nav();
        self.apply_inspector_visibility();
    }

    /// Settings, as a card over the window. It's built afresh so the library
    /// figures are current.
    fn show_settings(self: &Rc<Self>) {
        let (items, size) = {
            let s = self.state.borrow();
            (s.lib.photos.len(), s.lib.total_bytes_formatted())
        };
        let (a, b, c) = (self.clone(), self.clone(), self.clone());
        let (page, body) = settings_view::build(settings_view::Info {
            items,
            size,
            on_rescan: Rc::new(move || a.rescan(false)),
            on_folders: Rc::new(move || b.use_library_folders()),
            on_layout: Rc::new(move || {
                let p = prefs::get();
                c.gallery.set_justified(p.layout == "justified");
                c.state.borrow_mut().view_mode = p.view_mode;
                c.refresh(true);
            }),
        });
        if self.narrow.get() {
            body.add_css_class("narrow");
        }
        settings_dialog::open(&self.overlay, &self.window, &page, &body);
    }

    fn mark_active_nav(&self) {
        let page = self.page.borrow().clone();
        for (id, b) in self.nav_items.borrow().iter() {
            if *id == page {
                b.add_css_class("active");
            } else {
                b.remove_css_class("active");
            }
        }
    }

    // ----- Refreshing what is on screen -----

    fn refresh(self: &Rc<Self>, scroll_top: bool) {
        self.state.borrow_mut().recompute();
        self.update_counts();
        let s = self.state.borrow();
        let items: Vec<CellData> = s
            .shown
            .iter()
            .map(|&i| {
                let p = &s.lib.photos[i];
                cell_data(p)
            })
            .collect();
        let empty = items.is_empty();
        let message = self.empty_message(&s);
        let n = s.shown.len();
        chrome::set_title(&self.window, "Photos", &s.section_title());
        self.header.set_subtitle(&s.section_title());
        let videos = s.shown.iter().filter(|&&i| s.lib.photos[i].is_video()).count();
        let plural = |n: usize, one: &str, many: &str| format!("{} {}", thousands(n), if n == 1 { one } else { many });
        self.status_items.set_text(&format!(
            "{} · {}",
            plural(n - videos, "photo", "photos"),
            plural(videos, "video", "videos")
        ));
        let spans = s.spans.clone();
        // The scrubber is a timeline; folders and duplicates aren't one.
        let timeline = !matches!(s.section.as_str(), "folders" | "duplicates");
        let album = s.album_id().map(String::from);
        let library_empty = s.lib.photos.is_empty() && !self.scanning.get() && s.query.trim().is_empty();
        drop(s);
        self.gallery.set_timeline(timeline);
        self.gallery.set_items(items, spans, scroll_top);
        let action: Option<widgets::ButtonAction> = library_empty.then(|| {
            let a = self.clone();
            ("Choose a folder…", Rc::new(move || a.choose_library_folder()) as Rc<dyn Fn()>)
        });
        self.gallery.set_empty(if empty { Some((message.0, message.1.as_str(), message.2.as_str())) } else { None }, action);
        self.update_album_actions(album);
        self.refresh_selection(true);
    }

    fn empty_message(&self, s: &State) -> (&'static str, String, String) {
        let root = s.lib.roots.iter().map(|r| paths::pretty(r)).collect::<Vec<_>>().join(", ");
        if self.scanning.get() && s.lib.photos.is_empty() {
            return ("view-refresh-symbolic", "Scanning your photos".into(), format!("Looking in {root}."));
        }
        if s.lib.photos.is_empty() && s.query.trim().is_empty() {
            return (
                "folder-pictures-symbolic",
                "No photos here yet".into(),
                format!("Photos looks in {root}. Pick the folder where your photos are, or add photos there."),
            );
        }
        if !s.query.trim().is_empty() {
            return (
                "system-search-symbolic",
                format!("No photos match “{}”", s.query.trim()),
                "Try a file name, a folder, a camera, a date such as 2024 or “last month”, or is:video.".into(),
            );
        }
        match s.section.as_str() {
            "favorites" => ("starred-symbolic", "No favorites yet".into(), "Press F on a photo to add it here.".into()),
            "recent" => (
                "document-open-recent-symbolic",
                "Nothing new lately".into(),
                "Photos added in the last 30 days appear here.".into(),
            ),
            "duplicates" if s.duplicates.is_none() => {
                ("edit-copy-symbolic", "Looking for duplicates".into(), "Comparing files of the same size…".into())
            }
            "duplicates" => ("edit-copy-symbolic", "No duplicates".into(), "No two files in the library are the same.".into()),
            "videos" => {
                ("video-x-generic-symbolic", "No videos found".into(), format!("Add video files (.mp4, .mov, .mkv) to {root}."))
            }
            "screenshots" => {
                ("video-display-symbolic", "No screenshots found".into(), format!("Screenshots in {root} appear here."))
            }
            id if s.lib.album(id).is_some() => (
                "folder-symbolic",
                "This album is empty".into(),
                "Right-click photos and choose an album under “Add to album”.".into(),
            ),
            _ => ("image-x-generic-symbolic", "No photos found".into(), format!("Add photos to {root}.")),
        }
    }

    /// Rename and delete, shown in the bar while an album is open.
    fn update_album_actions(self: &Rc<Self>, album: Option<String>) {
        while let Some(c) = self.header_actions.first_child() {
            self.header_actions.remove(&c);
        }
        let Some(id) = album else { return };
        let rename = widgets::icon_button("document-edit-symbolic", "Rename album");
        let (a, rid) = (self.clone(), id.clone());
        rename.connect_clicked(move |_| a.rename_album(&rid));
        self.header_actions.append(&rename);
        let delete = widgets::icon_button("user-trash-symbolic", "Delete album (the photos stay)");
        delete.add_css_class("danger");
        let a = self.clone();
        delete.connect_clicked(move |_| a.delete_album(&id));
        self.header_actions.append(&delete);
    }

    fn update_counts(&self) {
        let s = self.state.borrow();
        for (id, l) in self.counts.borrow().iter() {
            l.set_text(&s.count(id).map(|n| n.to_string()).unwrap_or_default());
        }
    }

    fn refresh_selection(self: &Rc<Self>, reveal: bool) {
        let s = self.state.borrow();
        self.gallery.set_selection(s.selected_indices(), s.cursor_index(), reveal);
        let n = s.selected.len();
        self.status_selection.set_text("");
        // The pill says how many only for a multiple selection; one photo's actions stay.
        self.deselect.set_visible(n > 1);
        self.selection_chip.set_visible(n > 1);
        self.selection_chip.set_text(&format!("{n} selected"));
        if n > 1 {
            self.selection_pill.add_css_class("many");
        } else {
            self.selection_pill.remove_css_class("many");
        }
        let ids = s.target_ids();
        let any = !ids.is_empty();
        let all_fav = any && ids.iter().all(|id| s.lib.is_favorite(id));
        let editable = ids.len() == 1 && s.cursor_photo().is_some_and(|p| !p.is_video());
        self.fav_button.set_icon_name(if all_fav { "starred-symbolic" } else { "non-starred-symbolic" });
        self.fav_button.set_tooltip_text(Some(if all_fav { "Remove from favorites (F)" } else { "Add to favorites (F)" }));
        if all_fav {
            self.fav_button.add_css_class("on");
        } else {
            self.fav_button.remove_css_class("on");
        }
        self.edit_button.set_sensitive(editable);
        self.rotate_button.set_sensitive(editable);
        for w in [self.fav_button.upcast_ref::<gtk::Widget>(), self.trash_button.upcast_ref(), self.album_menu.upcast_ref()] {
            w.set_sensitive(any);
        }
        let view = inspector::View {
            photo: s.cursor_photo().cloned(),
            selected: n,
            albums: s
                .cursor
                .as_ref()
                .map(|id| {
                    s.lib.albums.iter().filter(|a| a.photo_ids.contains(id)).map(|a| (a.id.clone(), a.name.clone())).collect()
                })
                .unwrap_or_default(),
        };
        drop(s);
        self.inspector.update(&view);
    }

    // ----- Library -----

    /// Show what the last scan found, before scanning again.
    fn load_cached_library(self: &Rc<Self>) {
        let cache = library::load_cache();
        if cache.is_empty() {
            return;
        }
        let roots = self.state.borrow().lib.roots.clone();
        let mut photos: Vec<_> =
            cache.into_values().filter(|p| roots.iter().any(|r| std::path::Path::new(&p.path).starts_with(r))).collect();
        crate::scanner::sort_newest_first(&mut photos);
        self.set_screens();
        self.state.borrow_mut().lib.set_photos(photos);
        self.state.borrow_mut().recompute();
    }

    /// Tell the library the connected screens' sizes, so screenshots can be told apart.
    fn set_screens(&self) {
        let Some(display) = gdk::Display::default() else { return };
        let monitors = display.monitors();
        let sizes = (0..monitors.n_items())
            .filter_map(|i| monitors.item(i).and_downcast::<gdk::Monitor>())
            .map(|m| {
                let g = m.geometry();
                let scale = m.scale();
                ((g.width() as f64 * scale).round() as u32, (g.height() as f64 * scale).round() as u32)
            })
            .collect();
        self.state.borrow_mut().lib.screens = sizes;
    }

    /// Scan the folders. `quiet` (at start-up, or after a watched folder changed) says
    /// nothing unless something was added or went.
    fn rescan(self: &Rc<Self>, quiet: bool) {
        if self.scanning.replace(true) {
            self.rescan_again.set(true);
            return;
        }
        if !quiet {
            self.status_items.set_text("Scanning…");
        }
        let (roots, depth) = (self.state.borrow().lib.roots.clone(), self.scan_depth.get());
        let known: HashMap<String, crate::models::PhotoItem> =
            self.state.borrow().lib.photos.iter().map(|p| (p.path.clone(), p.clone())).collect();
        let before: std::collections::HashSet<String> = known.keys().cloned().collect();
        let in_library = self.extra_root.borrow().is_none();
        let a = self.clone();
        cmd::background(
            move || library::scan(&roots, depth, &known),
            move |scan| {
                a.scanning.set(false);
                let n = scan.items.len();
                let added = scan.items.iter().filter(|p| !before.contains(&p.path)).count();
                let removed = before.len().saturating_sub(n - added);
                let changed = added > 0 || removed > 0;
                if in_library {
                    let items = scan.items.clone();
                    std::thread::spawn(move || {
                        library::save_cache(&items);
                        let keep = items.iter().map(|p| PathBuf::from(&p.thumbnail_path)).collect();
                        crate::thumbnails::prune(&keep);
                    });
                }
                a.watch(&scan.dirs);
                a.set_screens();
                a.state.borrow_mut().lib.set_photos(scan.items);
                if changed || !quiet {
                    a.state.borrow_mut().duplicates = None;
                    if a.state.borrow().section == "duplicates" {
                        a.find_duplicates();
                    }
                }
                a.refresh(false);
                a.build_albums();
                if !a.on_gallery() {
                    a.show_settings();
                }
                if let Some(path) = a.open_after_scan.borrow_mut().take() {
                    a.open_path(path);
                }
                let items = |n: usize| if n == 1 { "item" } else { "items" };
                if !quiet {
                    a.show_toast(&format!("Found {n} {}", items(n)));
                } else if before.is_empty() {
                    // The first scan, with nothing cached: no news to give.
                } else if added > 0 && removed == 0 {
                    a.show_toast(&format!("{added} new {}", items(added)));
                } else if removed > 0 && added == 0 && removed > 1 {
                    a.show_toast(&format!("{removed} {} gone from the folders", items(removed)));
                }
                if a.rescan_again.take() {
                    a.rescan(true);
                }
            },
        );
        // Show the scanning message right away.
        if self.state.borrow().shown.is_empty() {
            self.refresh(true);
        }
    }

    /// Watch `dirs` and scan again shortly after anything in them changes.
    fn watch(self: &Rc<Self>, dirs: &[PathBuf]) {
        let mut monitors = self.monitors.borrow_mut();
        for m in monitors.drain(..) {
            m.cancel();
        }
        for dir in dirs {
            let Ok(m) = gio::File::for_path(dir).monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            else {
                continue;
            };
            let weak = Rc::downgrade(self);
            m.connect_changed(move |_, file, _, event| {
                use gio::FileMonitorEvent as E;
                if !matches!(event, E::Created | E::Deleted | E::ChangesDoneHint | E::MovedIn | E::MovedOut | E::Renamed) {
                    return;
                }
                // Ignore the temporary files written while saving.
                if file.basename().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                    return;
                }
                if let Some(a) = weak.upgrade() {
                    a.rescan_soon();
                }
            });
            monitors.push(m);
        }
    }

    fn rescan_soon(self: &Rc<Self>) {
        if let Some(t) = self.rescan_timer.borrow_mut().take() {
            t.remove();
        }
        let a = self.clone();
        let id = glib::timeout_add_local_once(RESCAN_AFTER, move || {
            a.rescan_timer.borrow_mut().take();
            a.rescan(true);
        });
        *self.rescan_timer.borrow_mut() = Some(id);
    }

    /// Go back to the library folders from Settings (after they changed), or from a
    /// folder opened on the command line.
    fn use_library_folders(self: &Rc<Self>) {
        self.extra_root.borrow_mut().take();
        self.scan_depth.set(8);
        let roots = library::configured_roots(&prefs::get().folders);
        {
            let mut s = self.state.borrow_mut();
            s.lib.roots = roots;
            s.lib.photos.clear();
        }
        self.load_cached_library();
        self.refresh(true);
        self.rescan(false);
    }

    /// Pick the folder the library is in (from the empty gallery).
    fn choose_library_folder(self: &Rc<Self>) {
        let a = self.clone();
        dialog::choose_folder("Where are your photos?", Some(&paths::home()), move |path| {
            prefs::update(|p| p.folders = vec![paths::pretty(&path)]);
            a.use_library_folders();
        });
    }

    fn find_duplicates(self: &Rc<Self>) {
        if self.finding_duplicates.replace(true) {
            return;
        }
        let files: Vec<(String, String, u64)> =
            self.state.borrow().lib.photos.iter().map(|p| (p.id.clone(), p.path.clone(), p.file_size)).collect();
        let a = self.clone();
        cmd::background(
            move || library::find_duplicates(files),
            move |sets| {
                a.finding_duplicates.set(false);
                a.state.borrow_mut().duplicates = Some(sets);
                if a.state.borrow().section == "duplicates" {
                    a.refresh(false);
                } else {
                    a.update_counts();
                }
            },
        );
    }

    /// Show one file in the viewer, looking in its folder if it isn't in the library yet.
    pub fn open_path(self: &Rc<Self>, path: PathBuf) {
        let id = self.state.borrow().lib.photos.iter().find(|p| std::path::Path::new(&p.path) == path).map(|p| p.id.clone());
        match id {
            Some(id) => {
                self.search.set_text("");
                let idx = {
                    let mut s = self.state.borrow_mut();
                    s.query.clear();
                    s.recompute();
                    s.index_of(&id)
                };
                if let Some(i) = idx {
                    self.state.borrow_mut().select_single(i);
                    self.refresh_selection(true);
                    if self.edit_when_opened.take() {
                        self.open_editor();
                    } else {
                        self.open_builtin_viewer();
                    }
                }
            }
            None => {
                if let Some(dir) = path.parent() {
                    self.state.borrow_mut().lib.roots = vec![dir.to_path_buf()];
                    *self.extra_root.borrow_mut() = Some(dir.to_path_buf());
                }
                self.scan_depth.set(1);
                *self.open_after_scan.borrow_mut() = Some(path);
                self.scanning.set(false);
                self.rescan(true);
            }
        }
    }

    // ----- Selection and actions -----

    fn on_cell_click(self: &Rc<Self>, idx: usize, mods: gdk::ModifierType, presses: i32) {
        {
            let mut s = self.state.borrow_mut();
            if presses >= 2 {
                s.select_single(idx);
            } else if mods.contains(gdk::ModifierType::CONTROL_MASK) {
                s.toggle(idx);
            } else if mods.contains(gdk::ModifierType::SHIFT_MASK) {
                s.select_range(idx);
            } else {
                s.select_single(idx);
            }
        }
        self.refresh_selection(false);
        if presses >= 2 {
            self.open_viewer();
        }
    }

    fn move_to(self: &Rc<Self>, idx: isize, extend: bool) {
        self.state.borrow_mut().move_to(idx, extend);
        self.refresh_selection(true);
    }

    fn move_cursor(self: &Rc<Self>, delta: isize, extend: bool) {
        let cur = self.state.borrow().cursor_index().map(|i| i as isize).unwrap_or(-1);
        self.move_to(cur + delta, extend);
    }

    fn move_vertical(self: &Rc<Self>, down: bool, rows: usize, extend: bool) {
        let Some(cur) = self.state.borrow().cursor_index() else {
            self.move_cursor(1, false);
            return;
        };
        let target = self.gallery.vertical_target(cur, down, rows);
        // Past the last row: go to the very first or last photo.
        let target = target.map(|t| t as isize).unwrap_or(if down { isize::MAX } else { 0 });
        self.move_to(target, extend);
    }

    /// Thumbnail size up or down by `step` (of 0 to 1), as the slider would.
    fn zoom_thumbnails(&self, step: f64) {
        let v = (self.zoom_scale.value() + step).clamp(0.0, 1.0);
        self.zoom_scale.set_value(v);
    }

    /// Open the photo under the cursor in the viewer chosen in Settings.
    fn open_viewer(self: &Rc<Self>) {
        let standalone = {
            let s = self.state.borrow();
            let Some(photo) = s.cursor_photo() else { return };
            (prefs::get().open_with == "viewer" && !photo.is_video() && cmd::present(VIEWER)).then(|| photo.path.clone())
        };
        match standalone {
            Some(path) => cmd::spawn(&[VIEWER, &path]),
            None => self.open_builtin_viewer(),
        }
    }

    fn open_builtin_viewer(self: &Rc<Self>) {
        if self.state.borrow().cursor_photo().is_some() {
            self.show_in_viewer();
        }
    }

    /// Show the cursor's photo in the viewer, with the photos around it in the
    /// filmstrip, and decode its neighbours ahead.
    fn show_in_viewer(&self) {
        let s = self.state.borrow();
        let Some(photo) = s.cursor_photo() else { return };
        let Some(at) = s.cursor_index() else { return };
        let rotation = s.lib.rotation(&photo.id);
        let n = s.shown.len();
        let from = at.saturating_sub(lightbox::STRIP_SIDE);
        let to = (at + lightbox::STRIP_SIDE + 1).min(n);
        let strip = (from..to).filter_map(|i| s.photo(i).map(|p| (i, cell_data(p)))).collect();
        self.lightbox.show(photo, rotation, at + 1, n, strip);
        if n > 1 {
            for i in [(at + 1) % n, (at + n - 1) % n] {
                if let Some(p) = s.photo(i).filter(|p| !p.is_video()) {
                    crate::stage::preload(&p.path, s.lib.rotation(&p.id));
                }
            }
        }
    }

    fn close_viewer(self: &Rc<Self>) {
        self.lightbox.hide();
        self.refresh_selection(true);
    }

    fn step_viewer(self: &Rc<Self>, delta: isize) {
        self.state.borrow_mut().step_wrapping(delta);
        self.show_in_viewer();
        self.refresh_selection(false);
    }

    fn toggle_slideshow(self: &Rc<Self>) {
        if self.lightbox.is_playing() {
            self.lightbox.stop_slideshow();
        } else {
            self.lightbox.start_slideshow(prefs::get().slideshow_seconds);
        }
    }

    /// Start a slideshow of what the gallery shows, from the selected photo.
    fn start_slideshow(self: &Rc<Self>) {
        if self.state.borrow().cursor_photo().is_none() {
            self.state.borrow_mut().move_to(0, false);
        }
        self.show_in_viewer();
        self.lightbox.start_slideshow(prefs::get().slideshow_seconds);
    }

    /// Open the editor on the selected photo, starting from how the viewer has it turned.
    fn open_editor(self: &Rc<Self>) {
        let (path, name, rotation) = {
            let s = self.state.borrow();
            let Some(p) = s.cursor_photo() else { return };
            (p.path.clone(), p.file_name.clone(), s.lib.rotation(&p.id))
        };
        self.editor.open(std::path::Path::new(&path), &name, rotation);
    }

    fn on_editor_outcome(self: &Rc<Self>, outcome: Outcome) {
        let Outcome::Saved { path, original, backup } = outcome else {
            self.refresh_selection(false);
            return;
        };
        let replaced = path == original;
        let id = {
            let mut s = self.state.borrow_mut();
            // The editor started from the viewer's turn, so it is in the file now.
            let original_id = s.lib.photos.iter().find(|p| std::path::Path::new(&p.path) == original).map(|p| p.id.clone());
            if let Some(oid) = original_id {
                s.lib.set_rotation(&oid, 0);
            }
            s.lib.add_or_refresh(&path)
        };
        self.refresh(false);
        if let Some(i) = id.and_then(|id| self.state.borrow().index_of(&id)) {
            self.state.borrow_mut().select_single(i);
        }
        self.refresh_selection(true);
        if self.lightbox.is_open() {
            self.show_in_viewer();
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let message = match (replaced, backup) {
            (false, _) => format!("Saved “{name}” next to the original"),
            (true, Some(b)) => format!("Replaced the original. A backup is in {}", paths::pretty(b.parent().unwrap_or(&b))),
            (true, None) => "Replaced the original".to_string(),
        };
        self.show_toast(&message);
    }

    fn toggle_favorite(self: &Rc<Self>) {
        let ids = self.state.borrow().target_ids();
        self.toggle_favorites(ids);
    }

    fn toggle_favorites(self: &Rc<Self>, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        // A mixed selection becomes all favourites; otherwise everything flips.
        let all_fav = ids.iter().all(|id| self.state.borrow().lib.is_favorite(id));
        for id in &ids {
            let now = self.state.borrow().lib.is_favorite(id);
            if now == all_fav
                && let Err(e) = self.state.borrow_mut().lib.toggle_favorite(id)
            {
                self.show_toast(&format!("Couldn't save favorites: {e}"));
                return;
            }
        }
        if self.state.borrow().section == "favorites" {
            self.refresh(false);
        } else {
            let s = self.state.borrow();
            for id in &ids {
                if let Some(i) = s.index_of(id) {
                    self.gallery.set_favorite(i, s.photo(i).is_some_and(|p| p.is_favorite));
                }
            }
            drop(s);
            self.update_counts();
            self.refresh_selection(false);
        }
        if self.lightbox.is_open()
            && let Some(p) = self.state.borrow().cursor_photo()
        {
            self.lightbox.set_favorite(p.is_favorite);
        }
        self.show_toast(if all_fav { "Removed from favorites" } else { "Added to favorites" });
    }

    /// The right-click menu. Right-clicking a photo outside the selection selects it first.
    fn show_context_menu(self: &Rc<Self>, idx: usize, anchor: &gtk::Widget, x: f64, y: f64) {
        {
            let mut s = self.state.borrow_mut();
            let Some(id) = s.photo(idx).map(|p| p.id.clone()) else { return };
            if !s.selected.contains(&id) {
                s.select_single(idx);
            } else {
                s.cursor = Some(id);
            }
        }
        self.refresh_selection(false);
        let (count, in_album, albums, favorite, video) = {
            let s = self.state.borrow();
            let ids = s.target_ids();
            let all_fav = ids.iter().all(|id| s.lib.is_favorite(id));
            let video = s.cursor_photo().is_some_and(|p| p.is_video());
            (
                ids.len(),
                s.album_id().is_some(),
                s.lib.albums.iter().map(|a| (a.id.clone(), a.name.clone())).collect::<Vec<_>>(),
                all_fav,
                video,
            )
        };
        let single = count == 1;

        let pop = gtk::Popover::new();
        pop.set_parent(anchor);
        pop.set_has_arrow(false);
        pop.set_halign(gtk::Align::Start);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let list = vbox(2);
        list.set_size_request(250, -1);
        let item = |icon: &str, text: &str, keys: &str, run: MenuAction| {
            let b = gtk::Button::new();
            b.add_css_class("flat");
            b.add_css_class("action-button");
            let content = hbox(8);
            content.append(&gtk::Image::from_icon_name(icon));
            let l = gtk::Label::new(Some(text));
            l.set_xalign(0.0);
            l.set_hexpand(true);
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            content.append(&l);
            if !keys.is_empty() {
                content.append(&widgets::key_combo(keys));
            }
            b.set_child(Some(&content));
            let (a, pop) = (self.clone(), pop.clone());
            b.connect_clicked(move |_| {
                pop.popdown();
                run(&a);
            });
            b
        };
        if single {
            list.append(&item("document-open-symbolic", "Open", "Enter", Box::new(|a| a.open_viewer())));
            if !video {
                list.append(&item("document-edit-symbolic", "Edit", "E", Box::new(|a| a.open_editor())));
                list.append(&item("object-rotate-right-symbolic", "Rotate", "R", Box::new(|a| a.rotate())));
            }
        } else {
            list.append(&item("media-playback-start-symbolic", "Slideshow", "", Box::new(|a| a.start_slideshow())));
        }
        let (icon, text) = if favorite {
            ("non-starred-symbolic", if single { "Remove from favorites" } else { "Remove all from favorites" })
        } else {
            ("starred-symbolic", if single { "Add to favorites" } else { "Add all to favorites" })
        };
        list.append(&item(icon, text, "F", Box::new(|a| a.toggle_favorite())));

        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let heading = label("Add to album", "dim");
        heading.set_xalign(0.0);
        heading.set_margin_start(10);
        list.append(&heading);
        let names = vbox(2);
        for (id, name) in albums {
            names.append(&item("folder-symbolic", &name, "", Box::new(move |a| a.add_to_album(&id))));
        }
        names.append(&item("list-add-symbolic", "New album…", "", Box::new(|a| a.add_to_new_album())));
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(170)
            .child(&names)
            .build();
        list.append(&scroll);
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        if in_album {
            list.append(&item("list-remove-symbolic", "Remove from this album", "", Box::new(|a| a.remove_from_album())));
        }
        if single {
            if !video {
                list.append(&item("edit-copy-symbolic", "Copy image", "Ctrl+C", Box::new(|a| a.copy_image())));
            }
            list.append(&item("edit-copy-symbolic", "Copy path", "", Box::new(|a| a.copy_path())));
            list.append(&item("document-edit-symbolic", "Rename…", "F2", Box::new(|a| a.rename())));
            if !video && crate::actions::can_set_background() {
                list.append(&item(
                    "preferences-desktop-wallpaper-symbolic",
                    "Set as background",
                    "",
                    Box::new(|a| a.set_background()),
                ));
            }
        } else {
            list.append(&item("edit-copy-symbolic", &format!("Copy {count} files"), "Ctrl+C", Box::new(|a| a.copy_image())));
        }
        let export = if single { "Export…".to_string() } else { format!("Export {count} photos…") };
        list.append(&item("document-save-as-symbolic", &export, "", Box::new(|a| a.export())));
        list.append(&item("send-to-symbolic", "Share…", "", Box::new(|a| a.share())));
        list.append(&item("folder-open-symbolic", "Reveal in file manager", "Ctrl+O", Box::new(|a| a.reveal())));
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let trash = if single { "Move to trash".to_string() } else { format!("Move {count} to trash") };
        let delete = item("user-trash-symbolic", &trash, "Delete", Box::new(|a| a.delete_selected()));
        delete.add_css_class("danger");
        list.append(&delete);

        pop.set_child(Some(&list));
        pop.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        pop.popup();
    }

    /// Move the selected photos to the trash and forget them here. The toast can undo it.
    fn delete_selected(self: &Rc<Self>) {
        let (ids, next) = {
            let s = self.state.borrow();
            let ids = s.target_ids();
            let first = ids.iter().filter_map(|id| s.index_of(id)).min().unwrap_or(0);
            (ids, first)
        };
        let paths: Vec<(String, String)> =
            self.state.borrow().photos_by_id(&ids).iter().map(|p| (p.id.clone(), p.path.clone())).collect();
        let (gone_paths, failed) = crate::actions::trash(&paths.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>());
        let gone: Vec<String> = paths.iter().filter(|(_, p)| gone_paths.contains(p)).map(|(id, _)| id.clone()).collect();
        let membership = self.state.borrow().lib.membership(&gone);
        if !gone.is_empty() {
            let result = self.state.borrow_mut().lib.remove_photos(&gone);
            if let Err(e) = result {
                self.show_toast(&format!("Couldn't save the library: {e}"));
            }
            self.state.borrow_mut().selected.clear();
            self.state.borrow_mut().cursor = None;
            self.refresh(false);
            let n = self.state.borrow().shown.len();
            if n > 0 {
                self.state.borrow_mut().select_single(next.min(n - 1));
            }
            self.refresh_selection(true);
            if self.lightbox.is_open() {
                if n == 0 { self.close_viewer() } else { self.show_in_viewer() }
            }
        }
        let n = gone.len();
        let message = crate::actions::trash_message(n, failed);
        if n == 0 {
            self.show_toast(&message);
            return;
        }
        let a = self.clone();
        self.show_toast_with_action(&message, "Undo", move || a.undo_delete(gone_paths.clone(), membership.clone()));
    }

    /// Bring trashed files back, with their favourites, albums and rotations.
    fn undo_delete(self: &Rc<Self>, paths: Vec<String>, membership: library::Membership) {
        let a = self.clone();
        cmd::background(
            move || library::untrash(&paths),
            move |restored| {
                if restored.is_empty() {
                    a.show_toast("Couldn't find the files in the trash");
                    return;
                }
                let first = {
                    let mut s = a.state.borrow_mut();
                    let ids: Vec<String> =
                        restored.iter().filter_map(|p| s.lib.add_or_refresh(std::path::Path::new(p))).collect();
                    if let Err(e) = s.lib.restore(&membership) {
                        drop(s);
                        a.show_toast(&format!("Couldn't save the library: {e}"));
                        return;
                    }
                    ids.into_iter().next()
                };
                a.refresh(false);
                if let Some(i) = first.and_then(|id| a.state.borrow().index_of(&id)) {
                    a.state.borrow_mut().select_single(i);
                    a.refresh_selection(true);
                }
                let n = restored.len();
                a.show_toast(&format!("Put back {n} {}", if n == 1 { "photo" } else { "photos" }));
            },
        );
    }

    fn rotate(self: &Rc<Self>) {
        let Some(id) = self.state.borrow().cursor.clone() else { return };
        self.state.borrow_mut().lib.rotate(&id);
        if self.lightbox.is_open() {
            self.show_in_viewer();
        }
        self.refresh_selection(false);
    }

    /// Write the viewer's rotation into the file (asking nothing: the original is backed
    /// up first when Settings says so).
    fn save_rotation(self: &Rc<Self>) {
        let (path, id, rotation) = {
            let s = self.state.borrow();
            let Some(p) = s.cursor_photo() else { return };
            (p.path.clone(), p.id.clone(), s.lib.rotation(&p.id))
        };
        if rotation == 0 {
            return;
        }
        let a = self.clone();
        self.show_toast("Saving the rotation…");
        crate::actions::save_rotation(std::path::Path::new(&path), rotation, move |result| match result {
            Ok(saved) => {
                a.state.borrow_mut().lib.set_rotation(&id, 0);
                a.state.borrow_mut().lib.add_or_refresh(&saved.path);
                a.refresh(false);
                if a.lightbox.is_open() {
                    a.show_in_viewer();
                }
                a.show_toast(crate::actions::rotation_saved_message(&saved));
            }
            Err(e) => a.show_toast(&format!("Couldn't save the rotation: {e:#}")),
        });
    }

    fn reset_rotation(self: &Rc<Self>) {
        let Some(id) = self.state.borrow().cursor.clone() else { return };
        self.state.borrow_mut().lib.set_rotation(&id, 0);
        if self.lightbox.is_open() {
            self.show_in_viewer();
        }
        self.refresh_selection(false);
    }

    fn reveal(self: &Rc<Self>) {
        let target = {
            let s = self.state.borrow();
            s.cursor_photo()
                .map(|p| p.path.clone())
                .or_else(|| s.lib.roots.first().map(|r| r.to_string_lossy().to_string()))
                .unwrap_or_default()
        };
        library::reveal(&target);
    }

    fn copy_path(self: &Rc<Self>) {
        if let Some(p) = self.state.borrow().cursor_photo() {
            self.window.clipboard().set_text(&p.path);
            self.show_toast("Copied the path");
        }
    }

    /// Copy the photo itself (as a picture, and as a file for file managers), or the
    /// selected files when there are several.
    fn copy_image(self: &Rc<Self>) {
        let (paths, video) = {
            let s = self.state.borrow();
            let paths: Vec<String> = s.photos_by_id(&s.target_ids()).iter().map(|p| p.path.clone()).collect();
            (paths, s.cursor_photo().is_some_and(|p| p.is_video()))
        };
        let a = self.clone();
        crate::actions::copy(&self.window.clipboard(), paths, !video, move |m| a.show_toast(&m));
    }

    /// The Share Sheet for the photo, or the selected ones.
    fn share(self: &Rc<Self>) {
        let paths: Vec<String> = {
            let s = self.state.borrow();
            s.photos_by_id(&s.target_ids()).iter().map(|p| p.path.clone()).collect()
        };
        if paths.is_empty() {
            return;
        }
        let a = self.clone();
        crate::share::files(paths, move |r| {
            if let Err(e) = r {
                a.show_toast(&e);
            }
        });
    }

    fn set_background(self: &Rc<Self>) {
        let Some(path) = self.state.borrow().cursor_photo().filter(|p| !p.is_video()).map(|p| p.path.clone()) else { return };
        self.show_toast(crate::actions::set_background(&path));
    }

    /// Rename the photo's file, in a small dialog.
    fn rename(self: &Rc<Self>) {
        let Some((id, name)) = self.state.borrow().cursor_photo().map(|p| (p.id.clone(), p.file_name.clone())) else { return };
        let a = self.clone();
        dialog::ask_text("Rename", "The file is renamed in its folder.", &name, "Rename", move |new| {
            let result = a.state.borrow_mut().lib.rename(&id, &new);
            match result {
                Ok(new_id) => {
                    a.refresh(false);
                    if let Some(i) = a.state.borrow().index_of(&new_id) {
                        a.state.borrow_mut().select_single(i);
                    }
                    a.refresh_selection(true);
                    if a.lightbox.is_open() {
                        a.show_in_viewer();
                    }
                }
                Err(e) => a.show_toast(&format!("Couldn't rename: {e:#}")),
            }
        });
    }

    /// Export copies of the selection, smaller or in another format.
    fn export(self: &Rc<Self>) {
        let files: Vec<PathBuf> = {
            let s = self.state.borrow();
            s.photos_by_id(&s.target_ids()).iter().map(|p| PathBuf::from(&p.path)).collect()
        };
        if files.is_empty() {
            return;
        }
        let a = self.clone();
        dialog::ask_export(files.len(), move |opts, dest| {
            let (files, d) = (files.clone(), dest.clone());
            a.show_toast(&format!("Exporting {} {}…", files.len(), if files.len() == 1 { "photo" } else { "photos" }));
            let a2 = a.clone();
            cmd::background(
                move || crate::export::export(&files, &d, &opts),
                move |report| {
                    let n = report.written;
                    let mut message =
                        format!("Exported {n} {} to {}", if n == 1 { "photo" } else { "photos" }, paths::pretty(&dest));
                    if !report.failed.is_empty() {
                        message.push_str(&format!(". {} failed: {}", report.failed.len(), report.failed[0]));
                    }
                    let dir = dest.to_string_lossy().to_string();
                    a2.show_toast_with_action(&message, "Show", move || library::reveal(&format!("{dir}/")));
                },
            );
        });
    }

    fn add_to_album(self: &Rc<Self>, album: &str) {
        let ids = self.state.borrow().target_ids();
        let result = self.state.borrow_mut().lib.add_to_album(album, &ids);
        match result {
            Ok(added) => {
                self.update_counts();
                self.show_toast(&format!("Added {added} {} to the album", if added == 1 { "photo" } else { "photos" }));
            }
            Err(e) => self.show_toast(&format!("Couldn't save the album: {e}")),
        }
    }

    /// Make an album from the selection, then ask for its name.
    fn add_to_new_album(self: &Rc<Self>) {
        let n = self.state.borrow().lib.albums.len() + 1;
        let created = self.state.borrow_mut().lib.create_album(&format!("New album {n}"));
        match created {
            Ok(id) => {
                self.add_to_album(&id);
                self.build_albums();
                self.rename_album(&id);
            }
            Err(e) => self.show_toast(&format!("Couldn't create the album: {e}")),
        }
    }

    fn remove_from_album(self: &Rc<Self>) {
        let (album, ids) = {
            let s = self.state.borrow();
            (s.album_id().map(String::from), s.target_ids())
        };
        let Some(album) = album else { return };
        for id in ids {
            if let Err(e) = self.state.borrow_mut().lib.remove_from_album(&album, &id) {
                self.show_toast(&format!("Couldn't save the album: {e}"));
                return;
            }
        }
        self.refresh(false);
        self.show_toast("Removed from the album");
    }

    fn new_album(self: &Rc<Self>) {
        let n = self.state.borrow().lib.albums.len() + 1;
        let created = self.state.borrow_mut().lib.create_album(&format!("New album {n}"));
        match created {
            Ok(id) => {
                self.build_albums();
                self.navigate(&id);
                self.rename_album(&id);
            }
            Err(e) => self.show_toast(&format!("Couldn't create the album: {e}")),
        }
    }

    fn rename_album(self: &Rc<Self>, id: &str) {
        let Some(name) = self.state.borrow().lib.album(id).map(|a| a.name.clone()) else { return };
        let (a, id) = (self.clone(), id.to_string());
        dialog::ask_text("Rename album", "", &name, "Save", move |new| {
            if let Err(e) = a.state.borrow_mut().lib.rename_album(&id, &new) {
                a.show_toast(&format!("Couldn't save the album: {e}"));
                return;
            }
            a.build_albums();
            a.refresh(false);
        });
    }

    /// Delete an album (its photos stay). The message can undo it.
    fn delete_album(self: &Rc<Self>, id: &str) {
        let deleted = self.state.borrow_mut().lib.delete_album(id);
        let (album, at) = match deleted {
            Ok(Some(d)) => d,
            Ok(None) => return,
            Err(e) => {
                self.show_toast(&format!("Couldn't delete the album: {e}"));
                return;
            }
        };
        self.build_albums();
        if self.state.borrow().section == id {
            self.navigate("all_photos");
        }
        let message = format!("Deleted the album “{}”. Its photos are still in the library", album.name);
        let a = self.clone();
        self.show_toast_with_action(&message, "Undo", move || {
            let id = album.id.clone();
            if let Err(e) = a.state.borrow_mut().lib.restore_album(album.clone(), at) {
                a.show_toast(&format!("Couldn't save the album: {e}"));
                return;
            }
            a.build_albums();
            a.navigate(&id);
        });
    }

    /// The right-click menu of an album in the sidebar.
    fn show_album_menu(self: &Rc<Self>, id: &str, anchor: &gtk::Widget, x: f64, y: f64) {
        let pop = gtk::Popover::new();
        pop.set_parent(anchor);
        pop.set_has_arrow(false);
        pop.set_halign(gtk::Align::Start);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let list = vbox(2);
        list.set_size_request(220, -1);
        let empty = self.state.borrow().count(id).unwrap_or(0) == 0;
        type Run = Box<dyn Fn(&Rc<App>, &str)>;
        let add = |icon: &str, text: &str, run: Run| {
            let (a, id, run) = (self.clone(), id.to_string(), Rc::new(run));
            // Run after the menu has let go of the sidebar item, which deleting rebuilds.
            let b = menu_item(&pop, icon, text, "", move || {
                let (a, id, run) = (a.clone(), id.clone(), run.clone());
                glib::idle_add_local_once(move || run(&a, &id));
            });
            list.append(&b);
            b
        };
        add("folder-open-symbolic", "Open", Box::new(|a, id| a.navigate(id)));
        add("document-edit-symbolic", "Rename…", Box::new(|a, id| a.rename_album(id)));
        if !empty {
            // Both act on the whole album: open it and select everything first.
            let whole = |a: &Rc<App>, id: &str| {
                a.navigate(id);
                a.state.borrow_mut().select_all();
                a.refresh_selection(false);
            };
            add(
                "media-playback-start-symbolic",
                "Slideshow",
                Box::new(move |a, id| {
                    a.navigate(id);
                    a.state.borrow_mut().move_to(0, false);
                    a.start_slideshow();
                }),
            );
            add(
                "document-save-as-symbolic",
                "Export all…",
                Box::new(move |a, id| {
                    whole(a, id);
                    a.export();
                }),
            );
        }
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let delete = add("user-trash-symbolic", "Delete album", Box::new(|a, id| a.delete_album(id)));
        delete.add_css_class("danger");
        delete.set_tooltip_text(Some("The photos stay in the library"));
        pop.set_child(Some(&list));
        pop.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        pop.popup();
    }

    fn on_inspector(self: &Rc<Self>, action: inspector::Action) {
        use inspector::Action::*;
        match action {
            Close => self.toggle_inspector(),
            Open => self.open_viewer(),
            Reveal => self.reveal(),
            RemoveFromAlbum(album) => {
                let Some(id) = self.state.borrow().cursor.clone() else { return };
                if let Err(e) = self.state.borrow_mut().lib.remove_from_album(&album, &id) {
                    self.show_toast(&format!("Couldn't save the album: {e}"));
                    return;
                }
                self.refresh(false);
                self.show_toast("Removed from the album");
            }
        }
    }

    /// The toolbar's album menu: every album, then a new one.
    fn fill_album_menu(self: &Rc<Self>, button: &gtk::MenuButton) {
        let pop = gtk::Popover::new();
        let list = vbox(2);
        list.set_size_request(220, -1);
        let albums: Vec<(String, String)> =
            self.state.borrow().lib.albums.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
        let names = vbox(2);
        for (id, name) in albums {
            let a = self.clone();
            names.append(&menu_item(&pop, "folder-symbolic", &name, "", move || a.add_to_album(&id)));
        }
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(260)
            .child(&names)
            .build();
        list.append(&scroll);
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let a = self.clone();
        list.append(&menu_item(&pop, "list-add-symbolic", "New album…", "", move || a.add_to_new_album()));
        if self.state.borrow().album_id().is_some() {
            let a = self.clone();
            list.append(&menu_item(&pop, "list-remove-symbolic", "Remove from this album", "", move || a.remove_from_album()));
        }
        pop.set_child(Some(&list));
        button.set_popover(Some(&pop));
    }

    /// The toolbar's ⋯ menu: the less frequent things to do with the selection, and the
    /// library. In a narrow window it also holds the actions the toolbar has no room for.
    fn fill_more_menu(self: &Rc<Self>, button: &gtk::MenuButton) {
        let pop = gtk::Popover::new();
        let list = vbox(2);
        list.set_size_request(250, -1);
        let (count, single, video, any, all_fav, in_album) = {
            let s = self.state.borrow();
            let ids = s.target_ids();
            let video = s.cursor_photo().is_some_and(|p| p.is_video());
            let all_fav = !ids.is_empty() && ids.iter().all(|id| s.lib.is_favorite(id));
            (ids.len(), ids.len() == 1, video, !ids.is_empty(), all_fav, s.album_id().is_some())
        };
        type Run = Box<dyn Fn(&Rc<App>)>;
        let add = |list: &gtk::Box, icon: &str, text: &str, keys: &str, run: Run| {
            let a = self.clone();
            list.append(&menu_item(&pop, icon, text, keys, move || run(&a)));
        };
        if self.narrow.get() && any {
            let fav = if all_fav { "Remove from favorites" } else { "Add to favorites" };
            add(
                &list,
                if all_fav { "starred-symbolic" } else { "non-starred-symbolic" },
                fav,
                "F",
                Box::new(|a| a.toggle_favorite()),
            );
            if single && !video {
                add(&list, "document-edit-symbolic", "Edit", "E", Box::new(|a| a.open_editor()));
                add(&list, "object-rotate-right-symbolic", "Rotate", "R", Box::new(|a| a.rotate()));
            }
            add(&list, "list-add-symbolic", "Add to a new album…", "", Box::new(|a| a.add_to_new_album()));
            if in_album {
                add(&list, "list-remove-symbolic", "Remove from this album", "", Box::new(|a| a.remove_from_album()));
            }
            add(&list, "user-trash-symbolic", "Move to trash", "Delete", Box::new(|a| a.delete_selected()));
            list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        if any {
            if single && !video {
                add(&list, "edit-copy-symbolic", "Copy image", "Ctrl+C", Box::new(|a| a.copy_image()));
            } else {
                let text = if single { "Copy file".to_string() } else { format!("Copy {count} files") };
                add(&list, "edit-copy-symbolic", &text, "Ctrl+C", Box::new(|a| a.copy_image()));
            }
            if single {
                add(&list, "edit-copy-symbolic", "Copy path", "", Box::new(|a| a.copy_path()));
                add(&list, "document-edit-symbolic", "Rename…", "F2", Box::new(|a| a.rename()));
            }
            let export = if single { "Export…".to_string() } else { format!("Export {count} photos…") };
            add(&list, "document-save-as-symbolic", &export, "", Box::new(|a| a.export()));
            add(&list, "send-to-symbolic", "Share…", "", Box::new(|a| a.share()));
            if single && !video && crate::actions::can_set_background() {
                add(&list, "preferences-desktop-wallpaper-symbolic", "Set as background", "", Box::new(|a| a.set_background()));
            }
            add(&list, "media-playback-start-symbolic", "Slideshow", "", Box::new(|a| a.start_slideshow()));
            add(&list, "folder-open-symbolic", "Reveal in file manager", "Ctrl+O", Box::new(|a| a.reveal()));
            list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        add(
            &list,
            "edit-select-all-symbolic",
            "Select all",
            "Ctrl+A",
            Box::new(|a| {
                a.state.borrow_mut().select_all();
                a.refresh_selection(false);
            }),
        );
        add(&list, "view-refresh-symbolic", "Scan the folders again", "", Box::new(|a| a.rescan(false)));
        add(
            &list,
            "input-keyboard-symbolic",
            "Keyboard shortcuts",
            "?",
            Box::new(|_| dialog::show_shortcuts(settings_view::SHORTCUTS)),
        );
        pop.set_child(Some(&list));
        button.set_popover(Some(&pop));
    }

    fn on_lightbox(self: &Rc<Self>, action: lightbox::Action) {
        use lightbox::Action::*;
        match action {
            Close => self.close_viewer(),
            Prev => self.step_viewer(-1),
            Next => self.step_viewer(1),
            Rotate => self.rotate(),
            Reveal => self.reveal(),
            ToggleFavorite => self.toggle_favorite(),
            Edit => self.open_editor(),
            Play => {
                if let Some(p) = self.state.borrow().cursor_photo() {
                    cmd::spawn(&["xdg-open", &p.path]);
                }
            }
            Slideshow => self.toggle_slideshow(),
            SaveRotation => self.save_rotation(),
            ResetRotation => self.reset_rotation(),
            Wallpaper => self.set_background(),
            CopyImage => self.copy_image(),
            Share => self.share(),
            OpenGallery => {}
            Jump(i) => {
                self.lightbox.stop_slideshow();
                self.state.borrow_mut().select_single(i);
                self.show_in_viewer();
                self.refresh_selection(false);
            }
        }
    }

    // ----- Inspector, sidebar and sizes -----

    fn toggle_inspector(self: &Rc<Self>) {
        prefs::update(|p| p.inspector_open = !p.inspector_open);
        self.apply_inspector_visibility();
    }

    /// The info drawer, beside the grid below the bar: the grid makes room for it.
    fn apply_inspector_visibility(&self) {
        // A half-screen window has no room for it: the grid keeps the width.
        let show = prefs::get().inspector_open && !self.narrow.get();
        self.info_button.set_visible(!self.narrow.get());
        self.inspector.root.set_visible(show);
        if show {
            self.info_button.add_css_class("on");
        } else {
            self.info_button.remove_css_class("on");
        }
    }

    fn toggle_sidebar(self: &Rc<Self>) {
        prefs::update(|p| p.sidebar_collapsed = !p.sidebar_collapsed);
        self.apply_compact(self.narrow.get() || prefs::get().sidebar_collapsed);
    }

    fn focus_search(self: &Rc<Self>) {
        self.search.grab_focus();
    }

    /// Hand the keyboard back to the gallery.
    fn leave_search(&self) {
        GtkWindowExt::set_focus(&self.window, None::<&gtk::Widget>);
    }

    fn apply_width(self: &Rc<Self>) {
        let w = &self.window;
        let width = if w.width() > 0 { w.width() } else { w.default_width() };
        if width <= 0 {
            return;
        }
        settings_dialog::fit(w);
        let narrow = width < NARROW_BELOW;
        let first = !self.nav.has_css_class("sized");
        if narrow == self.narrow.get() && !first {
            return;
        }
        self.nav.add_css_class("sized");
        self.narrow.set(narrow);
        self.gallery.set_narrow(narrow);
        self.zoom_inline.set_visible(!narrow);
        self.zoom_menu.set_visible(narrow);
        self.quick_actions.set_visible(!narrow);
        if narrow {
            self.gallery_page.add_css_class("narrow");
        } else {
            self.gallery_page.remove_css_class("narrow");
        }
        self.apply_compact(narrow || prefs::get().sidebar_collapsed);
        self.apply_inspector_visibility();
    }

    /// The sidebar shows only icons: hide the labels, centre the icons and the toggle.
    fn apply_compact(&self, compact: bool) {
        if compact {
            self.nav.add_css_class("compact");
        } else {
            self.nav.remove_css_class("compact");
        }
        self.compact.set(compact);
        for w in self.compact_items.borrow().iter() {
            w.set_visible(!compact);
        }
        self.collapse_button.set_tooltip_text(Some(if compact {
            "Expand the sidebar (Ctrl+B)"
        } else {
            "Collapse the sidebar (Ctrl+B)"
        }));
        centre_icons(self.nav.upcast_ref(), compact);
    }

    // ----- Toasts -----

    fn show_toast(self: &Rc<Self>, message: &str) {
        self.toast(message, None, Duration::from_millis(3500));
    }

    /// A toast with a button (Undo, Show…) that runs `action` and closes it.
    fn show_toast_with_action(self: &Rc<Self>, message: &str, button: &str, action: impl Fn() + 'static) {
        self.toast(message, Some((button, Rc::new(action))), UNDO_TOAST);
    }

    fn toast(self: &Rc<Self>, message: &str, action: Option<widgets::ButtonAction>, stay: Duration) {
        if let Some(old) = self.toast.borrow_mut().take() {
            self.overlay.remove_overlay(&old);
        }
        let l = gtk::Label::new(Some(message));
        l.set_wrap(true);
        l.set_max_width_chars(70);
        let bx = hbox(12);
        bx.add_css_class("toast");
        bx.append(&l);
        let motion = !prefs::get().reduce_motion;
        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .transition_duration(if motion { 180 } else { 0 })
            .child(&bx)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .build();
        // Only a toast with a button takes clicks; others let them through.
        revealer.set_can_target(action.is_some());
        if let Some((text, run)) = action {
            let b = gtk::Button::with_label(text);
            b.add_css_class("toast-action");
            let (a, r) = (self.clone(), revealer.clone());
            b.connect_clicked(move |_| {
                run();
                a.dismiss_toast(&r);
            });
            bx.append(&b);
        }
        self.overlay.add_overlay(&revealer);
        revealer.set_reveal_child(true);
        *self.toast.borrow_mut() = Some(revealer.clone());
        let a = self.clone();
        glib::timeout_add_local_once(stay, move || a.dismiss_toast(&revealer));
    }

    fn dismiss_toast(self: &Rc<Self>, revealer: &gtk::Revealer) {
        // Only if it hasn't been replaced already.
        if self.toast.borrow().as_ref() != Some(revealer) {
            return;
        }
        let motion = !prefs::get().reduce_motion;
        revealer.set_reveal_child(false);
        let (a, revealer) = (self.clone(), revealer.clone());
        glib::timeout_add_local_once(Duration::from_millis(if motion { 200 } else { 0 }), move || {
            let current = a.toast.borrow().as_ref() == Some(&revealer);
            if current {
                *a.toast.borrow_mut() = None;
                a.overlay.remove_overlay(&revealer);
            }
        });
    }
}

/// A row in a popover menu: icon, text, keys; closes the menu and runs `run`.
fn menu_item(pop: &gtk::Popover, icon: &str, text: &str, keys: &str, run: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("flat");
    b.add_css_class("action-button");
    let content = hbox(8);
    content.append(&gtk::Image::from_icon_name(icon));
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_hexpand(true);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&l);
    if !keys.is_empty() {
        content.append(&widgets::key_combo(keys));
    }
    b.set_child(Some(&content));
    let pop = pop.clone();
    b.connect_clicked(move |_| {
        pop.popdown();
        run();
    });
    b
}

/// What the grid needs to draw a photo.
fn cell_data(p: &crate::models::PhotoItem) -> CellData {
    CellData {
        thumb: p.thumbnail_path.clone(),
        source: p.path.clone(),
        mtime: p.mtime,
        favorite: p.is_favorite,
        badge: p.is_video().then(|| p.duration.map(crate::models::format_duration).unwrap_or_else(|| p.format.clone())),
        video: p.is_video(),
        aspect: p.aspect(),
    }
}

/// 4812 → "4,812".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn centre_icons(w: &gtk::Widget, compact: bool) {
    if w.has_css_class("nav-item")
        && let Some(content) = w.downcast_ref::<gtk::Button>().and_then(|b| b.child())
    {
        content.set_halign(if compact { gtk::Align::Center } else { gtk::Align::Fill });
    }
    if w.has_css_class("nav-collapse") && w.parent().is_some_and(|p| p.has_css_class("nav-group-row"))
    {
        w.set_halign(if compact { gtk::Align::Center } else { gtk::Align::End });
        w.set_hexpand(compact);
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        centre_icons(&c, compact);
        child = c.next_sibling();
    }
}
