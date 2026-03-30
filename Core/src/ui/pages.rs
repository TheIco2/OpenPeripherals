use std::path::PathBuf;

/// Directory where page HTML files are stored.
fn pages_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pages")
}

pub fn base_page() -> PathBuf {
    pages_dir().join("base.html")
}

// Content fragment pages — loaded into <page-content> at runtime.
#[allow(dead_code)]
pub fn devices_page() -> PathBuf {
    pages_dir().join("devices.html")
}

#[allow(dead_code)]
pub fn addons_page() -> PathBuf {
    pages_dir().join("addons.html")
}

#[allow(dead_code)]
pub fn ai_learning_page() -> PathBuf {
    pages_dir().join("ai_learning.html")
}

#[allow(dead_code)]
pub fn profiles_page() -> PathBuf {
    pages_dir().join("profiles.html")
}

#[allow(dead_code)]
pub fn settings_page() -> PathBuf {
    pages_dir().join("settings.html")
}

#[allow(dead_code)]
pub fn device_edit_page() -> PathBuf {
    pages_dir().join("device_edit.html")
}
