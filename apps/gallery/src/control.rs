//! Photos' tools on the control interface (`nyx_core::control`), for Nyx Assistant: find
//! photos by name, date, camera, album or favourite, list albums, and show a photo. Nothing
//! is moved, edited or deleted.

use crate::models::PhotoItem;
use crate::window;
use nyx_core::control::{self, text};
use serde_json::{Value, json};

pub fn tools() -> Value {
    json!([
        {"name": "photos.search", "description": "Find photos and videos in the Photos library by words (file name, camera, place), year, month, album or favourites.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}, "year": {"type": "integer"}, "month": {"type": "integer", "minimum": 1, "maximum": 12}, "album": {"type": "string"}, "favorites": {"type": "boolean"}}}},
        {"name": "photos.albums", "description": "The albums in Photos, with how many photos each holds.", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "photos.open", "description": "Show a photo (by its path) in the Photos viewer.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
    ])
}

fn item(p: &PhotoItem) -> Value {
    let camera = [p.camera_make.as_deref(), p.camera_model.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" ");
    json!({"name": p.file_name, "path": p.path, "taken": p.date_display, "size": [p.width, p.height], "camera": camera, "video": p.duration.is_some()})
}

pub fn handle(name: &str, args: Value) -> Result<Value, String> {
    match name {
        "photos.search" => window::with_library(|lib| {
            let words: Vec<String> = args["query"].as_str().unwrap_or("").split_whitespace().map(str::to_lowercase).collect();
            let album = match args["album"].as_str() {
                Some(a) => {
                    let a = a.to_lowercase();
                    Some(lib.albums.iter().find(|x| x.name.to_lowercase() == a).ok_or("no album of that name")?.photo_ids.clone())
                }
                None => None,
            };
            let found: Vec<&PhotoItem> = lib
                .photos
                .iter()
                .filter(|p| args["year"].as_i64().is_none_or(|y| p.year as i64 == y))
                .filter(|p| args["month"].as_i64().is_none_or(|m| p.month as i64 == m))
                .filter(|p| args["favorites"] != true || lib.favorites.contains(&p.id))
                .filter(|p| album.as_ref().is_none_or(|ids| ids.contains(&p.id)))
                .filter(|p| {
                    let hay = format!("{} {} {}", p.file_name, p.camera_make.as_deref().unwrap_or(""), p.camera_model.as_deref().unwrap_or("")).to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                })
                .collect();
            // Newest first.
            let mut found = found;
            found.sort_by_key(|p| std::cmp::Reverse(p.taken));
            Ok(json!({"count": found.len(), "photos": found.iter().take(30).map(|p| item(p)).collect::<Vec<_>>()}))
        })
        .unwrap_or(Err("Photos isn't ready".into())),
        "photos.albums" => window::with_library(|lib| {
            Ok(json!({"albums": lib.albums.iter().map(|a| json!({"name": a.name, "photos": a.photo_ids.len()})).collect::<Vec<_>>()}))
        })
        .unwrap_or(Err("Photos isn't ready".into())),
        "photos.open" => {
            let path = control::home_path(&text(&args, "path")?)?;
            if !path.is_file() {
                return Err("no such file".into());
            }
            if !window::open_photo(path.clone()) {
                return Err("Photos isn't ready".into());
            }
            Ok(json!({"opened": path}))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}
