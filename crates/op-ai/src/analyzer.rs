use std::collections::HashMap;

use op_core::signal::{
    diff_reports, ByteDiff, CapturedReport, ParameterType, SignalParameter, SignalPattern,
};

/// Analyze captured signal data to identify patterns.
///
/// This is the core AI analysis engine. It compares signal captures from different
/// device states to determine which bytes correspond to which settings.
pub struct SignalAnalyzer {
    /// Named captures: step_id -> captured reports.
    captures: HashMap<String, Vec<CapturedReport>>,
}

impl SignalAnalyzer {
    pub fn new() -> Self {
        Self {
            captures: HashMap::new(),
        }
    }

    /// Store a capture for a given step.
    pub fn add_capture(&mut self, step_id: &str, reports: Vec<CapturedReport>) {
        self.captures.insert(step_id.to_string(), reports);
    }

    /// Compare two captures and find which bytes changed.
    pub fn compare(&self, step_a: &str, step_b: &str) -> Option<Vec<ByteDiff>> {
        let a = self.captures.get(step_a)?;
        let b = self.captures.get(step_b)?;
        Some(diff_reports(a, b))
    }

    /// Analyze RGB color captures to detect color byte positions.
    ///
    /// Expects captures for: "rgb_off", "rgb_white", "rgb_red", "rgb_green", "rgb_blue".
    /// Returns a signal pattern for setting RGB if the pattern is detected.
    pub fn analyze_rgb(&self) -> Option<SignalPattern> {
        let off = self.captures.get("rgb_off")?;
        let white = self.captures.get("rgb_white")?;
        let red = self.captures.get("rgb_red")?;
        let green = self.captures.get("rgb_green")?;
        let blue = self.captures.get("rgb_blue")?;

        // Compare off → white to find which report and bytes are involved
        let off_to_white = diff_reports(off, white);
        if off_to_white.is_empty() {
            return None;
        }

        // Compare white → red: bytes that went from 0xFF to 0x00 are G and B
        let white_to_red = diff_reports(white, red);
        // Compare white → green: bytes that went from 0xFF to 0x00 are R and B
        let white_to_green = diff_reports(white, green);
        // Compare white → blue: bytes that went from 0xFF to 0x00 are R and G
        let white_to_blue = diff_reports(white, blue);

        // Find the report index that has consistent changes
        let report_idx = off_to_white.first()?.report_index;

        // Find R byte: changed in white→green and white→blue but NOT white→red
        let r_byte = find_color_byte(&white_to_red, &white_to_green, &white_to_blue, report_idx, false, true, true);
        // Find G byte: changed in white→red and white→blue but NOT white→green
        let g_byte = find_color_byte(&white_to_red, &white_to_green, &white_to_blue, report_idx, true, false, true);
        // Find B byte: changed in white→red and white→green but NOT white→blue
        let b_byte = find_color_byte(&white_to_red, &white_to_green, &white_to_blue, report_idx, true, true, false);

        let (r_off, g_off, b_off) = match (r_byte, g_byte, b_byte) {
            (Some(r), Some(g), Some(b)) => (r, g, b),
            _ => return None,
        };

        // Build the command template from the "red" capture (known state)
        let command_bytes = if let Some(report) = red.get(report_idx) {
            report.data.clone()
        } else {
            return None;
        };

        Some(SignalPattern {
            name: "set_rgb".to_string(),
            description: "Set the RGB color of the device".to_string(),
            command_bytes,
            expected_response: None,
            parameters: vec![
                SignalParameter {
                    offset: r_off,
                    length: 1,
                    name: "red".to_string(),
                    param_type: ParameterType::ColorComponent,
                },
                SignalParameter {
                    offset: g_off,
                    length: 1,
                    name: "green".to_string(),
                    param_type: ParameterType::ColorComponent,
                },
                SignalParameter {
                    offset: b_off,
                    length: 1,
                    name: "blue".to_string(),
                    param_type: ParameterType::ColorComponent,
                },
            ],
            confidence: 0.85,
        })
    }

    /// Analyze DPI captures to detect DPI value byte positions.
    ///
    /// Expects captures for: "dpi_lowest", "dpi_highest".
    pub fn analyze_dpi(&self) -> Option<SignalPattern> {
        let low = self.captures.get("dpi_lowest")?;
        let high = self.captures.get("dpi_highest")?;

        let diffs = diff_reports(low, high);
        if diffs.is_empty() {
            return None;
        }

        // Look for bytes with the largest value difference (likely the DPI value)
        let report_idx = diffs.first()?.report_index;
        let relevant: Vec<_> = diffs
            .iter()
            .filter(|d| d.report_index == report_idx)
            .collect();

        // DPI is typically 1-2 bytes
        let command_bytes = if let Some(report) = high.get(report_idx) {
            report.data.clone()
        } else {
            return None;
        };

        let params: Vec<_> = relevant
            .iter()
            .take(2) // DPI is usually 1-2 bytes
            .enumerate()
            .map(|(i, diff)| SignalParameter {
                offset: diff.offset,
                length: 1,
                name: if i == 0 {
                    "dpi_value_high".to_string()
                } else {
                    "dpi_value_low".to_string()
                },
                param_type: ParameterType::UInt {
                    min: diff.old_value as u64,
                    max: diff.new_value as u64,
                    big_endian: true,
                },
            })
            .collect();

        Some(SignalPattern {
            name: "set_dpi".to_string(),
            description: "Set the DPI / sensitivity of the device".to_string(),
            command_bytes,
            expected_response: None,
            parameters: params,
            confidence: 0.7,
        })
    }

    /// Analyze polling rate captures.
    pub fn analyze_polling_rate(&self) -> Option<SignalPattern> {
        let low = self.captures.get("polling_rate_low")?;
        let high = self.captures.get("polling_rate_high")?;

        let diffs = diff_reports(low, high);
        if diffs.is_empty() {
            return None;
        }

        let report_idx = diffs.first()?.report_index;
        let command_bytes = if let Some(report) = high.get(report_idx) {
            report.data.clone()
        } else {
            return None;
        };

        let params: Vec<_> = diffs
            .iter()
            .filter(|d| d.report_index == report_idx)
            .take(1)
            .map(|diff| SignalParameter {
                offset: diff.offset,
                length: 1,
                name: "polling_rate_value".to_string(),
                param_type: ParameterType::UInt {
                    min: diff.old_value as u64,
                    max: diff.new_value as u64,
                    big_endian: false,
                },
            })
            .collect();

        Some(SignalPattern {
            name: "set_polling_rate".to_string(),
            description: "Set the USB polling rate".to_string(),
            command_bytes,
            expected_response: None,
            parameters: params,
            confidence: 0.6,
        })
    }

    /// Run all available analyses and return detected patterns.
    pub fn run_all_analyses(&self) -> Vec<SignalPattern> {
        let mut patterns = Vec::new();

        if let Some(p) = self.analyze_rgb() {
            patterns.push(p);
        }
        if let Some(p) = self.analyze_dpi() {
            patterns.push(p);
        }
        if let Some(p) = self.analyze_polling_rate() {
            patterns.push(p);
        }

        patterns
    }
}

impl Default for SignalAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper: find a color byte offset using cross-comparison of diffs.
fn find_color_byte(
    white_to_red: &[ByteDiff],
    white_to_green: &[ByteDiff],
    white_to_blue: &[ByteDiff],
    report_idx: usize,
    changed_in_red: bool,
    changed_in_green: bool,
    changed_in_blue: bool,
) -> Option<usize> {
    // Collect byte offsets that changed in each comparison
    let red_offsets: Vec<usize> = white_to_red
        .iter()
        .filter(|d| d.report_index == report_idx)
        .map(|d| d.offset)
        .collect();
    let green_offsets: Vec<usize> = white_to_green
        .iter()
        .filter(|d| d.report_index == report_idx)
        .map(|d| d.offset)
        .collect();
    let blue_offsets: Vec<usize> = white_to_blue
        .iter()
        .filter(|d| d.report_index == report_idx)
        .map(|d| d.offset)
        .collect();

    // Build a set of all candidate offsets
    let mut candidates = std::collections::HashSet::new();
    candidates.extend(&red_offsets);
    candidates.extend(&green_offsets);
    candidates.extend(&blue_offsets);

    for &offset in &candidates {
        let in_red = red_offsets.contains(&offset);
        let in_green = green_offsets.contains(&offset);
        let in_blue = blue_offsets.contains(&offset);

        if in_red == changed_in_red && in_green == changed_in_green && in_blue == changed_in_blue {
            return Some(offset);
        }
    }

    None
}
