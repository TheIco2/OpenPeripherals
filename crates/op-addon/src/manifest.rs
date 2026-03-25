use serde::{Deserialize, Serialize};

use op_core::device::DeviceType;

/// Manifest for an OpenPeripheral addon package (`addon.yaml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonManifest {
    /// Unique addon identifier (e.g., "corsair-devices").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Author or organization.
    pub author: String,
    /// Short description.
    pub description: String,
    /// Minimum OpenPeripheral version required.
    pub min_app_version: Option<String>,
    /// Devices this addon supports.
    pub supported_devices: Vec<SupportedDevice>,
    /// Filename of the shared library (e.g., "corsair_devices.dll").
    pub library: String,
    /// Optional path to UI assets directory within the addon package.
    pub ui_assets: Option<String>,
}

/// A device that an addon claims to support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedDevice {
    /// USB Vendor ID.
    pub vendor_id: u16,
    /// USB Product IDs.
    pub product_ids: Vec<u16>,
    /// Device name.
    pub name: String,
    /// Device type.
    pub device_type: DeviceType,
}

impl AddonManifest {
    /// Load a manifest from a YAML file.
    pub fn from_yaml(path: &std::path::Path) -> Result<Self, AddonManifestError> {
        let data = std::fs::read_to_string(path)?;
        let manifest: Self = serde_yaml::from_str(&data)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Load a manifest from a JSON file.
    pub fn from_json(path: &std::path::Path) -> Result<Self, AddonManifestError> {
        let data = std::fs::read_to_string(path)?;
        let manifest: Self = serde_json::from_str(&data)?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), AddonManifestError> {
        if self.id.is_empty() {
            return Err(AddonManifestError::Invalid("addon id is empty".into()));
        }
        if self.library.is_empty() {
            return Err(AddonManifestError::Invalid("library path is empty".into()));
        }
        if self.supported_devices.is_empty() {
            return Err(AddonManifestError::Invalid(
                "no supported devices declared".into(),
            ));
        }
        Ok(())
    }

    /// Check if this addon supports a given VID:PID pair.
    pub fn supports_device(&self, vid: u16, pid: u16) -> bool {
        self.supported_devices
            .iter()
            .any(|d| d.vendor_id == vid && d.product_ids.contains(&pid))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddonManifestError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid manifest: {0}")]
    Invalid(String),
}
