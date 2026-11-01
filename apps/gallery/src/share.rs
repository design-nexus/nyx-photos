//! Sharing through Nyx Share: the Share service opens its Share Sheet for the
//! photos and videos, sent as read-only descriptors. The files aren't changed.

use nyx_share_client::{Client, Content, ShareOptions};

/// Open the Share Sheet for these files; `done` gets what went wrong, if anything.
pub fn files(paths: Vec<String>, done: impl FnOnce(Result<(), String>) + 'static) {
    crate::cmd::background(
        move || {
            let mut content = Content::new();
            for p in &paths {
                content.file(p).map_err(|e| e.to_string())?;
            }
            let options = ShareOptions { app_name: Some("Photos".into()), ..Default::default() };
            Client::connect().and_then(|c| c.share(&content, &options)).map(|_| ()).map_err(|e| e.to_string())
        },
        done,
    );
}
