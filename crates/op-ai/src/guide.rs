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

impl StepCategory {
    /// Short stable name for matching / filtering.
    pub fn name(&self) -> &str {
        match self {
            Self::Baseline => "baseline",
            Self::Rgb => "rgb",
            Self::Dpi => "dpi",
            Self::PollingRate => "polling_rate",
            Self::Macro => "macro",
            Self::Battery => "battery",
            Self::Equalizer => "equalizer",
            Self::Sidetone => "sidetone",
            Self::Brightness => "brightness",
            Self::Custom(s) => s,
        }
    }
}

/// A yes/no capability question shown before the learning steps begin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityQuestion {
    /// Unique question ID (matches the category name it controls).
    pub id: String,
    /// The question text shown to the user.
    pub question: String,
    /// Which step category to disable when the user answers "no".
    pub category: StepCategory,
}

/// Generate the pre-screening questions for a device type.
pub fn generate_questions(device_type: &DeviceType) -> Vec<CapabilityQuestion> {
    match device_type {
        DeviceType::Keyboard => vec![
            CapabilityQuestion {
                id: "rgb".into(),
                question: "Does your keyboard have RGB or backlight lighting?".into(),
                category: StepCategory::Rgb,
            },
            CapabilityQuestion {
                id: "macro".into(),
                question: "Does your keyboard have dedicated macro keys?".into(),
                category: StepCategory::Macro,
            },
            CapabilityQuestion {
                id: "polling_rate".into(),
                question: "Can you change your keyboard's polling rate?".into(),
                category: StepCategory::PollingRate,
            },
        ],
        DeviceType::Mouse => vec![
            CapabilityQuestion {
                id: "rgb".into(),
                question: "Does your mouse have RGB lighting?".into(),
                category: StepCategory::Rgb,
            },
            CapabilityQuestion {
                id: "dpi".into(),
                question: "Can you adjust your mouse's DPI / sensitivity?".into(),
                category: StepCategory::Dpi,
            },
            CapabilityQuestion {
                id: "polling_rate".into(),
                question: "Can you change the polling rate?".into(),
                category: StepCategory::PollingRate,
            },
        ],
        DeviceType::Headset => vec![
            CapabilityQuestion {
                id: "rgb".into(),
                question: "Does your headset have RGB lighting?".into(),
                category: StepCategory::Rgb,
            },
            CapabilityQuestion {
                id: "equalizer".into(),
                question: "Can you adjust an equalizer / EQ presets?".into(),
                category: StepCategory::Equalizer,
            },
            CapabilityQuestion {
                id: "sidetone".into(),
                question: "Does your headset have sidetone / mic monitoring?".into(),
                category: StepCategory::Sidetone,
            },
        ],
        DeviceType::MousePad => vec![
            CapabilityQuestion {
                id: "rgb".into(),
                question: "Does your mouse pad have RGB lighting?".into(),
                category: StepCategory::Rgb,
            },
            CapabilityQuestion {
                id: "brightness".into(),
                question: "Can you adjust the brightness separately from color?".into(),
                category: StepCategory::Brightness,
            },
        ],
        _ => Vec::new(), // No questions for other types; show all steps.
    }
}

/// Filter guided steps, removing any whose category is in `disabled`.
pub fn filter_steps(steps: Vec<GuidedStep>, disabled: &[String]) -> Vec<GuidedStep> {
    steps
        .into_iter()
        .filter(|s| {
            // Always keep Baseline.
            matches!(s.category, StepCategory::Baseline)
                || !disabled.contains(&s.category.name().to_string())
        })
        .collect()
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
        DeviceType::Tablet => {
            steps.extend(tablet_guide());
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

fn tablet_guide() -> Vec<GuidedStep> {
    vec![
        GuidedStep {
            id: "pen_hover".into(),
            instruction: "Hover the pen about 1cm above the centre of the tablet. Press Continue when ready.".into(),
            category: StepCategory::Custom("pressure".into()),
            capture_duration_ms: 3000,
            capture_during_action: true,
        },
        GuidedStep {
            id: "pen_press_light".into(),
            instruction: "Lightly touch the pen to the tablet surface. Press Continue when done.".into(),
            category: StepCategory::Custom("pressure".into()),
            capture_duration_ms: 3000,
            capture_during_action: true,
        },
        GuidedStep {
            id: "pen_press_hard".into(),
            instruction: "Press the pen HARD onto the tablet surface. Press Continue when done.".into(),
            category: StepCategory::Custom("pressure".into()),
            capture_duration_ms: 3000,
            capture_during_action: true,
        },
        GuidedStep {
            id: "pen_tilt".into(),
            instruction: "Tilt the pen at a steep angle while touching the surface. Press Continue when done.".into(),
            category: StepCategory::Custom("tilt".into()),
            capture_duration_ms: 3000,
            capture_during_action: true,
        },
        GuidedStep {
            id: "pen_button".into(),
            instruction: "Press any button on the pen (barrel button). Press Continue when done.".into(),
            category: StepCategory::Custom("pen_button".into()),
            capture_duration_ms: 3000,
            capture_during_action: true,
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
