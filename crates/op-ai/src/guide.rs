use serde::{Deserialize, Serialize};

use op_core::device::DeviceType;

/// A step in the AI-guided signal learning process.
///
/// The AI presents these instructions to the user one at a time, captures
/// HID traffic while the user performs the action, and then analyzes the results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidedStep {
    /// Unique step ID.
    pub id: String,
    /// Instruction displayed to the user (e.g., "Set your DPI to the lowest setting").
    pub instruction: String,
    /// Category of action being tested.
    pub category: StepCategory,
    /// How long to capture signals after the user confirms (milliseconds).
    pub capture_duration_ms: u64,
    /// Whether the user should perform the action DURING capture (true)
    /// or BEFORE capture starts (false, for state comparison).
    pub capture_during_action: bool,
}

/// Category of an AI-guided learning step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepCategory {
    Baseline,
    Rgb,
    Dpi,
    PollingRate,
    Macro,
    Battery,
    Equalizer,
    Sidetone,
    Brightness,
    Custom(String),
}

/// Generate a list of guided steps for a given device type.
pub fn generate_guide(device_type: &DeviceType) -> Vec<GuidedStep> {
    let mut steps = vec![
        // Always start with a baseline capture
        GuidedStep {
            id: "baseline_idle".into(),
            instruction: "Don't touch the device. We'll capture its idle state.".into(),
            category: StepCategory::Baseline,
            capture_duration_ms: 3000,
            capture_during_action: false,
        },
    ];

    match device_type {
        DeviceType::Mouse => {
            steps.extend(mouse_guide());
        }
        DeviceType::Keyboard => {
            steps.extend(keyboard_guide());
        }
        DeviceType::Headset => {
            steps.extend(headset_guide());
        }
        DeviceType::MousePad => {
            steps.extend(mousepad_guide());
        }
        DeviceType::SmartLight => {
            steps.extend(smart_light_guide());
        }
        DeviceType::Other(_) => {
            steps.extend(generic_guide());
        }
    }

    steps
}

fn mouse_guide() -> Vec<GuidedStep> {
    vec![
        // --- DPI ---
        GuidedStep {
            id: "dpi_lowest".into(),
            instruction: "Using your mouse's software or DPI button, set the DPI to the LOWEST setting. Press Continue when done.".into(),
            category: StepCategory::Dpi,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "dpi_highest".into(),
            instruction: "Now set the DPI to the HIGHEST setting. Press Continue when done.".into(),
            category: StepCategory::Dpi,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "dpi_cycle".into(),
            instruction: "Press the DPI button on your mouse to cycle through DPI stages. Do it slowly, one press every 2 seconds.".into(),
            category: StepCategory::Dpi,
            capture_duration_ms: 10000,
            capture_during_action: true,
        },
        // --- RGB ---
        GuidedStep {
            id: "rgb_off".into(),
            instruction: "Turn OFF all lighting on the mouse. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_white".into(),
            instruction: "Set the mouse lighting to solid WHITE. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_red".into(),
            instruction: "Set the mouse lighting to solid RED. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_green".into(),
            instruction: "Set the mouse lighting to solid GREEN. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_blue".into(),
            instruction: "Set the mouse lighting to solid BLUE. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        // --- Polling Rate ---
        GuidedStep {
            id: "polling_rate_low".into(),
            instruction: "If your mouse supports it, set the polling rate to 125Hz. Press Continue when done (or skip).".into(),
            category: StepCategory::PollingRate,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "polling_rate_high".into(),
            instruction: "Set the polling rate to the highest available (e.g., 1000Hz or 4000Hz). Press Continue when done.".into(),
            category: StepCategory::PollingRate,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        // --- Battery ---
        GuidedStep {
            id: "battery_query".into(),
            instruction: "We'll try to read the battery level. Don't touch the device.".into(),
            category: StepCategory::Battery,
            capture_duration_ms: 3000,
            capture_during_action: false,
        },
    ]
}

fn keyboard_guide() -> Vec<GuidedStep> {
    vec![
        // --- RGB ---
        GuidedStep {
            id: "rgb_off".into(),
            instruction: "Turn OFF all keyboard lighting. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_white".into(),
            instruction: "Set all keys to solid WHITE. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_red".into(),
            instruction: "Set all keys to solid RED. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_green".into(),
            instruction: "Set all keys to solid GREEN. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_blue".into(),
            instruction: "Set all keys to solid BLUE. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        // --- Macro / Key Remap ---
        GuidedStep {
            id: "macro_record".into(),
            instruction: "If your keyboard has dedicated macro keys, press one now. We'll watch for the signal.".into(),
            category: StepCategory::Macro,
            capture_duration_ms: 5000,
            capture_during_action: true,
        },
        // --- Polling Rate ---
        GuidedStep {
            id: "polling_rate_toggle".into(),
            instruction: "If your keyboard supports polling rate changes, switch between the lowest and highest. Press Continue when done.".into(),
            category: StepCategory::PollingRate,
            capture_duration_ms: 4000,
            capture_during_action: false,
        },
    ]
}

fn headset_guide() -> Vec<GuidedStep> {
    vec![
        // --- RGB ---
        GuidedStep {
            id: "rgb_off".into(),
            instruction: "Turn OFF the headset lighting. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_white".into(),
            instruction: "Set the headset lighting to solid WHITE. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        // --- EQ ---
        GuidedStep {
            id: "eq_flat".into(),
            instruction: "Set the equalizer to FLAT / default. Press Continue when done.".into(),
            category: StepCategory::Equalizer,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "eq_bass_boost".into(),
            instruction: "Set the equalizer to a bass-heavy preset. Press Continue when done.".into(),
            category: StepCategory::Equalizer,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        // --- Sidetone ---
        GuidedStep {
            id: "sidetone_off".into(),
            instruction: "Turn sidetone / mic monitoring OFF. Press Continue when done.".into(),
            category: StepCategory::Sidetone,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "sidetone_max".into(),
            instruction: "Turn sidetone / mic monitoring to MAXIMUM. Press Continue when done.".into(),
            category: StepCategory::Sidetone,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        // --- Battery ---
        GuidedStep {
            id: "battery_query".into(),
            instruction: "We'll try to read the battery level. Don't touch the headset.".into(),
            category: StepCategory::Battery,
            capture_duration_ms: 3000,
            capture_during_action: false,
        },
    ]
}

fn mousepad_guide() -> Vec<GuidedStep> {
    vec![
        GuidedStep {
            id: "rgb_off".into(),
            instruction: "Turn OFF the mouse pad lighting. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_white".into(),
            instruction: "Set the mouse pad to solid WHITE. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "rgb_red".into(),
            instruction: "Set the mouse pad to solid RED. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "brightness_min".into(),
            instruction: "Set the brightness to MINIMUM. Press Continue when done.".into(),
            category: StepCategory::Brightness,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "brightness_max".into(),
            instruction: "Set the brightness to MAXIMUM. Press Continue when done.".into(),
            category: StepCategory::Brightness,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
    ]
}

fn smart_light_guide() -> Vec<GuidedStep> {
    vec![
        GuidedStep {
            id: "light_off".into(),
            instruction: "Turn the light OFF. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "light_white".into(),
            instruction: "Set the light to solid WHITE at full brightness. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "light_red".into(),
            instruction: "Set the light to RED. Press Continue when done.".into(),
            category: StepCategory::Rgb,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "brightness_low".into(),
            instruction: "Set brightness to about 10%. Press Continue when done.".into(),
            category: StepCategory::Brightness,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "brightness_full".into(),
            instruction: "Set brightness to 100%. Press Continue when done.".into(),
            category: StepCategory::Brightness,
            capture_duration_ms: 2000,
            capture_during_action: false,
        },
    ]
}

fn generic_guide() -> Vec<GuidedStep> {
    vec![
        GuidedStep {
            id: "action_1".into(),
            instruction: "Change any visible setting on the device (e.g., lighting, sensitivity). Press Continue when done.".into(),
            category: StepCategory::Custom("unknown".into()),
            capture_duration_ms: 3000,
            capture_during_action: false,
        },
        GuidedStep {
            id: "action_2".into(),
            instruction: "Change the setting to a different value. Press Continue when done.".into(),
            category: StepCategory::Custom("unknown".into()),
            capture_duration_ms: 3000,
            capture_during_action: false,
        },
    ]
}
