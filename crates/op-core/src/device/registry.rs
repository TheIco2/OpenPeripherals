use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::{DeviceDriver, DeviceInfo};

/// Central registry of all active device drivers.
///
/// The addon system registers drivers here when devices are detected.
pub struct DeviceRegistry {
    devices: RwLock<HashMap<u32, Arc<dyn DeviceDriver>>>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            devices: RwLock::new(HashMap::new()),
        }
    }

    /// Register a device driver, keyed by its VID:PID.
    pub fn register(&self, driver: Arc<dyn DeviceDriver>) {
        let key = driver.info().vid_pid_key();
        let mut devices = self.devices.write().expect("registry lock poisoned");
        devices.insert(key, driver);
    }

    /// Remove a device driver by VID:PID key.
    pub fn unregister(&self, vid_pid_key: u32) {
        let mut devices = self.devices.write().expect("registry lock poisoned");
        devices.remove(&vid_pid_key);
    }

    /// Get a device driver by VID:PID key.
    pub fn get(&self, vid_pid_key: u32) -> Option<Arc<dyn DeviceDriver>> {
        let devices = self.devices.read().expect("registry lock poisoned");
        devices.get(&vid_pid_key).cloned()
    }

    /// List all registered devices and their info.
    pub fn list(&self) -> Vec<DeviceInfo> {
        let devices = self.devices.read().expect("registry lock poisoned");
        devices.values().map(|d| d.info().clone()).collect()
    }

    /// Number of registered devices.
    pub fn count(&self) -> usize {
        let devices = self.devices.read().expect("registry lock poisoned");
        devices.len()
    }
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
