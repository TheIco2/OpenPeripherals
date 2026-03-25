use serde::{Deserialize, Serialize};

/// A capability that a device may support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Capability {
    /// RGB lighting control.
    Rgb {
        /// Number of independently controllable zones.
        zone_count: u32,
    },

    /// DPI (sensitivity) adjustment for mice.
    Dpi {
        min: u32,
        max: u32,
        step: u32,
    },

    /// Polling rate selection.
    PollingRate {
        /// Available rates in Hz (e.g., [125, 500, 1000, 4000]).
        rates: Vec<u32>,
    },

    /// Battery level reporting.
    Battery,

    /// Audio equalizer (headsets).
    Equalizer {
        bands: u32,
    },

    /// Sidetone / mic monitoring level (headsets).
    Sidetone {
        min: u32,
        max: u32,
    },

    /// Programmable macro support.
    Macro,

    /// Key/button remapping.
    KeyRemap,

    /// Media playback controls.
    MediaControl,

    /// Brightness control (mouse pads, smart lights).
    Brightness {
        min: u32,
        max: u32,
    },

    /// Firmware update support.
    FirmwareUpdate,

    /// Tablet pressure sensitivity levels.
    PressureSensitivity {
        levels: u32,
    },

    /// Tablet active area configuration.
    ActiveArea,

    /// Custom capability defined by an addon.
    Custom {
        name: String,
        description: String,
    },
}

/// An RGB color value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const WHITE: Self = Self::new(255, 255, 255);
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const RED: Self = Self::new(255, 0, 0);
    pub const GREEN: Self = Self::new(0, 255, 0);
    pub const BLUE: Self = Self::new(0, 0, 255);
}

/// A lighting effect that can be applied to RGB zones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LightingEffect {
    Static(RgbColor),
    Breathing {
        color: RgbColor,
        speed: f32,
    },
    Rainbow {
        speed: f32,
    },
    Wave {
        colors: Vec<RgbColor>,
        speed: f32,
    },
    Custom {
        name: String,
        params: serde_json::Value,
    },
}

/// Settings that can be applied to a device. Each variant maps to a capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceSetting {
    SetRgb {
        zone: u32,
        effect: LightingEffect,
    },
    SetDpi(u32),
    SetPollingRate(u32),
    SetEqualizer {
        bands: Vec<f32>,
    },
    SetSidetone(u32),
    SetBrightness(u32),
    Custom {
        name: String,
        value: serde_json::Value,
    },
}

/// Infer capabilities from detected signal patterns.
pub fn capabilities_from_patterns(patterns: &[crate::signal::SignalPattern]) -> Vec<Capability> {
    let mut caps = Vec::new();
    for p in patterns {
        match p.name.as_str() {
            "set_rgb" => caps.push(Capability::Rgb { zone_count: 1 }),
            "set_dpi" => caps.push(Capability::Dpi {
                min: 100,
                max: 30000,
                step: 50,
            }),
            "set_polling_rate" => caps.push(Capability::PollingRate {
                rates: vec![125, 500, 1000],
            }),
            _ => {}
        }
    }
    caps
}
