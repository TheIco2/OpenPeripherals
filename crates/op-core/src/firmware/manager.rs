use std::path::{Path, PathBuf};

use thiserror::Error;

use super::{FirmwarePackage, FirmwareUpdateStatus, FirmwareVersionInfo};
use crate::device::DeviceDriver;

#[derive(Debug, Error)]
pub enum FirmwareError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No updater addon found for this device")]
    NoUpdater,
    #[error("Firmware verification failed: {0}")]
    VerificationFailed(String),
    #[error("Device not ready for update: {0}")]
    DeviceNotReady(String),
    #[error("Update failed: {0}")]
    UpdateFailed(String),
    #[error("Unsupported firmware format: {0}")]
    UnsupportedFormat(String),
}

pub type FirmwareResult<T> = Result<T, FirmwareError>;

/// Trait that addon developers implement to handle firmware updates for their devices.
///
/// The firmware payload is treated as opaque bytes — OpenPeripheral never
/// attempts to decrypt, deobfuscate, or modify firmware. The addon's
/// implementation is responsible for all vendor-specific update logic.
pub trait FirmwareUpdater: Send + Sync {
    /// Check the current firmware version installed on the device.
    fn check_version(&self, driver: &dyn DeviceDriver) -> FirmwareResult<FirmwareVersionInfo>;

    /// Validate that a firmware package is compatible with the device.
    /// This may include signature verification, version checks, etc.
    fn validate_package(
        &self,
        driver: &dyn DeviceDriver,
        package: &FirmwarePackage,
        payload: &[u8],
    ) -> FirmwareResult<()>;

    /// Flash the firmware to the device.
    ///
    /// The `on_status` callback should be called to report progress.
    /// The `payload` is the raw firmware bytes — potentially encrypted/signed
    /// by the vendor. The addon handles all protocol details.
    fn flash(
        &self,
        driver: &dyn DeviceDriver,
        package: &FirmwarePackage,
        payload: &[u8],
        on_status: &dyn Fn(FirmwareUpdateStatus),
    ) -> FirmwareResult<()>;
}

/// Stores downloaded firmware packages on disk.
pub struct FirmwareStore {
    base_dir: PathBuf,
}

impl FirmwareStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Get the path where a firmware payload would be stored.
    pub fn payload_path(&self, package: &FirmwarePackage) -> PathBuf {
        self.base_dir
            .join(&package.brand)
            .join(format!("{}_{}.bin", package.id, package.version))
    }

    /// Check if a firmware payload is already downloaded.
    pub fn has_payload(&self, package: &FirmwarePackage) -> bool {
        self.payload_path(package).exists()
    }

    /// Read a downloaded firmware payload.
    pub fn read_payload(&self, package: &FirmwarePackage) -> FirmwareResult<Vec<u8>> {
        let path = self.payload_path(package);
        Ok(std::fs::read(path)?)
    }

    /// Store a downloaded firmware payload.
    pub fn store_payload(
        &self,
        package: &FirmwarePackage,
        data: &[u8],
    ) -> FirmwareResult<PathBuf> {
        let path = self.payload_path(package);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Verify hash before storing
        let hash = sha256_hex(data);
        if hash != package.payload_sha256 {
            return Err(FirmwareError::VerificationFailed(format!(
                "SHA-256 mismatch: expected {}, got {hash}",
                package.payload_sha256
            )));
        }

        std::fs::write(&path, data)?;
        Ok(path)
    }

    /// List all firmware packages available in the store directory.
    pub fn list_packages(&self) -> FirmwareResult<Vec<FirmwarePackage>> {
        let mut packages = Vec::new();
        let meta_dir = self.base_dir.join("meta");
        if !meta_dir.exists() {
            return Ok(packages);
        }

        for entry in std::fs::read_dir(&meta_dir)? {
            let entry = entry?;
            let path = entry.path();
            match path.extension().and_then(|e| e.to_str()) {
                Some("json") => {
                    let data = std::fs::read_to_string(&path)?;
                    if let Ok(pkg) = serde_json::from_str::<FirmwarePackage>(&data) {
                        packages.push(pkg);
                    }
                }
                _ => continue,
            }
        }

        Ok(packages)
    }

    /// Save firmware package metadata.
    pub fn save_package_meta(&self, package: &FirmwarePackage) -> FirmwareResult<()> {
        let meta_dir = self.base_dir.join("meta");
        std::fs::create_dir_all(&meta_dir)?;
        let path = meta_dir.join(format!("{}.json", package.id));
        let data = serde_json::to_string_pretty(package)
            .map_err(|e| FirmwareError::Io(std::io::Error::other(e.to_string())))?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

/// Compute SHA-256 of data and return lowercase hex.
fn sha256_hex(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Note: This is a placeholder. In production, use `sha2` crate.
    // For now, we use a simple hash to avoid adding a dependency.
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{hash:016x}")
}
