use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::device::{Capability, DeviceType};
use crate::signal::SignalPattern;

/// A complete device profile describing how to communicate with a peripheral.
///
/// This is the primary output of the AI signal learning system, and also what
/// addon developers provide for known devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    /// Profile format version.
    pub version: u32,
    /// Unique profile identifier.
    pub id: String,
    /// Human-readable device name.
    pub device_name: String,
    /// Brand / manufacturer.
    pub brand: String,
    /// USB Vendor ID.
    pub vendor_id: u16,
    /// USB Product IDs this profile applies to (some devices have multiple).
    pub product_ids: Vec<u16>,
    /// What kind of device this is.
    pub device_type: DeviceType,
    /// Capabilities this device supports.
    pub capabilities: Vec<Capability>,
    /// Named signal patterns for controlling the device.
    pub signals: HashMap<String, SignalPattern>,
    /// Which HID interface(s) to use.
    pub hid_interfaces: Vec<HidInterfaceConfig>,
    /// Free-form notes (e.g., from AI learning session).
    pub notes: Option<String>,
}

/// Configuration for a specific HID interface on the device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HidInterfaceConfig {
    pub interface_number: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub description: String,
}

impl DeviceProfile {
    /// Check if this profile matches a given VID:PID.
    pub fn matches(&self, vid: u16, pid: u16) -> bool {
        self.vendor_id == vid && self.product_ids.contains(&pid)
    }
}
