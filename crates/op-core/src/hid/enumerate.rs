use super::{HidError, HidResult};
use serde::{Deserialize, Serialize};

/// Summary of a detected HID device on the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HidDeviceEntry {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_name: String,
    pub manufacturer: String,
    pub serial_number: Option<String>,
    pub interface_number: i32,
    pub usage_page: u16,
    pub usage: u16,
}

/// Enumerate all HID devices currently connected to the system.
pub fn enumerate_hid_devices() -> HidResult<Vec<HidDeviceEntry>> {
    let api = hidapi::HidApi::new().map_err(|e| HidError::InitFailed(e.to_string()))?;

    let devices = api
        .device_list()
        .map(|dev| HidDeviceEntry {
            vendor_id: dev.vendor_id(),
            product_id: dev.product_id(),
            product_name: dev
                .product_string()
                .unwrap_or_default()
                .to_string(),
            manufacturer: dev
                .manufacturer_string()
                .unwrap_or_default()
                .to_string(),
            serial_number: dev.serial_number().map(|s| s.to_string()),
            interface_number: dev.interface_number(),
            usage_page: dev.usage_page(),
            usage: dev.usage(),
        })
        .collect();

    Ok(devices)
}

/// Find all HID interfaces for a specific VID:PID.
pub fn find_device_interfaces(vid: u16, pid: u16) -> HidResult<Vec<HidDeviceEntry>> {
    let all = enumerate_hid_devices()?;
    Ok(all
        .into_iter()
        .filter(|d| d.vendor_id == vid && d.product_id == pid)
        .collect())
}
