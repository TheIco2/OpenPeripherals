use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use op_core::device::DeviceType;
use op_core::hid::HidHandle;
use op_core::profile::DeviceProfile;
use op_core::signal::{CapturedReport, SignalCapture, SignalPattern};

use super::analyzer::SignalAnalyzer;
use super::guide::{
    filter_steps, generate_guide, generate_questions, CapabilityQuestion, GuidedStep,
};

/// The current state of a learning session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Waiting for the user to start.
    NotStarted,
    /// Pre-screening: asking the user about device capabilities.
    AskingQuestions { question_index: usize },
    /// Showing the instruction for a step, waiting for user confirmation.
    WaitingForUser { step_index: usize },
    /// Actively capturing signals.
    Capturing { step_index: usize },
    /// All steps complete, analyzing results.
    Analyzing,
    /// Verifying a detected pattern — sending test command and awaiting user confirmation.
    Verifying { pattern_index: usize },
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
    /// Number of HID reports captured in the last step (0 if not applicable).
    pub last_capture_count: usize,
    /// Feature report probe diagnostics: (attempted, succeeded).
    pub last_feature_probes: (usize, usize),
    /// Number of interrupt reads that returned data in the last step.
    pub last_interrupt_reads: usize,
    // --- Questionnaire fields ---
    pub current_question: Option<CapabilityQuestion>,
    pub total_questions: usize,
    pub question_index: usize,
    // --- Verification fields ---
    pub verify_pattern_name: Option<String>,
    pub verify_description: Option<String>,
    pub verify_index: usize,
    pub verify_total: usize,
}

/// An interactive AI signal learning session.
pub struct LearningSession {
    state: SessionState,
    device_type: DeviceType,
    device_name: String,
    brand: String,
    vid: u16,
    pid: u16,
    questions: Vec<CapabilityQuestion>,
    disabled_categories: Vec<String>,
    steps: Vec<GuidedStep>,
    captures: HashMap<String, Vec<CapturedReport>>,
    skipped_steps: Vec<String>,
    log: Vec<String>,
    /// Patterns detected by the analyzer, stored for verification phase.
    detected_patterns: Vec<SignalPattern>,
    /// (pattern_name, user_confirmed)
    verified: Vec<(String, bool)>,
}

impl LearningSession {
    pub fn new(
        device_type: DeviceType,
        device_name: String,
        brand: String,
        vid: u16,
        pid: u16,
    ) -> Self {
        let questions = generate_questions(&device_type);
        let steps = generate_guide(&device_type);
        Self {
            state: SessionState::NotStarted,
            device_type,
            device_name,
            brand,
            vid,
            pid,
            questions,
            disabled_categories: Vec::new(),
            steps,
            captures: HashMap::new(),
            skipped_steps: Vec::new(),
            log: Vec::new(),
            detected_patterns: Vec::new(),
            verified: Vec::new(),
        }
    }

    /// Append a diagnostic entry to the session log.
    pub fn log(&mut self, entry: impl Into<String>) {
        self.log.push(entry.into());
    }

    // ------------------------------------------------------------------
    // Questionnaire phase
    // ------------------------------------------------------------------

    /// Start the session.  Begins with the questionnaire if questions exist,
    /// otherwise jumps straight to the first guided step.
    pub fn start(&mut self) -> SessionUpdate {
        if self.questions.is_empty() {
            self.state = SessionState::WaitingForUser { step_index: 0 };
            self.make_update("Session started. Follow the instructions below.")
        } else {
            self.state = SessionState::AskingQuestions { question_index: 0 };
            self.make_update("First, a few quick questions about your device.")
        }
    }

    /// User answered a capability question.
    /// `yes` = true means the device has this capability.
    pub fn answer_question(&mut self, yes: bool) -> SessionUpdate {
        let qi = match &self.state {
            SessionState::AskingQuestions { question_index } => *question_index,
            _ => return self.make_update("Not in questionnaire phase."),
        };

        let q = &self.questions[qi];
        if !yes {
            let cat_name = q.category.name().to_string();
            self.log.push(format!("question '{}': user said NO → skip {} steps", q.id, cat_name));
            self.disabled_categories.push(cat_name);
        } else {
            self.log.push(format!("question '{}': user said YES", q.id));
        }

        let next = qi + 1;
        if next < self.questions.len() {
            self.state = SessionState::AskingQuestions { question_index: next };
            self.make_update("Next question.")
        } else {
            // Done with questions — filter steps and start learning.
            self.steps = filter_steps(
                std::mem::take(&mut self.steps),
                &self.disabled_categories,
            );
            self.log.push(format!(
                "questionnaire done: {} steps remaining after filtering",
                self.steps.len(),
            ));
            self.state = SessionState::WaitingForUser { step_index: 0 };
            self.make_update("Great! Let's begin the learning steps.")
        }
    }

    // ------------------------------------------------------------------
    // Guided step phase
    // ------------------------------------------------------------------

    /// The user confirms they've performed the current step.
    pub fn user_ready(&mut self, handle: &HidHandle) -> SessionUpdate {
        let step_index = match &self.state {
            SessionState::WaitingForUser { step_index } => *step_index,
            _ => return self.make_update("Not waiting for user input."),
        };

        let step = &self.steps[step_index];
        self.state = SessionState::Capturing { step_index };

        let duration = Duration::from_millis(step.capture_duration_ms);

        match SignalCapture::capture_passive(handle, duration) {
            Ok(reports) => {
                log::info!("Captured {} reports for step '{}'", reports.len(), step.id);
                self.captures.insert(step.id.clone(), reports);
            }
            Err(e) => {
                log::warn!("Capture failed for step '{}': {e}", step.id);
            }
        }

        self.advance(step_index)
    }

    /// Like `user_ready` but captures from ALL HID interfaces with feature-report
    /// probing and vendor active probing.
    pub fn user_ready_multi(&mut self, handles: &[HidHandle]) -> SessionUpdate {
        let step_index = match &self.state {
            SessionState::WaitingForUser { step_index } => *step_index,
            _ => return self.make_update("Not waiting for user input."),
        };

        let step = &self.steps[step_index];
        self.state = SessionState::Capturing { step_index };

        let duration = Duration::from_millis(step.capture_duration_ms);

        let result = SignalCapture::capture_full(handles, duration);
        let d = &result.diagnostics;
        let step_log = format!(
            "step '{}': {} reports (feat {}/{}, vendor {}/{}, intr {}) across {} iface(s)",
            step.id,
            result.reports.len(),
            d.feature_probes_succeeded,
            d.feature_probes_attempted,
            d.vendor_probes_responded,
            d.vendor_probes_sent,
            d.interrupt_reads,
            d.interfaces_used,
        );
        log::info!("{step_log}");
        self.log.push(step_log);
        self.captures.insert(step.id.clone(), result.reports);

        let mut update = self.advance(step_index);
        update.last_feature_probes = (
            d.feature_probes_attempted,
            d.feature_probes_succeeded,
        );
        update.last_interrupt_reads = d.interrupt_reads;
        update
    }

    /// The user wants to skip the current step.
    pub fn skip_step(&mut self) -> SessionUpdate {
        let step_index = match &self.state {
            SessionState::WaitingForUser { step_index } => *step_index,
            _ => return self.make_update("Not waiting for user input."),
        };

        let step_id = self.steps[step_index].id.clone();
        self.log.push(format!("step '{}': skipped by user", step_id));
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

    // ------------------------------------------------------------------
    // Analysis + verification phase
    // ------------------------------------------------------------------

    /// Run the analysis.  If patterns are detected, enters `Verifying` state
    /// so the user can confirm each one.  Returns the update + the detected
    /// patterns so the caller can send test commands.
    pub fn analyze(&mut self) -> SessionUpdate {
        self.state = SessionState::Analyzing;

        self.log.push(format!(
            "analyze: {} step captures ({} total reports)",
            self.captures.len(),
            self.captures.values().map(|v| v.len()).sum::<usize>(),
        ));

        // Log per-step report breakdown.
        for (step_id, reports) in &self.captures {
            let feat = reports.iter().filter(|r| r.direction == op_core::signal::SignalDirection::FeatureReport).count();
            let vendor = reports.iter().filter(|r| r.direction == op_core::signal::SignalDirection::VendorResponse).count();
            let other = reports.len() - feat - vendor;
            self.log.push(format!(
                "  step '{}': {} reports ({} feature, {} vendor, {} other)",
                step_id, reports.len(), feat, vendor, other,
            ));
        }

        let mut analyzer = SignalAnalyzer::new();
        for (step_id, reports) in &self.captures {
            analyzer.add_capture(step_id, reports.clone());
        }

        // Log key diffs for diagnostics.
        let pairs = [
            ("rgb_off", "rgb_white"),
            ("rgb_off", "rgb_red"),
            ("baseline_idle", "rgb_off"),
        ];
        for (a, b) in &pairs {
            if let Some(diffs) = analyzer.compare(a, b) {
                self.log.push(format!("diff {} → {}: {} byte differences", a, b, diffs.len()));
                for d in diffs.iter().take(8) {
                    self.log.push(format!(
                        "  report[{}] offset {} : 0x{:02X} → 0x{:02X}",
                        d.report_index, d.offset, d.old_value, d.new_value,
                    ));
                }
                if diffs.len() > 8 {
                    self.log.push(format!("  ... and {} more", diffs.len() - 8));
                }
            }
        }

        // Log first few bytes of each vendor response in baseline for raw inspection.
        if let Some(baseline) = self.captures.get("baseline_idle") {
            for r in baseline.iter().filter(|r| r.direction == op_core::signal::SignalDirection::VendorResponse) {
                let hex: String = r.data.iter().take(16).map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
                let key = r.match_key.as_deref().unwrap_or("?");
                self.log.push(format!("  baseline vendor [{}]: {}", key, hex));
            }
        }

        let patterns = analyzer.run_all_analyses();

        if patterns.is_empty() {
            self.log.push("analyze: NO patterns detected — profile will be empty".into());
            self.detected_patterns = Vec::new();
            self.state = SessionState::Complete;
            self.make_update("Analysis complete — no patterns could be detected.")
        } else {
            for p in &patterns {
                self.log.push(format!(
                    "analyze: detected '{}' (confidence {:.2}, {} params)",
                    p.name, p.confidence, p.parameters.len(),
                ));
            }
            self.detected_patterns = patterns;
            self.state = SessionState::Verifying { pattern_index: 0 };
            self.make_update("Patterns detected! Let's verify them.")
        }
    }

    /// Get the pattern currently awaiting verification (so the caller can
    /// send its test command to the device).
    pub fn current_verification_pattern(&self) -> Option<&SignalPattern> {
        match &self.state {
            SessionState::Verifying { pattern_index } => {
                self.detected_patterns.get(*pattern_index)
            }
            _ => None,
        }
    }

    /// User answers whether the test command worked.
    pub fn verify_result(&mut self, confirmed: bool) -> SessionUpdate {
        let pi = match &self.state {
            SessionState::Verifying { pattern_index } => *pattern_index,
            _ => return self.make_update("Not in verification phase."),
        };

        let name = self.detected_patterns[pi].name.clone();
        self.log.push(format!(
            "verify '{}': {}",
            name,
            if confirmed { "CONFIRMED" } else { "REJECTED" },
        ));
        self.verified.push((name, confirmed));

        let next = pi + 1;
        if next < self.detected_patterns.len() {
            self.state = SessionState::Verifying { pattern_index: next };
            self.make_update("Next pattern to verify.")
        } else {
            self.state = SessionState::Complete;
            self.make_update("Verification complete!")
        }
    }

    /// Build the final profile (call after `Complete` state).
    pub fn build_profile(&mut self) -> DeviceProfile {
        let capabilities =
            op_core::device::capabilities_from_patterns(&self.detected_patterns);

        // Only include patterns the user confirmed (or all if no verification occurred).
        let confirmed_names: Vec<&str> = if self.verified.is_empty() {
            self.detected_patterns.iter().map(|p| p.name.as_str()).collect()
        } else {
            self.verified
                .iter()
                .filter(|(_, ok)| *ok)
                .map(|(n, _)| n.as_str())
                .collect()
        };

        let mut signals = HashMap::new();
        for p in &self.detected_patterns {
            if confirmed_names.contains(&p.name.as_str()) {
                signals.insert(p.name.clone(), p.clone());
            }
        }

        DeviceProfile {
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
                self.skipped_steps,
            )),
            learning_log: std::mem::take(&mut self.log),
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

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
            _ => 0,
        };

        let last_capture_count = if completed > 0 && completed <= self.steps.len() {
            let prev_step = &self.steps[completed - 1];
            self.captures
                .get(&prev_step.id)
                .map(|r| r.len())
                .unwrap_or(0)
        } else {
            0
        };

        // Questionnaire fields.
        let (current_question, question_index, total_questions) = match &self.state {
            SessionState::AskingQuestions { question_index: qi } => (
                self.questions.get(*qi).cloned(),
                *qi,
                self.questions.len(),
            ),
            _ => (None, 0, self.questions.len()),
        };

        // Verification fields.
        let (vp_name, vp_desc, vi, vt) = match &self.state {
            SessionState::Verifying { pattern_index: pi } => {
                let p = self.detected_patterns.get(*pi);
                let name = p.map(|p| p.name.clone());
                let desc = p.map(|p| verification_prompt(&p.name));
                (name, desc, *pi, self.detected_patterns.len())
            }
            _ => (None, None, 0, self.detected_patterns.len()),
        };

        SessionUpdate {
            state: self.state.clone(),
            current_step: self.current_step().cloned(),
            total_steps: self.steps.len(),
            completed_steps: completed,
            message: message.to_string(),
            last_capture_count,
            last_feature_probes: (0, 0),
            last_interrupt_reads: 0,
            current_question,
            total_questions,
            question_index,
            verify_pattern_name: vp_name,
            verify_description: vp_desc,
            verify_index: vi,
            verify_total: vt,
        }
    }
}

/// Human-readable prompt for verifying a pattern.
fn verification_prompt(pattern_name: &str) -> String {
    match pattern_name {
        "set_rgb" => "We sent a command to set the lighting to RED. Did the device's lighting change?".into(),
        "set_dpi" => "We sent a command to change the DPI. Did you notice the mouse sensitivity change?".into(),
        "set_polling_rate" => "We sent a command to change the polling rate. Did you notice any difference?".into(),
        "get_brightness" => "We sent a command to turn off the keyboard brightness. Did the lighting turn off?".into(),
        "get_render_mode" => "We toggled the render mode. Did you notice any lighting change?".into(),
        other if other.starts_with("get_") => format!("We changed a device property ('{other}'). Did you notice any change on the device?"),
        other => format!("We sent a test command for '{other}'. Did you notice any change on the device?"),
    }
}

/// Build the test command bytes for a detected pattern.
/// Returns `(command_bytes, human_readable_description)`.
///
/// For Corsair V2 vendor properties (names starting with "get_"), we build a
/// SET command (CMD 0x01) that writes a test value to the same property address.
/// The original pattern stores the GET command which is read-only.
pub fn test_command_for_pattern(pattern: &SignalPattern) -> (Vec<u8>, String) {
    let mut cmd = pattern.command_bytes.clone();
    let desc = match pattern.name.as_str() {
        "set_rgb" => {
            for param in &pattern.parameters {
                match param.name.as_str() {
                    "red" => cmd[param.offset] = 255,
                    "green" | "blue" => cmd[param.offset] = 0,
                    _ => {}
                }
            }
            "Setting lighting to RED…"
        }
        "set_dpi" => "Sending a DPI change command…",
        "set_polling_rate" => "Sending a polling rate change command…",
        _ => {
            // For Corsair V2 vendor properties: convert the GET command into a SET.
            // GET format: [0x00, write_cmd, 0x02, addr, ...]
            // SET format: [0x00, write_cmd, 0x01, addr, value_bytes...]
            if cmd.len() >= 4 && (cmd[1] == 0x08 || cmd[1] == 0x09) && cmd[2] == 0x02 {
                cmd[2] = 0x01; // Change CMD from GET (0x02) to SET (0x01)
                let addr = cmd[3];
                match addr {
                    // Brightness (addr 0x02): set to 0 to turn off LEDs
                    0x02 => {
                        cmd[4] = 0x00; // value LE: 0
                        cmd[5] = 0x00;
                    }
                    // Render mode (addr 0x03): toggle to HW mode (0x01)
                    0x03 => {
                        cmd[4] = 0x01;
                    }
                    // Default: set value to 0
                    _ => {
                        cmd[4] = 0x00;
                    }
                }
                "Sending V2 SET command…"
            } else {
                "Sending test command…"
            }
        }
    };
    (cmd, desc.to_string())
}
