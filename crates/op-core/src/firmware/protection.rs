use serde::{Deserialize, Serialize};

/// Describes how a firmware payload is protected by the vendor.
///
/// OpenPeripheral respects that brands may obfuscate, encrypt, or sign their
/// firmware. We never attempt to decrypt or bypass vendor protection — we simply
/// pass the opaque payload through to the device's vendor-provided update
/// routine (inside the addon's `FirmwareUpdater` implementation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FirmwareProtection {
    /// Firmware is plain / unprotected — bytes can be inspected.
    None,

    /// Firmware is signed (the device or updater verifies a signature).
    Signed {
        /// Algorithm identifier (e.g., "ed25519", "rsa-2048", "ecdsa-p256").
        algorithm: String,
        /// Optional public key or fingerprint the vendor publishes (hex).
        public_key_hint: Option<String>,
    },

    /// Firmware is encrypted — contents are opaque to OpenPeripheral.
    Encrypted {
        /// Algorithm identifier (e.g., "aes-256-gcm", "chacha20poly1305").
        algorithm: String,
    },

    /// Firmware uses vendor-proprietary obfuscation.
    Obfuscated {
        /// Free-form description (e.g., "XOR scramble", "custom codec").
        description: String,
    },

    /// Combination of multiple protections.
    Multi(Vec<FirmwareProtection>),
}

impl FirmwareProtection {
    pub fn is_protected(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Human-readable summary for UI display.
    pub fn summary(&self) -> String {
        match self {
            Self::None => "Unprotected".to_string(),
            Self::Signed { algorithm, .. } => format!("Signed ({algorithm})"),
            Self::Encrypted { algorithm } => format!("Encrypted ({algorithm})"),
            Self::Obfuscated { description } => format!("Obfuscated ({description})"),
            Self::Multi(protections) => {
                let parts: Vec<String> = protections.iter().map(|p| p.summary()).collect();
                parts.join(" + ")
            }
        }
    }
}
