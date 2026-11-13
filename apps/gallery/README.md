# Photos

A fast, keyboard-first photo gallery for Omarchy, built with GTK4 and Rust. It follows the
Omarchy theme live, groups photos on a timeline, and shows EXIF details in a side drawer.

## Features

- **Timeline:** group by years, months or days, or show everything in one grid. Photos are dated by when they were taken (EXIF), else by the file's date.
- **Thumbnails:** squares, or rows that keep each photo's shape (Settings → Gallery and viewer). A slider, <kbd>Ctrl</kbd>+wheel or <kbd>Ctrl</kbd>+<kbd>+</kbd>/<kbd>−</kbd> sets their size.
- **Date scrubber:** the right edge shows where you are; hover it for the years and drag it to jump.
- **Sections:** all photos, favorites, recently added (last 30 days), folders, possible duplicates, and photos, screenshots and videos by type.
- **Library folders:** look in any number of folders (Settings → Library). New, changed and deleted files show up by themselves. What a scan found is remembered, so the grid fills at once on the next start.
- **Toolbar:** Favorite, Edit, Rotate, Move to trash and an Album menu act on the selected photos; the ⋯ menu has the rest (copy, rename, export, set as background, slideshow, reveal, select all, rescan). In a narrow window they all move into ⋯.
- **Albums:** create albums with + in the sidebar; right-click one there to rename it, play it as a slideshow, export it or delete it (deleting keeps the photos, and can be undone). Add one photo or a whole selection to an album (or a new one) from the toolbar or the right-click menu. The photo info lists the albums a photo is in.
- **Search:** file name, folder, camera, lens or format, plus dates (`2024`, `2023-2024`, `2024-05`, `before:2024`, `after:2023-06`, `today`, `last week`, `this month`, `last year`) and kinds (`is:favorite`, `is:video`, `is:screenshot`, `is:photo`).
- **Viewer:** full-size view with zoom, pan, a filmstrip of nearby photos, and next/previous with the arrow keys. Neighboring photos load ahead. The controls fade while the pointer rests. <kbd>S</kbd> starts a slideshow.
- **Rotation:** <kbd>R</kbd> turns the view and remembers it; <kbd>Ctrl</kbd>+<kbd>S</kbd> or "Save to file" writes it into the photo (backing up the original first if Settings says so).
- **Files:** move to the trash with <kbd>Delete</kbd> (Undo in the message puts it back), rename with <kbd>F2</kbd>, copy the image with <kbd>Ctrl</kbd>+<kbd>C</kbd>, drag photos out to other apps, set one as the Omarchy background, and export a selection as smaller copies or in another format.
- **Videos:** show their length, and play silently while the pointer rests on them.
- **Photo Viewer:** `nyx-viewer FILE` opens one photo (and its folder) in a light window, with the same controls as the gallery's viewer — favorite, rotate and save the rotation, slideshow, copy, set as background, rename, trash with undo — plus Open in gallery and Edit in gallery. Favorites and rotations are shared with the gallery. Settings → Opening photos makes a double-click use it. Opened without a photo, it shows a drop zone and the photos it opened recently.
- **Editing:** rotate, flip, crop (free or fixed ratio), resize, brightness, contrast, saturation, Mono and Sepia filters, and markup (pen, highlighter, arrow, rectangle, ellipse, text and blur). Edits can be undone and redone, copied to the clipboard, and saved as a copy or over the original.
- **Photo info:** as a card over the photo or a panel on the right (Settings → Gallery and viewer): file details, a brightness histogram, camera and lens settings, the location (with a map link) when the photo has one, the folder, and the albums it is in.
- **Themes:** follows the current Omarchy theme as it changes, or pick one of 15 bundled themes or your own.
- **Narrow windows:** the sidebar collapses to icons when the window is narrow, so it fits tiled at half a screen.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/design-nexus/nyx-photos/main/install.sh | bash -s -- photos
```

This builds Photos with Cargo (installing Rust and GTK 4 with pacman if needed) and
installs the binary, desktop entry and icon under `~/.local`. From a clone, run `./install.sh`.

Thumbnails for videos need `ffmpegthumbnailer` or `ffmpeg`.

## Usage

```bash
nyx-photos                      # open the library (~/Pictures)
nyx-photos photo.jpg            # open a file in the viewer (its folder is shown if it isn't in the library)
nyx-viewer photo.jpg                     # open a file in Photo Viewer (on its own it shows recent photos)
nyx-photos --edit photo.jpg     # open a file straight in the editor
nyx-photos --section settings   # open a page: all_photos, favorites, recent, folders, duplicates, photos, screenshots, videos, settings
```

Launching it again focuses the open window. The photo info (<kbd>I</kbd>) is closed until you open it, and it remembers. Photos and videos are looked for in `~/Pictures`
and its subfolders until other folders are chosen in Settings; hidden folders are skipped. A file
outside the library folders opens with its own folder shown instead.

HEIC and AVIF photos need a gdk-pixbuf loader for them (glycin, or libheif's loader).

## Keyboard shortcuts

Press <kbd>F1</kbd> (or <kbd>?</kbd>) in the app to see them all.

| Keys | Action |
| :--- | :--- |
| <kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Move between photos (with <kbd>Shift</kbd>: select a range) |
| <kbd>Home</kbd> <kbd>End</kbd> <kbd>PgUp</kbd> <kbd>PgDn</kbd> | First, last, a page up or down |
| <kbd>Space</kbd> or <kbd>Enter</kbd> | Open the viewer |
| <kbd>Esc</kbd> | Close the viewer, clear the search, or deselect |
| <kbd>F</kbd> or <kbd>.</kbd> | Favorite |
| <kbd>E</kbd> | Edit the photo |
| <kbd>R</kbd> | Rotate the view 90° |
| <kbd>F2</kbd> | Rename |
| <kbd>Delete</kbd> | Move to the trash |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> | Copy the image (or the selected files) |
| <kbd>I</kbd> | Show or hide the photo info |
| <kbd>Ctrl</kbd>+<kbd>A</kbd> | Select every photo |
| <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>Ctrl</kbd>+<kbd>−</kbd> | Bigger or smaller thumbnails |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> or <kbd>Ctrl</kbd>+<kbd>K</kbd> | Search |
| <kbd>Ctrl</kbd>+<kbd>B</kbd> | Collapse or expand the sidebar |
| <kbd>Ctrl</kbd>+<kbd>O</kbd> | Reveal in the file manager |
| <kbd>+</kbd> <kbd>-</kbd> <kbd>0</kbd> <kbd>1</kbd> | Viewer: zoom in, zoom out, fit, 100% |
| <kbd>S</kbd> | Viewer: start or stop the slideshow |
| <kbd>W</kbd> | Viewer: set as the Omarchy background |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | Viewer and gallery: save the rotation to the file. Editor: save |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> | Editor: undo / redo |
| <kbd>F1</kbd> or <kbd>?</kbd> | Show the shortcuts |
| <kbd>Ctrl</kbd>+<kbd>Q</kbd> or <kbd>Ctrl</kbd>+<kbd>W</kbd> | Close |
