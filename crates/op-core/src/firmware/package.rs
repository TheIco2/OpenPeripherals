use serde::{Deserialize, Serialize};

use super::FirmwareProtection;

/// A firmware package ready for flashing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwarePackage {
    /// Unique package identifier.
    pub id: String,
    /// Device brand / manufacturer.
    pub brand: String,
    /// Human-readable device name(s) this firmware applies to.
    pub device_name: String,
    /// USB vendor ID.
    pub vendor_id: u16,
    /// USB product IDs this firmware applies to.
    pub product_ids: Vec<u16>,
    /// Firmware version string (e.g., "5.12.140").
    pub version: String,
    /// Minimum firmware version required to apply this update, if any.
    pub min_current_version: Option<String>,
    /// Release notes / changelog.
    pub release_notes: Option<String>,
    /// Size of the payload in bytes.
    pub payload_size: u64,
    /// SHA-256 hash of the raw payload bytes (hex).
    pub payload_sha256: String,
    /// Protection applied to this firmware by the vendor.
    pub protection: FirmwareProtection,
    /// The addon ID whose `FirmwareUpdater` handles flashing this firmware.
    pub updater_addon_id: String,
}

/// Status of a firmware update in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FirmwareUpdateStatus {
    /// Checking device state before update.
    Preparing,
    /// Transferring firmware to device.
    Transferring { progress_percent: f32 },
    /// Waiting for device to apply the update.
    Applying,
    /// Device is rebooting after update.
    Rebooting,
    /// Update completed successfully.
    Complete { new_version: String },
    /// Update failed.
    Failed { reason: String },
}

/// Result of a firmware version check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareVersionInfo {
    /// Currently installed version on the device.
    pub current_version: String,
    /// Latest available version (if known).
    pub latest_version: Option<String>,
    /// Whether an update is available.
    pub update_available: bool,
}
