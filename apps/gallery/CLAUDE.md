# Photos — notes for working on this repo

- GTK4 (gtk4-rs 0.11) + Rust, no libadwaita. Nyx 2.0 (`nyx_theme::use_design_2`): one
  blended surface. The header (`chrome::app_header`) has the sidebar toggle, search in
  the middle, then album actions, Info, the ⋯ menu (which also holds the main actions for
  one photo) and Settings. Below: the sidebar (Library, Tidy up and Albums under
  `nyx_ui::sidebar` headings in their section colours, coloured bars between them on the
  icon rail, the scan's status at its foot), the timeline at full width with a slim
  scrubber of months and years, the photo info (I) as a card over it beside its photo or,
  as Settings chooses (`info_style`), a panel down its right (a narrow window shows the
  card), a bar of labelled actions floating at its foot while several photos are picked, and
  the foot (counts, Group by, the layout switch, thumbnail size). Settings is a card over the window
  (`settings_dialog.rs`), rebuilt from `settings_view::build` each time it opens, listing
  its groups (named by `widgets::group`).
  Every colour in `src/style.css` is a `@theme_*` token; `src/theme.rs` loads the
  palette and the stylesheet as one provider, so a theme change restyles live.
- Layout: `window.rs` owns the `App` (sidebar, pages, toasts, key handling and all
  actions; the floating bar, the album and ⋯ menus, built when opened, and where the info
  card goes: `place_info`, from `Gallery::cell_bounds`). `state.rs` is plain data (section, search, timeline groups, selection) with
  the unit tests. `grid.rs` is the thumbnail grid: a virtualised `ListView` whose rows are
  headings or rows of thumbnails. `lightbox.rs` is the viewer, `inspector.rs` the info
  card (its links: Map, Show in Files, Add to album), `info_panel.rs` the info panel (preview,
  size and camera with a histogram, a drawn map, folder and album chips), `settings_view.rs` the settings page (shown in `settings_dialog.rs`), `library.rs` scanning, favourites and
  albums (atomic writes).
- Two binaries share one library (`lib.rs`): `nyx-photos` (`main.rs`) and `nyx-viewer`, Photo
  Viewer (`src/bin/nyx-viewer.rs`, one folder of photos, hands off to the gallery with `nyx-photos [--edit] FILE`).
  `nyx-viewer` is `Lightbox` in `Mode::Standalone`, so viewer changes land in both. With no photo, `nyx-viewer` shows its
  home page (`home_page`: drop zone and the recent photos from `recent.rs`, `~/.local/share/nyx-photos/recent.json`). It reads favourites and
  rotations fresh from disk for every change; the gallery re-reads them when its window is activated.
  `actions.rs` holds what both do to files (copy, trash, save rotation, set background).
  `stage.rs` is the picture area (fit/zoom/pan/pinch) used by both viewers. The `open_with` pref
  (`builtin`/`viewer`) picks what a double-click in the gallery opens; CLI file arguments always use the built-in viewer.
- Editing: `src/edit/` has no widgets. `mod.rs` is `EditState` (rotation, flips, crop, resize,
  adjustments, markup) and `History` (undo/redo); `crop.rs` is the crop-box maths in pixel
  units; `pipeline.rs` renders a state onto pixels in a fixed order (flips, rotation, crop,
  resize, adjustments, blur, markup) and encodes; `marks.rs` draws markup with cairo/pango from
  normalised coordinates, so the live preview and the saved file match; `save.rs` writes a
  copy or replaces the original (backup first, atomic). `editor.rs` is the overlay: the canvas,
  gestures, panels and the save/cancel flows. The preview renders a ≤2048 px copy on a worker
  thread; only blur changes preview pixels, other marks are drawn live over it. Markup and
  geometry exclude each other (marks are placed on the cropped picture).
- Thumbnails are made on demand: a scan only reads metadata (`metadata.rs`), and the grid
  calls `thumbnails::generate_thumbnail` on a worker thread when a cell is shown. They are
  512 px, upright by EXIF, written under a hidden name and renamed (never read half-made),
  and pruned after each library scan. `grid::thumbnail_texture` is the shared LRU of decoded ones.
- Ids are `models::photo_id` (FNV-1a of the path, stable across builds). `legacy_photo_id`
  (std `DefaultHasher`) only exists so `Library::migrate_ids` can move old favourites, albums
  and rotations over. Dates are `taken`: wall-clock seconds (`metadata::naive`); sort and
  filter on that, never on `date_display`.
- Scanning: `scanner::scan` reuses the previous `PhotoItem` when a file's size and mtime
  match. The result is saved to `~/.cache/nyx-photos/library.json` and loaded at start-up.
  `window.rs` watches every scanned folder with `gio::FileMonitor` and rescans 1.5 s after a
  change. Library folders are the `folders` pref (empty = `~/Pictures`).
- `grid.rs` rows are `Row::Cells { start, height, widths }`: squares or justified (`justify`).
  The scrubber's labels (`scrub_marks`: the latest year's months, then each earlier year and
  its months; `fit_marks` drops what doesn't fit) come from `row_labels` (`Span::short`/`shorts`)
  and sit evenly down the column; a click or drag between two goes to the part of the list
  between their rows (`row_tops`, estimated heights, scaled to the scroll adjustment).
- Preferences: `~/.config/nyx-photos/settings.toml` (`prefs.rs`, debounced writes).
  Albums, favourites and view rotations: `~/.local/share/nyx-photos/`. `export.rs` writes
  smaller/converted copies; deleting trashes with an Undo toast (`library::untrash`). User themes:
  `~/.config/nyx-photos/themes/*.toml`.
- Control (`src/control.rs`): `nyx_core::control` on the Photos bus name, for Nyx Assistant: `photos.search`, `albums`, `open` (`window::with_library`, `window::open_photo`).
- Checks: `cargo clippy -- -D warnings`, `cargo test`. Every path honours the XDG
  variables, so `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME` can point at a
  scratch directory to try the app without touching real data (with
  `NYX_PHOTOS_STANDALONE=1` beside a running copy; `NYX_PHOTOS_APP_ID` gives it its own
  app ID, and `NYX_PHOTOS_TEST_PICK=0,1,3` picks those photos after the scan, for screenshots).
