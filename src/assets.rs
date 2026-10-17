//! Versioned URLs for the files in `public/`, so a deploy that changes them
//! can never be hidden by a browser's cached copy.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::Path,
    sync::OnceLock,
};

static VERSION: OnceLock<String> = OnceLock::new();

/// Files the pages reference directly. The service worker keeps its fixed
/// URL: browsers check it for updates on their own.
const FILES: [&str; 7] = ["app.css", "app.js", "upload.js", "send.js", "open.js", "share-store.js", "manifest.json"];

/// Hashes the referenced files once at startup.
pub fn init(public_dir: &Path) {
    let mut hasher = DefaultHasher::new();
    for file in FILES {
        fs::read(public_dir.join(file)).unwrap_or_default().hash(&mut hasher);
    }
    let _ = VERSION.set(format!("{:x}", hasher.finish()));
}

/// `/app.css` → `/app.css?v=…`
pub fn url(path: &str) -> String {
    match VERSION.get() {
        Some(version) => format!("{path}?v={version}"),
        None => path.to_owned(),
    }
}
