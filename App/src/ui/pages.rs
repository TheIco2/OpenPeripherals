use std::path::PathBuf;

/// Directory where page HTML files are stored.
fn pages_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pages")
}

pub fn devices_page() -> PathBuf {
    pages_dir().join("devices.html")
}

pub fn addons_page() -> PathBuf {
    pages_dir().join("addons.html")
}

pub fn ai_learning_page() -> PathBuf {
    pages_dir().join("ai_learning.html")
}

pub fn profiles_page() -> PathBuf {
    pages_dir().join("profiles.html")
}

pub fn settings_page() -> PathBuf {
    pages_dir().join("settings.html")
}
