pub use op_core::device::{
    Capability, DeviceDriver, DeviceError, DeviceInfo, DeviceResult, DeviceSetting, DeviceState,
    DeviceType, LightingEffect, RgbColor,
};
pub use op_core::firmware::{
    FirmwareError, FirmwarePackage, FirmwareProtection, FirmwareResult, FirmwareUpdateStatus,
    FirmwareUpdater, FirmwareVersionInfo,
};
pub use op_core::hid::{HidHandle, HidResult};
pub use op_core::profile::DeviceProfile;
pub use op_core::signal::SignalPattern;

/// Macro to export the required addon entry point.
///
/// Usage in your addon crate:
/// ```rust,ignore
/// use op_sdk::*;
///
/// struct MyDriver { /* ... */ }
/// impl DeviceDriver for MyDriver { /* ... */ }
///
/// op_sdk::export_driver!(|vid, pid| {
///     if vid == 0x1b1c && pid == 0x1b4f {
///         Some(Box::new(MyDriver::new(vid, pid)))
///     } else {
///         None
///     }
/// });
/// ```
#[macro_export]
macro_rules! export_driver {
    ($factory:expr) => {
        #[no_mangle]
        pub extern "C" fn op_create_driver(
            vid: u16,
            pid: u16,
        ) -> *mut dyn $crate::DeviceDriver {
            let factory: fn(u16, u16) -> Option<Box<dyn $crate::DeviceDriver>> = $factory;
            match factory(vid, pid) {
                Some(driver) => Box::into_raw(driver),
                None => std::ptr::null_mut(),
            }
        }
    };
}
