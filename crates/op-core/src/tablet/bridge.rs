use std::path::{Path, PathBuf};

use thiserror::Error;

use super::config::{OtdRawConfig, OtdTabletConfig};
use crate::device::{Capability, DeviceInfo, DeviceType};
use crate::profile::{DeviceProfile, HidInterfaceConfig};

#[derive(Debug, Error)]
pub enum OtdError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No configurations directory found at: {0}")]
    ConfigDirNotFound(String),
    #[error("Invalid OTD config: {0}")]
    InvalidConfig(String),
}

pub type OtdResult<T> = Result<T, OtdError>;

/// Bridge to OpenTabletDriver's tablet configuration database.
///
/// OpenTabletDriver stores tablet hardware definitions as JSON files in:
///   `OpenTabletDriver.Configurations/Configurations/<Brand>/<Model>.json`
///
/// This bridge imports those definitions so OpenPeripheral can recognize
/// and interact with drawing tablets without addon authors needing to
/// duplicate the identification work.
pub struct OtdBridge {
    /// Path to the OpenTabletDriver Configurations directory.
    configs_dir: PathBuf,
    /// Loaded tablet configs.
    tablets: Vec<OtdTabletConfig>,
}

impl OtdBridge {
    /// Create a new bridge pointing at an OTD configurations directory.
    ///
    /// `configs_dir` should point to the directory containing brand subdirectories
    /// (e.g., `Wacom/`, `XP-Pen/`, `Huion/`, etc.).
    pub fn new(configs_dir: impl Into<PathBuf>) -> Self {
        Self {
            configs_dir: configs_dir.into(),
            tablets: Vec::new(),
        }
    }

    /// Scan the OTD configurations directory and load all tablet definitions.
    pub fn load_all(&mut self) -> OtdResult<()> {
        self.tablets.clear();

        if !self.configs_dir.exists() {
            return Err(OtdError::ConfigDirNotFound(
                self.configs_dir.display().to_string(),
            ));
        }

        self.scan_dir(&self.configs_dir.clone())?;

        log::info!(
            "OTD bridge: loaded {} tablet definitions from {}",
            self.tablets.len(),
            self.configs_dir.display()
        );

        Ok(())
    }

    fn scan_dir(&mut self, dir: &Path) -> OtdResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Recurse into brand subdirectories
                self.scan_dir(&path)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match self.load_config(&path) {
                    Ok(Some(config)) => {
                        self.tablets.push(config);
                    }
                    Ok(None) => {
                        log::debug!("Skipped OTD config (no digitizer): {}", path.display());
                    }
                    Err(e) => {
                        log::warn!("Failed to parse OTD config {}: {e}", path.display());
                    }
                }
            }
        }
        Ok(())
    }

    fn load_config(&self, path: &Path) -> OtdResult<Option<OtdTabletConfig>> {
        let data = std::fs::read_to_string(path)?;
        let raw = OtdRawConfig::from_json(&data)?;
        Ok(raw.into_tablet_config())
    }

    /// Find a tablet config matching the given VID:PID.
    pub fn find_by_vid_pid(&self, vid: u16, pid: u16) -> Option<&OtdTabletConfig> {
        self.tablets
            .iter()
            .find(|t| t.vendor_id == vid && t.product_id == pid)
    }

    /// Convert an OTD tablet config into an OpenPeripheral DeviceInfo.
    pub fn to_device_info(config: &OtdTabletConfig) -> DeviceInfo {
        // Extract brand from the name (often "Brand Model")
        let brand = config
            .name
            .split_whitespace()
            .next()
            .unwrap_or("Unknown")
            .to_string();

        DeviceInfo {
            vendor_id: config.vendor_id,
            product_id: config.product_id,
            name: config.name.clone(),
            brand,
            device_type: DeviceType::Tablet,
            firmware_version: None,
            serial: None,
        }
    }

    /// Convert an OTD tablet config into an OpenPeripheral DeviceProfile.
    pub fn to_device_profile(config: &OtdTabletConfig) -> DeviceProfile {
        let brand = config
            .name
            .split_whitespace()
            .next()
            .unwrap_or("Unknown")
            .to_string();

        let mut capabilities = vec![
            Capability::PressureSensitivity {
                levels: config.max_pressure,
            },
            Capability::ActiveArea,
        ];

        if config.aux_buttons > 0 {
            capabilities.push(Capability::KeyRemap);
        }

        let mut hid_interfaces = Vec::new();
        if let Some(report_len) = config.input_report_length {
            hid_interfaces.push(HidInterfaceConfig {
                interface_number: 0,
                usage_page: 0x000D, // Digitizer usage page
                usage: 0x0002,     // Pen
                description: format!("Primary digitizer (report len: {report_len})"),
            });
        }

        DeviceProfile {
            version: 1,
            id: format!(
                "otd-{}-{:#06x}-{:#06x}",
                config.name.to_lowercase().replace(' ', "-"),
                config.vendor_id,
                config.product_id,
            ),
            device_name: config.name.clone(),
            brand,
            vendor_id: config.vendor_id,
            product_ids: vec![config.product_id],
            device_type: DeviceType::Tablet,
            capabilities,
            signals: std::collections::HashMap::new(),
            hid_interfaces,
            notes: Some("Imported from OpenTabletDriver configuration database".to_string()),
        }
    }

    /// List all loaded tablet configs.
    pub fn tablets(&self) -> &[OtdTabletConfig] {
        &self.tablets
    }

    /// Number of loaded tablets.
    pub fn count(&self) -> usize {
        self.tablets.len()
    }

    /// Get the configurations directory path.
    pub fn configs_dir(&self) -> &Path {
        &self.configs_dir
    }
}
