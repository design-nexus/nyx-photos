//! Photos — a fast, keyboard-first photo gallery for Omarchy.

use gtk::prelude::*;
use gtk::{gio, glib};
use nyx_photos::{control, library, window};

const APP_ID: &str = "io.github.design_nexus.nyx.Photos";

const USAGE: &str = "Usage: nyx-photos [--section ID] [--edit] [FILE]

  FILE          open this photo or video (its folder is shown if it isn't in the library)
  --edit        with FILE, open it straight in the editor
  --section ID  open a page: all_photos, favorites, recent, folders, duplicates,
                photos, screenshots, videos, settings
  -h, --help    show this help
  -V, --version print the version
";

fn main() -> glib::ExitCode {
    nyx_core::app::init(nyx_core::app::App { slug: "nyx-photos", name: "Photos" });
    nyx_core::migrate::from(&["nexus-photos"]);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return glib::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("nyx-photos {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }

    // One window: launching again hands the arguments to the open one.
    let mut flags = gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    // For trying a build beside a running copy without taking it over.
    if std::env::var_os("NYX_PHOTOS_STANDALONE").is_some() {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    // NYX_PHOTOS_APP_ID runs a test copy under its own ID, beside the real one.
    let id = std::env::var("NYX_PHOTOS_APP_ID").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| APP_ID.to_string());
    // Windows without an application, like dialogs, take their Wayland app ID from this.
    glib::set_prgname(Some(id.as_str()));
    let app = gtk::Application::builder().application_id(id.as_str()).flags(flags).build();
    // Nyx Assistant finds and shows photos through this (primary instance only).
    app.connect_startup(|app| nyx_core::control::serve(app, control::tools(), control::handle));
    app.connect_command_line(|app, cl| {
        let argv: Vec<String> = cl.arguments().iter().skip(1).map(|a| a.to_string_lossy().to_string()).collect();
        let section = argv.iter().position(|a| a == "--section").and_then(|i| argv.get(i + 1)).cloned();
        // Leave out the section's value so it isn't mistaken for a file.
        let edit = argv.iter().any(|a| a == "--edit");
        let rest: Vec<String> = argv
            .iter()
            .enumerate()
            .filter(|(i, a)| *a != "--section" && *a != "--edit" && !(*i > 0 && argv[*i - 1] == "--section"))
            .map(|(_, a)| a.clone())
            .collect();
        let file = library::file_argument(&rest);
        window::present(app, section.as_deref(), file, edit);
        glib::ExitCode::SUCCESS
    });
    app.run()
}
