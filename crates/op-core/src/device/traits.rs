use super::{Capability, DeviceInfo, DeviceSetting};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("Device communication error: {0}")]
    Communication(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
    #[error("Device disconnected")]
    Disconnected,
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("HID error: {0}")]
    Hid(#[from] crate::hid::HidError),
}

pub type DeviceResult<T> = Result<T, DeviceError>;

/// Runtime state snapshot of a device.
#[derive(Debug, Clone)]
pub struct DeviceState {
    pub connected: bool,
    pub battery_percent: Option<u8>,
    pub current_dpi: Option<u32>,
    pub current_polling_rate: Option<u32>,
}

/// The core trait that every device driver must implement.
///
/// Addon developers implement this to provide support for specific hardware.
pub trait DeviceDriver: Send + Sync {
    /// Static information about this device.
    fn info(&self) -> &DeviceInfo;

    /// List of capabilities this device supports.
    fn capabilities(&self) -> Vec<Capability>;

    /// Query the current state of the device.
    fn get_state(&self) -> DeviceResult<DeviceState>;

    /// Apply a setting to the device.
    fn apply_setting(&self, setting: &DeviceSetting) -> DeviceResult<()>;

    /// Gracefully disconnect from the device.
    fn disconnect(&self) -> DeviceResult<()>;

    /// Read a raw HID report from the device (for signal capture / AI learning).
    fn read_raw(&self, buf: &mut [u8], timeout_ms: i32) -> DeviceResult<usize>;

    /// Write a raw HID report to the device.
    fn write_raw(&self, data: &[u8]) -> DeviceResult<()>;
}
