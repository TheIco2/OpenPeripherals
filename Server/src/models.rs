use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ──── Addon models ────

/// Addon listing in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonEntry {
    /// Unique addon ID (e.g., "corsair-devices").
    pub id: String,
    /// Display name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Author or organization.
    pub author: String,
    /// Description.
    pub description: String,
    /// Device brands supported.
    pub brands: Vec<String>,
    /// Device types supported.
    pub device_types: Vec<String>,
    /// Supported VID:PID pairs (as "VID:PID" hex strings).
    pub supported_devices: Vec<String>,
    /// Download count.
    pub downloads: u64,
    /// SHA-256 of the addon package.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// When this version was published.
    pub published_at: DateTime<Utc>,
    /// Minimum OpenPeripheral version required.
    pub min_app_version: Option<String>,
}

/// Request to publish or update an addon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishAddonRequest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub brands: Vec<String>,
    pub device_types: Vec<String>,
    pub supported_devices: Vec<String>,
    pub min_app_version: Option<String>,
}

// ──── Firmware models ────

/// Firmware entry in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareEntry {
    /// Unique firmware ID.
    pub id: String,
    /// Brand / manufacturer.
    pub brand: String,
    /// Device name.
    pub device_name: String,
    /// Firmware version string.
    pub version: String,
    /// Vendor ID.
    pub vendor_id: u16,
    /// Product IDs.
    pub product_ids: Vec<u16>,
    /// SHA-256 of the firmware payload.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// Whether the firmware is encrypted/signed/obfuscated.
    pub protection: String,
    /// Release notes.
    pub release_notes: Option<String>,
    /// The addon ID whose updater handles this firmware.
    pub updater_addon_id: String,
    /// Downloads.
    pub downloads: u64,
    /// Publication date.
    pub published_at: DateTime<Utc>,
}

/// Request to publish firmware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishFirmwareRequest {
    pub id: String,
    pub brand: String,
    pub device_name: String,
    pub version: String,
    pub vendor_id: u16,
    pub product_ids: Vec<u16>,
    pub protection: String,
    pub release_notes: Option<String>,
    pub updater_addon_id: String,
}

// ──── App update models ────

/// Application version entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppVersion {
    pub version: String,
    pub release_notes: Option<String>,
    pub sha256: String,
    pub size: u64,
    pub published_at: DateTime<Utc>,
}

/// Request to publish an app update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishUpdateRequest {
    pub version: String,
    pub release_notes: Option<String>,
}

// ──── Query params ────

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub brand: Option<String>,
    pub device_type: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct FirmwareCheckQuery {
    pub vendor_id: u16,
    pub product_id: u16,
    pub _current_version: Option<String>,
}

// ──── Generic responses ────

#[derive(Debug, Serialize)]
pub struct ListResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}
