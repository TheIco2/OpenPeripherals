use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use op_core::device::DeviceType;
use op_core::hid::HidHandle;
use op_core::profile::DeviceProfile;
use op_core::signal::{CapturedReport, SignalCapture};

use super::analyzer::SignalAnalyzer;
use super::guide::{generate_guide, GuidedStep};

/// The current state of a learning session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Waiting for the user to start.
    NotStarted,
    /// Showing the instruction for a step, waiting for user confirmation.
    WaitingForUser { step_index: usize },
    /// Actively capturing signals.
    Capturing { step_index: usize },
    /// All steps complete, analyzing results.
    Analyzing,
    /// Done — profile generated.
    Complete,
}

/// Status update sent to the UI during a learning session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUpdate {
    pub state: SessionState,
    pub current_step: Option<GuidedStep>,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub message: String,
}

/// An interactive AI signal learning session.
///
/// The session:
/// 1. Generates guided steps for the device type
/// 2. Walks the user through each step
/// 3. Captures HID signals at each step
/// 4. After all steps, analyzes the data to detect patterns
/// 5. Exports a DeviceProfile
pub struct LearningSession {
    state: SessionState,
    device_type: DeviceType,
    device_name: String,
    brand: String,
    vid: u16,
    pid: u16,
    steps: Vec<GuidedStep>,
    captures: HashMap<String, Vec<CapturedReport>>,
    skipped_steps: Vec<String>,
}

impl LearningSession {
    pub fn new(
        device_type: DeviceType,
        device_name: String,
        brand: String,
        vid: u16,
        pid: u16,
    ) -> Self {
        let steps = generate_guide(&device_type);
        Self {
            state: SessionState::NotStarted,
            device_type,
            device_name,
            brand,
            vid,
            pid,
            steps,
            captures: HashMap::new(),
            skipped_steps: Vec::new(),
        }
    }

    /// Start the session. Returns the first step instruction.
    pub fn start(&mut self) -> SessionUpdate {
        self.state = SessionState::WaitingForUser { step_index: 0 };
        self.make_update("Session started. Follow the instructions below.")
    }

    /// The user confirms they've performed the current step.
    /// Begins capturing signals for this step.
    pub fn user_ready(&mut self, handle: &HidHandle) -> SessionUpdate {
        let step_index = match &self.state {
            SessionState::WaitingForUser { step_index } => *step_index,
            _ => return self.make_update("Not waiting for user input."),
        };

        let step = &self.steps[step_index];
        self.state = SessionState::Capturing { step_index };

        let duration = Duration::from_millis(step.capture_duration_ms);
        let _update = self.make_update(&format!("Capturing signals for '{}'...", step.id));

        // Perform the capture
        match SignalCapture::capture_passive(handle, duration) {
            Ok(reports) => {
                log::info!(
                    "Captured {} reports for step '{}'",
                    reports.len(),
                    step.id
                );
                self.captures.insert(step.id.clone(), reports);
            }
            Err(e) => {
                log::warn!("Capture failed for step '{}': {e}", step.id);
            }
        }

        // Advance to next step
        self.advance(step_index)
    }

    /// The user wants to skip the current step.
    pub fn skip_step(&mut self) -> SessionUpdate {
        let step_index = match &self.state {
            SessionState::WaitingForUser { step_index } => *step_index,
            _ => return self.make_update("Not waiting for user input."),
        };

        let step_id = self.steps[step_index].id.clone();
        self.skipped_steps.push(step_id);
        self.advance(step_index)
    }

    fn advance(&mut self, current_index: usize) -> SessionUpdate {
        let next_index = current_index + 1;
        if next_index < self.steps.len() {
            self.state = SessionState::WaitingForUser {
                step_index: next_index,
            };
            self.make_update("Ready for next step.")
        } else {
            self.state = SessionState::Analyzing;
            self.make_update("All steps complete. Analyzing captured data...")
        }
    }

    /// Run the analysis and generate a device profile.
    pub fn analyze(&mut self) -> DeviceProfile {
        self.state = SessionState::Analyzing;

        let mut analyzer = SignalAnalyzer::new();
        for (step_id, reports) in &self.captures {
            analyzer.add_capture(step_id, reports.clone());
        }

        let patterns = analyzer.run_all_analyses();
        let capabilities = op_core::device::capabilities_from_patterns(&patterns);

        let mut signals = HashMap::new();
        for pattern in patterns {
            signals.insert(pattern.name.clone(), pattern);
        }

        let profile = DeviceProfile {
            version: 1,
            id: format!(
                "{}-{}-{:#06x}-{:#06x}",
                self.brand.to_lowercase().replace(' ', "-"),
                self.device_name.to_lowercase().replace(' ', "-"),
                self.vid,
                self.pid,
            ),
            device_name: self.device_name.clone(),
            brand: self.brand.clone(),
            vendor_id: self.vid,
            product_ids: vec![self.pid],
            device_type: self.device_type.clone(),
            capabilities,
            signals,
            hid_interfaces: Vec::new(),
            notes: Some(format!(
                "Auto-generated by OpenPeripheral AI learning. Skipped steps: {:?}",
                self.skipped_steps
            )),
        };

        self.state = SessionState::Complete;
        profile
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn current_step(&self) -> Option<&GuidedStep> {
        match &self.state {
            SessionState::WaitingForUser { step_index }
            | SessionState::Capturing { step_index } => self.steps.get(*step_index),
            _ => None,
        }
    }

    fn make_update(&self, message: &str) -> SessionUpdate {
        let completed = match &self.state {
            SessionState::WaitingForUser { step_index } => *step_index,
            SessionState::Capturing { step_index } => *step_index,
            SessionState::Analyzing | SessionState::Complete => self.steps.len(),
            SessionState::NotStarted => 0,
        };

        SessionUpdate {
            state: self.state.clone(),
            current_step: self.current_step().cloned(),
            total_steps: self.steps.len(),
            completed_steps: completed,
            message: message.to_string(),
        }
    }
}
