use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use op_core::device::{DeviceDriver, DeviceRegistry};

use super::loader::{AddonLoadError, LoadedAddon};
use super::manifest::AddonManifest;

/// Manages all installed addons: discovery, loading, and device-to-addon matching.
pub struct AddonRegistry {
    /// Base directory where addons are installed (each addon in its own subdirectory).
    addons_dir: PathBuf,
    /// Loaded addons keyed by addon ID.
    loaded: HashMap<String, LoadedAddon>,
}

impl AddonRegistry {
    pub fn new(addons_dir: impl Into<PathBuf>) -> Self {
        Self {
            addons_dir: addons_dir.into(),
            loaded: HashMap::new(),
        }
    }

    /// Scan the addons directory and load all valid addon manifests.
    ///
    /// # Safety
    /// Loads native libraries. Only call with trusted addon directories.
    pub unsafe fn discover_and_load(&mut self) -> Vec<AddonLoadError> {
        let mut errors = Vec::new();

        if !self.addons_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&self.addons_dir) {
                errors.push(AddonLoadError::Io(e));
                return errors;
            }
        }

        let entries = match std::fs::read_dir(&self.addons_dir) {
            Ok(e) => e,
            Err(e) => {
                errors.push(AddonLoadError::Io(e));
                return errors;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Check if it has a manifest
            let has_manifest =
                path.join("addon.yaml").exists() || path.join("addon.json").exists();
            if !has_manifest {
                continue;
            }

            match unsafe { LoadedAddon::load(&path) } {
                Ok(addon) => {
                    log::info!("Loaded addon: {} v{}", addon.manifest.name, addon.manifest.version);
                    self.loaded.insert(addon.manifest.id.clone(), addon);
                }
                Err(e) => {
                    log::error!("Failed to load addon from {}: {e}", path.display());
                    errors.push(e);
                }
            }
        }

        errors
    }

    /// Try to create a driver for a detected device by checking all loaded addons.
    ///
    /// # Safety
    /// Calls into native addon code.
    pub unsafe fn create_driver_for(
        &self,
        vid: u16,
        pid: u16,
    ) -> Option<Arc<dyn DeviceDriver>> {
        for addon in self.loaded.values() {
            if let Some(driver) = unsafe { addon.create_driver(vid, pid) } {
                return Some(driver);
            }
        }
        None
    }

    /// Auto-detect connected devices, find matching addons, and register drivers.
    ///
    /// # Safety
    /// Calls into native addon code.
    pub unsafe fn auto_register(
        &self,
        device_registry: &DeviceRegistry,
    ) -> Result<usize, op_core::hid::HidError> {
        let devices = op_core::hid::enumerate_hid_devices()?;
        let mut count = 0;

        // De-duplicate by VID:PID
        let mut seen = std::collections::HashSet::new();
        for dev in &devices {
            let key = ((dev.vendor_id as u32) << 16) | (dev.product_id as u32);
            if !seen.insert(key) {
                continue;
            }

            if let Some(driver) = unsafe { self.create_driver_for(dev.vendor_id, dev.product_id) } {
                log::info!(
                    "Auto-registered driver for {} (VID:{:#06x} PID:{:#06x})",
                    driver.info().name,
                    dev.vendor_id,
                    dev.product_id,
                );
                device_registry.register(driver);
                count += 1;
            }
        }

        Ok(count)
    }

    /// Get the manifests of all loaded addons.
    pub fn list_addons(&self) -> Vec<&AddonManifest> {
        self.loaded.values().map(|a| &a.manifest).collect()
    }

    /// Number of loaded addons.
    pub fn count(&self) -> usize {
        self.loaded.len()
    }

    /// Path to the addons directory.
    pub fn addons_dir(&self) -> &Path {
        &self.addons_dir
    }
}
