#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use log::info;

use op_addon::AddonRegistry;
use op_core::device::DeviceRegistry;
use op_core::profile::ProfileStore;

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("OpenPeripheral")
}

fn main() -> Result<()> {
    op_core::logging::init("App", cfg!(debug_assertions));

    // Install a panic hook that logs the crash before the process terminates.
    // Without this, windows_subsystem = "windows" silently kills the process.
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".to_string()
        };
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        log::error!("PANIC at {}: {}", location, payload);
        // Also write to a crash file in case log isn't flushed.
        let crash_path = data_dir().join("crash.log");
        let _ = std::fs::write(&crash_path, format!("PANIC at {}: {}\n", location, payload));
    }));

    info!("OpenPeripheral starting...");

    let base = data_dir();
    let profiles_dir = base.join("profiles");
    let addons_dir = base.join("addons");

    // Initialize subsystems
    let device_registry = Arc::new(DeviceRegistry::new());
    let mut profile_store = ProfileStore::new(&profiles_dir);
    let mut addon_registry = AddonRegistry::new(&addons_dir);

    // Load existing profiles
    if let Err(e) = profile_store.load_all() {
        log::warn!("Failed to load some device profiles: {e}");
    }

    // Discover and load addons, then auto-register devices
    unsafe {
        let errors = addon_registry.discover_and_load();
        for e in &errors {
            log::warn!("Addon load error: {e}");
        }

        match addon_registry.auto_register(&device_registry) {
            Ok(count) => info!("Auto-registered {count} device(s) from addons"),
            Err(e) => log::warn!("Device enumeration failed: {e}"),
        }
    }

    info!(
        "Loaded {} addon(s), {} device(s) registered",
        addon_registry.count(),
        device_registry.count(),
    );

    // Launch the CanvasX UI
    ui::launch(device_registry, profile_store, addon_registry)?;

    Ok(())
}
