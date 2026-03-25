use serde::{Deserialize, Serialize};

/// The type of peripheral device.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceType {
    Keyboard,
    Mouse,
    Headset,
    MousePad,
    Tablet,
    SmartLight,
    Other(String),
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keyboard => write!(f, "Keyboard"),
            Self::Mouse => write!(f, "Mouse"),
            Self::Headset => write!(f, "Headset"),
            Self::MousePad => write!(f, "Mouse Pad"),
            Self::Tablet => write!(f, "Tablet"),
            Self::SmartLight => write!(f, "Smart Light"),
            Self::Other(name) => write!(f, "{name}"),
        }
    }
}

/// Identifying information for a connected device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    pub brand: String,
    pub device_type: DeviceType,
    pub firmware_version: Option<String>,
    pub serial: Option<String>,
}

impl DeviceInfo {
    /// USB VID:PID as a combined u32 key.
    pub fn vid_pid_key(&self) -> u32 {
        ((self.vendor_id as u32) << 16) | (self.product_id as u32)
    }
}
