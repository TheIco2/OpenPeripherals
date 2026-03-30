use super::{HidError, HidResult};

/// A handle to an opened HID device for raw read/write.
pub struct HidHandle {
    device: hidapi::HidDevice,
    vid: u16,
    pid: u16,
    interface: i32,
    usage_page: u16,
    usage: u16,
}

impl HidHandle {
    /// Open a HID device by VID, PID, and optional interface number.
    pub fn open(vid: u16, pid: u16, interface: Option<i32>) -> HidResult<Self> {
        let api = hidapi::HidApi::new().map_err(|e| HidError::InitFailed(e.to_string()))?;

        let (device, iface_num) = if let Some(iface) = interface {
            // Find the specific interface
            let dev_info = api
                .device_list()
                .find(|d| {
                    d.vendor_id() == vid
                        && d.product_id() == pid
                        && d.interface_number() == iface
                })
                .ok_or(HidError::DeviceNotFound { vid, pid })?;

            let dev = dev_info
                .open_device(&api)
                .map_err(|e| HidError::OpenFailed(e.to_string()))?;
            (dev, iface)
        } else {
            let dev = api
                .open(vid, pid)
                .map_err(|e| HidError::OpenFailed(e.to_string()))?;
            (dev, -1)
        };

        // Non-blocking by default for signal capture
        device
            .set_blocking_mode(false)
            .map_err(|e| HidError::OpenFailed(e.to_string()))?;

        Ok(Self {
            device,
            vid,
            pid,
            interface: iface_num,
            usage_page: 0,
            usage: 0,
        })
    }

    /// Open ALL HID top-level collections for a given VID:PID.
    ///
    /// On Windows a single USB interface can expose multiple TLCs (each with a
    /// unique device path and usage page). We deduplicate by **path**, not by
    /// interface number, so every openable TLC gets its own handle.
    pub fn open_all_interfaces(vid: u16, pid: u16) -> Vec<Self> {
        let api = match hidapi::HidApi::new() {
            Ok(a) => a,
            Err(e) => {
                log::warn!("Failed to init HID API: {e}");
                return Vec::new();
            }
        };

        let mut seen_paths = std::collections::HashSet::new();
        let mut handles = Vec::new();

        for dev_info in api.device_list() {
            if dev_info.vendor_id() != vid || dev_info.product_id() != pid {
                continue;
            }
            let path_str = String::from_utf8_lossy(dev_info.path().to_bytes()).to_string();
            if !seen_paths.insert(path_str.clone()) {
                continue; // same device path already processed
            }
            let iface = dev_info.interface_number();
            let up = dev_info.usage_page();
            let u = dev_info.usage();
            match dev_info.open_device(&api) {
                Ok(device) => {
                    if let Err(e) = device.set_blocking_mode(false) {
                        log::warn!("Failed to set non-blocking for iface {iface} ({up:#06x}/{u:#06x}): {e}");
                        continue;
                    }
                    log::info!(
                        "Opened TLC iface={iface} usage_page={up:#06x} usage={u:#06x}",
                    );
                    handles.push(Self {
                        device,
                        vid,
                        pid,
                        interface: iface,
                        usage_page: up,
                        usage: u,
                    });
                }
                Err(e) => {
                    log::warn!(
                        "Could not open iface {iface} ({up:#06x}/{u:#06x}): {e}",
                    );
                }
            }
        }

        handles
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

    pub fn interface(&self) -> i32 {
        self.interface
    }

    pub fn usage_page(&self) -> u16 {
        self.usage_page
    }

    pub fn usage(&self) -> u16 {
        self.usage
    }

    /// Returns true if this handle is on a vendor-specific HID interface
    /// (usage page >= 0xFF00).
    pub fn is_vendor_interface(&self) -> bool {
        self.usage_page >= 0xFF00
    }
}
