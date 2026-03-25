mod enumerate;
mod handle;

pub use enumerate::*;
pub use handle::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HidError {
    #[error("Failed to initialize HID API: {0}")]
    InitFailed(String),
    #[error("Device not found: VID={vid:#06x} PID={pid:#06x}")]
    DeviceNotFound { vid: u16, pid: u16 },
    #[error("Failed to open device: {0}")]
    OpenFailed(String),
    #[error("Read error: {0}")]
    ReadError(String),
    #[error("Write error: {0}")]
    WriteError(String),
    #[error("Device disconnected")]
    Disconnected,
}

pub type HidResult<T> = Result<T, HidError>;
