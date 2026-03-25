use super::{HidError, HidResult};

/// A handle to an opened HID device for raw read/write.
pub struct HidHandle {
    device: hidapi::HidDevice,
    vid: u16,
    pid: u16,
}

impl HidHandle {
    /// Open a HID device by VID, PID, and optional interface number.
    pub fn open(vid: u16, pid: u16, interface: Option<i32>) -> HidResult<Self> {
        let api = hidapi::HidApi::new().map_err(|e| HidError::InitFailed(e.to_string()))?;

        let device = if let Some(iface) = interface {
            // Find the specific interface
            let dev_info = api
                .device_list()
                .find(|d| {
                    d.vendor_id() == vid
                        && d.product_id() == pid
                        && d.interface_number() == iface
                })
                .ok_or(HidError::DeviceNotFound { vid, pid })?;

            dev_info
                .open_device(&api)
                .map_err(|e| HidError::OpenFailed(e.to_string()))?
        } else {
            api.open(vid, pid)
                .map_err(|e| HidError::OpenFailed(e.to_string()))?
        };

        // Non-blocking by default for signal capture
        device
            .set_blocking_mode(false)
            .map_err(|e| HidError::OpenFailed(e.to_string()))?;

        Ok(Self { device, vid, pid })
    }

    /// Read a HID report. Returns number of bytes read, or 0 if no data available.
    pub fn read(&self, buf: &mut [u8], timeout_ms: i32) -> HidResult<usize> {
        let n = self
            .device
            .read_timeout(buf, timeout_ms)
            .map_err(|e| HidError::ReadError(e.to_string()))?;
        Ok(n)
    }

    /// Write a HID report.
    pub fn write(&self, data: &[u8]) -> HidResult<usize> {
        let n = self
            .device
            .write(data)
            .map_err(|e| HidError::WriteError(e.to_string()))?;
        Ok(n)
    }

    /// Send a feature report.
    pub fn send_feature_report(&self, data: &[u8]) -> HidResult<()> {
        self.device
            .send_feature_report(data)
            .map_err(|e| HidError::WriteError(e.to_string()))?;
        Ok(())
    }

    /// Get a feature report.
    pub fn get_feature_report(&self, buf: &mut [u8]) -> HidResult<usize> {
        let n = self
            .device
            .get_feature_report(buf)
            .map_err(|e| HidError::ReadError(e.to_string()))?;
        Ok(n)
    }

    pub fn vid(&self) -> u16 {
        self.vid
    }

    pub fn pid(&self) -> u16 {
        self.pid
    }
}
