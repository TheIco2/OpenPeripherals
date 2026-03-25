use std::path::{Path, PathBuf};
use std::sync::Arc;

use op_core::device::DeviceDriver;

use super::manifest::{AddonManifest, AddonManifestError};

/// A loaded addon with its manifest and dynamic library.
pub struct LoadedAddon {
    pub manifest: AddonManifest,
    pub addon_dir: PathBuf,
    _lib: libloading::Library,
    create_fn: CreateDriverFn,
}

/// Signature of the entry point function that addons must export.
///
/// ```c
/// // C ABI function that the addon .dll/.so exports:
/// OpenPeripheralDriver* op_create_driver(uint16_t vid, uint16_t pid);
/// ```
///
/// In Rust, addons implement this as:
/// ```rust,ignore
/// #[no_mangle]
/// pub extern "C" fn op_create_driver(vid: u16, pid: u16) -> *mut dyn DeviceDriver {
///     // ...
/// }
/// ```
type CreateDriverFn = unsafe fn(u16, u16) -> *mut dyn DeviceDriver;

#[derive(Debug, thiserror::Error)]
pub enum AddonLoadError {
    #[error("Manifest error: {0}")]
    Manifest(#[from] AddonManifestError),
    #[error("Library load error: {0}")]
    Library(String),
    #[error("Missing entry point: op_create_driver")]
    MissingEntryPoint,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl LoadedAddon {
    /// Load an addon from a directory containing `addon.yaml` and a shared library.
    ///
    /// # Safety
    /// This loads and executes native code from the addon library. Only load trusted addons.
    pub unsafe fn load(addon_dir: &Path) -> Result<Self, AddonLoadError> {
        // Try YAML first, then JSON
        let manifest_path = addon_dir.join("addon.yaml");
        let manifest = if manifest_path.exists() {
            AddonManifest::from_yaml(&manifest_path)?
        } else {
            let json_path = addon_dir.join("addon.json");
            AddonManifest::from_json(&json_path)?
        };

        let lib_path = addon_dir.join(&manifest.library);

        let lib = unsafe {
            libloading::Library::new(&lib_path)
                .map_err(|e| AddonLoadError::Library(e.to_string()))?
        };

        let create_fn: CreateDriverFn = unsafe {
            let sym = lib
                .get::<CreateDriverFn>(b"op_create_driver")
                .map_err(|_| AddonLoadError::MissingEntryPoint)?;
            *sym
        };

        Ok(Self {
            manifest,
            addon_dir: addon_dir.to_path_buf(),
            _lib: lib,
            create_fn,
        })
    }

    /// Create a device driver for the given VID:PID.
    ///
    /// # Safety
    /// Calls into the addon's native code.
    pub unsafe fn create_driver(&self, vid: u16, pid: u16) -> Option<Arc<dyn DeviceDriver>> {
        if !self.manifest.supports_device(vid, pid) {
            return None;
        }

        let ptr = unsafe { (self.create_fn)(vid, pid) };
        if ptr.is_null() {
            return None;
        }

        // SAFETY: The addon allocated this via Box::into_raw, we take ownership
        let driver = unsafe { Box::from_raw(ptr) };
        Some(Arc::from(driver))
    }
}
