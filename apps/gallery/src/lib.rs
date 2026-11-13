//! Photos: the gallery (`nyx-photos`) and the stand-alone viewer
//! (`nyx-viewer`, Photo Viewer) share these modules.

#![allow(clippy::new_without_default)]
pub mod actions;
pub use nyx_ui::clamp;
pub mod cmd;
pub mod control;
pub mod dialog;
pub mod edit;
pub mod editor;
pub mod export;
pub mod grid;
pub mod info_panel;
pub mod inspector;
pub mod library;
pub mod lightbox;
pub mod metadata;
pub mod models;
pub mod paths;
pub mod prefs;
pub mod recent;
pub mod scanner;
pub mod settings_dialog;
pub mod settings_view;
pub mod share;
pub mod stage;
pub mod state;
pub mod theme;
pub mod thumbnails;
pub mod widgets;
pub mod window;
