//! Small building blocks shared by the views: labels, option rows, cards, buttons.

pub use nyx_ui::widgets::*;

use crate::{cmd, paths};
use gtk::prelude::*;
use gtk::{glib, pango};
use std::path::PathBuf;
use std::rc::Rc;

pub fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    for c in class.split_whitespace() {
        l.add_css_class(c);
    }
    l.set_xalign(0.0);
    l
}

/// An icon and a label side by side, as a button's content.
pub fn button_content(icon: &str, text: &str) -> gtk::Box {
    let b = hbox(8);
    b.append(&gtk::Image::from_icon_name(icon));
    b.append(&gtk::Label::new(Some(text)));
    b
}

pub fn key_cap(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("key-cap");
    l.set_valign(gtk::Align::Center);
    l
}

/// Keys joined with a dim "+", e.g. `Ctrl+B`.
pub fn key_combo(keys: &str) -> gtk::Box {
    let b = hbox(4);
    for (i, k) in keys.split('+').enumerate() {
        if i > 0 {
            b.append(&label("+", "dim"));
        }
        b.append(&key_cap(k));
    }
    b
}

pub fn tag(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("tag");
    l.set_valign(gtk::Align::Center);
    l
}

/// Opens a config file; a menu picks one when there are several.
pub fn config_button(files: &[PathBuf]) -> gtk::Widget {
    if let [path] = files {
        let path = path.clone();
        let b = bar_button("text-editor-symbolic", &format!("Open {}", paths::pretty(&path)));
        b.connect_clicked(move |_| cmd::open_in_editor(&path));
        return b.upcast();
    }
    let menu = gtk::MenuButton::new();
    menu.set_icon_name("text-editor-symbolic");
    menu.add_css_class("bar-button");
    menu.set_valign(gtk::Align::Center);
    menu.set_tooltip_text(Some("Open a config file"));
    let list = vbox(2);
    let popover = gtk::Popover::new();
    for path in files {
        let b = gtk::Button::with_label(&paths::pretty(path));
        b.add_css_class("flat");
        if let Some(l) = b.child().and_downcast::<gtk::Label>() {
            l.set_xalign(0.0);
        }
        let (p, pop) = (path.clone(), popover.clone());
        b.connect_clicked(move |_| {
            pop.popdown();
            cmd::open_in_editor(&p);
        });
        list.append(&b);
    }
    popover.set_child(Some(&list));
    menu.set_popover(Some(&popover));
    menu.upcast()
}

// ---------- Groups and rows ----------

/// A group: a heading, an optional note, and one card holding its rows.
pub struct Group {
    pub wrapper: gtk::Box,
    pub list: gtk::Box,
}

impl Group {
    pub fn add(&self, w: &impl IsA<gtk::Widget>) {
        self.list.append(w);
    }
}

pub fn group(title: &str, note: &str) -> Group {
    let wrapper = vbox(0);
    wrapper.add_css_class("settings-group");
    // The settings dialog lists groups by this name.
    wrapper.set_widget_name(title);
    wrapper.append(&label(&title.to_uppercase(), "group-title"));
    if !note.is_empty() {
        let n = label(note, "group-note");
        n.set_wrap(true);
        wrapper.append(&n);
    }
    let list = vbox(0);
    list.add_css_class("settings-card");
    // 2.0: the group is one card and its rows sit plainly inside it.
    list.add_css_class("group-list");
    // Keeps a highlighted first or last row inside the rounded corners.
    list.set_overflow(gtk::Overflow::Hidden);
    wrapper.append(&list);
    Group { wrapper, list }
}

/// Title and description on the left, control on the right.
pub fn row(title: &str, desc: &str, control: Option<&gtk::Widget>) -> gtk::Box {
    let row = hbox(14);
    row.add_css_class("settings-option");
    row.add_css_class("card-row");
    let text = vbox(2);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let t = label(title, "settings-option-title");
    t.set_wrap(true);
    t.set_wrap_mode(pango::WrapMode::WordChar);
    text.append(&t);
    if !desc.is_empty() {
        let d = label(desc, "settings-option-description");
        d.set_wrap(true);
        d.set_wrap_mode(pango::WrapMode::WordChar);
        text.append(&d);
    }
    row.append(&text);
    if let Some(c) = control {
        c.set_valign(gtk::Align::Center);
        row.append(c);
    }
    row
}

pub fn switch_row(title: &str, desc: &str, active: bool, on_change: impl Fn(bool) + 'static) -> (gtk::Box, gtk::Switch) {
    let sw = gtk::Switch::new();
    // Without this a screen reader announces an unnamed switch.
    sw.update_property(&[gtk::accessible::Property::Label(title)]);
    sw.set_active(active);
    sw.connect_active_notify(move |s| on_change(s.is_active()));
    (row(title, desc, Some(sw.upcast_ref())), sw)
}

pub fn choice_row(
    title: &str,
    desc: &str,
    options: Vec<(String, String)>,
    current: &str,
    on_change: impl Fn(String) + 'static,
) -> (gtk::Box, gtk::DropDown) {
    let dd = dropdown(&options, current);
    dd.connect_selected_notify(move |d| {
        if let Some((id, _)) = options.get(d.selected() as usize) {
            on_change(id.clone());
        }
    });
    (row(title, desc, Some(dd.upcast_ref())), dd)
}

/// A drop-down of `(id, label)` options. The closed button ellipsizes a long choice
/// instead of growing to the longest option; the open list shows every option in full.
pub fn dropdown(options: &[(String, String)], current: &str) -> gtk::DropDown {
    let labels: Vec<&str> = options.iter().map(|(_, l)| l.as_str()).collect();
    let dd = gtk::DropDown::from_strings(&labels);
    dd.set_factory(Some(&string_factory(true)));
    dd.set_list_factory(Some(&string_factory(false)));
    match options.iter().position(|(id, _)| id == current) {
        Some(i) => dd.set_selected(i as u32),
        None => dd.set_selected(gtk::INVALID_LIST_POSITION),
    }
    dd
}

/// Labels for a drop-down; the open list's rows also check the current choice.
fn string_factory(short: bool) -> gtk::SignalListItemFactory {
    let f = gtk::SignalListItemFactory::new();
    f.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
        let l = gtk::Label::new(None);
        l.set_xalign(0.0);
        if short {
            l.set_ellipsize(pango::EllipsizeMode::End);
            l.set_width_chars(8);
            l.set_max_width_chars(30);
            item.set_child(Some(&l));
            return;
        }
        l.set_hexpand(true);
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(&l);
        row.append(&check);
        item.set_child(Some(&row));
        item.connect_selected_notify(move |it| check.set_opacity(if it.is_selected() { 1.0 } else { 0.0 }));
    });
    f.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
        let Some(s) = item.item().and_downcast::<gtk::StringObject>() else { return };
        let child = item.child();
        let label = child.as_ref().and_then(|c| c.downcast_ref::<gtk::Label>().cloned().or_else(|| c.first_child().and_downcast::<gtk::Label>()));
        if let Some(l) = label {
            l.set_text(&s.string());
        }
        if let Some(check) = child.and_then(|c| c.last_child()).and_downcast::<gtk::Image>() {
            check.set_opacity(if item.is_selected() { 1.0 } else { 0.0 });
        }
    });
    f
}

/// Buttons joined into one control; exactly one is selected.
pub fn segmented(options: &[(&str, &str)], current: &str, on_change: impl Fn(String) + 'static) -> gtk::Box {
    let bx = hbox(0);
    bx.add_css_class("segmented");
    bx.set_valign(gtk::Align::Center);
    let mut first: Option<gtk::ToggleButton> = None;
    let on_change = Rc::new(on_change);
    for (id, text) in options {
        let b = gtk::ToggleButton::with_label(text);
        b.add_css_class("segment");
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
        }
        b.set_active(*id == current);
        let (id, cb) = (id.to_string(), on_change.clone());
        b.connect_toggled(move |b| {
            if b.is_active() {
                cb(id.clone());
            }
        });
        bx.append(&b);
    }
    bx
}

/// A button that needs a second click within four seconds, for things that can't
/// be undone. The first click relabels it with `confirm` and arms it.
pub fn confirm_button(label: &str, confirm: &str, on_confirm: impl Fn(&gtk::Button) + 'static) -> gtk::Button {
    let b = gtk::Button::new();
    b.set_child(Some(&button_content("user-trash-symbolic", label)));
    let (label, confirm) = (label.to_string(), confirm.to_string());
    b.connect_clicked(move |b| {
        if !b.has_css_class("armed") {
            b.add_css_class("armed");
            b.set_child(Some(&button_content("user-trash-symbolic", &confirm)));
            let (weak, label) = (b.downgrade(), label.clone());
            glib::timeout_add_seconds_local_once(4, move || {
                if let Some(b) = weak.upgrade() {
                    b.remove_css_class("armed");
                    b.set_child(Some(&button_content("user-trash-symbolic", &label)));
                }
            });
            return;
        }
        b.remove_css_class("armed");
        on_confirm(b);
    });
    b
}

/// A button's label and what it does, for messages that may offer one.
pub type ButtonAction<'a> = (&'a str, Rc<dyn Fn()>);
