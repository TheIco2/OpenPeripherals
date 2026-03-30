use std::collections::{HashMap, HashSet};

use op_core::signal::{
    diff_reports, ByteDiff, CapturedReport, ParameterType, SignalParameter, SignalPattern,
    SignalDirection,
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
        log::debug!("analyze_rgb: off={}, white={}, red={}, green={}, blue={} reports",
            off.len(), white.len(), red.len(), green.len(), blue.len());

        // Compare off → white to find which report and bytes are involved
        let off_to_white = diff_reports(off, white);
        log::debug!("analyze_rgb: off→white diffs: {}", off_to_white.len());
        if off_to_white.is_empty() {
            log::debug!("analyze_rgb: no diffs between off and white — aborting");
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

    /// Analyze vendor property responses to detect scalar state changes.
    ///
    /// Instead of looking for specific R/G/B byte layouts, this scans ALL
    /// vendor responses across captures and detects properties (match-keys)
    /// whose value changes between certain steps.  Each changed property
    /// becomes its own pattern (e.g. "set_brightness", "set_mode").
    pub fn analyze_vendor_properties(&self) -> Vec<SignalPattern> {
        let mut patterns = Vec::new();

        // Collect all match_keys that appear across any capture.
        let mut all_keys = HashSet::new();
        for reports in self.captures.values() {
            for r in reports {
                if r.direction == SignalDirection::VendorResponse {
                    if let Some(key) = &r.match_key {
                        all_keys.insert(key.clone());
                    }
                }
            }
        }

        if all_keys.is_empty() {
            return patterns;
        }

        // For each key, collect the response data from every step.
        // key → step_id → data bytes
        let mut key_data: HashMap<String, HashMap<String, Vec<u8>>> = HashMap::new();
        for (step_id, reports) in &self.captures {
            for r in reports {
                if r.direction == SignalDirection::VendorResponse {
                    if let Some(key) = &r.match_key {
                        key_data
                            .entry(key.clone())
                            .or_default()
                            .insert(step_id.clone(), r.data.clone());
                    }
                }
            }
        }

        // Find keys whose response data is NOT identical across all steps.
        for (key, step_map) in &key_data {
            let values: Vec<&Vec<u8>> = step_map.values().collect();
            if values.len() < 2 {
                continue;
            }
            let first = values[0];
            let any_diff = values.iter().skip(1).any(|v| *v != first);
            if !any_diff {
                continue;
            }

            // Skip responses that are error codes (status byte != 0 in V2 protocol).
            // V2 response layout: [report_id, CMD_echo, status, data...].
            // If the first response has a non-zero status byte, it's an error.
            if first.len() >= 3 && first[2] != 0x00 {
                continue;
            }

            // Build a meaningful name from the match key.
            // Keys look like "iface1_get02", "iface2_get0e", etc.
            let label = Self::label_for_property_key(key, first);

            // Find the data offsets that actually changed.
            let min_len = values.iter().map(|v| v.len()).min().unwrap_or(0);
            let mut changed_offsets = Vec::new();
            for offset in 0..min_len {
                let base_val = first[offset];
                if values.iter().skip(1).any(|v| v[offset] != base_val) {
                    changed_offsets.push(offset);
                }
            }

            if changed_offsets.is_empty() {
                continue;
            }

            // Collect (step_name, value_at_changed_offsets) for the description.
            let mut step_vals: Vec<(String, Vec<u8>)> = Vec::new();
            for (step_id, data) in step_map {
                let vals: Vec<u8> = changed_offsets
                    .iter()
                    .map(|&off| *data.get(off).unwrap_or(&0))
                    .collect();
                step_vals.push((step_id.clone(), vals));
            }
            step_vals.sort_by(|a, b| a.0.cmp(&b.0));

            // Determine min / max across the changed bytes (treat as LE u32).
            let val_from = |data: &[u8]| -> u64 {
                let mut v: u64 = 0;
                for (i, &off) in changed_offsets.iter().enumerate() {
                    if i < 4 {
                        v |= (*data.get(off).unwrap_or(&0) as u64) << (i * 8);
                    }
                }
                v
            };
            let all_vals: Vec<u64> = step_map.values().map(|d| val_from(d)).collect();
            let min_val = all_vals.iter().copied().min().unwrap_or(0);
            let max_val = all_vals.iter().copied().max().unwrap_or(0);

            // Use the response from a "high" state as command template.
            // Pick the step with the highest value.
            let template_step = step_map
                .iter()
                .max_by_key(|(_, d)| val_from(d))
                .map(|(_, d)| d.clone())
                .unwrap_or_default();

            // Build the GET command that reads this property.
            // The key format "ifaceN_getXX" encodes the address.
            let get_cmd = Self::build_get_command_from_key(key);

            let params: Vec<SignalParameter> = changed_offsets
                .iter()
                .enumerate()
                .map(|(i, &offset)| SignalParameter {
                    offset,
                    length: 1,
                    name: if changed_offsets.len() == 1 {
                        "value".to_string()
                    } else {
                        format!("byte{i}")
                    },
                    param_type: ParameterType::UInt {
                        min: min_val,
                        max: max_val,
                        big_endian: false,
                    },
                })
                .collect();

            let description = format!(
                "Vendor property '{}' changes across captures (offsets {:?}, range {}–{})",
                key,
                changed_offsets,
                min_val,
                max_val,
            );

            patterns.push(SignalPattern {
                name: label,
                description,
                command_bytes: get_cmd.unwrap_or(template_step),
                expected_response: None,
                parameters: params,
                confidence: 0.65,
            });
        }

        patterns.sort_by(|a, b| a.name.cmp(&b.name));
        patterns
    }

    /// Derive a human-friendly label from a vendor property key + response data.
    fn label_for_property_key(key: &str, _data: &[u8]) -> String {
        // Extract the address hex from keys like "iface1_get02".
        if let Some(addr_hex) = key.strip_prefix("iface1_get")
            .or_else(|| key.strip_prefix("iface2_get"))
        {
            if let Ok(addr) = u8::from_str_radix(addr_hex, 16) {
                return match addr {
                    0x01 => "get_firmware_version".into(),
                    0x02 => "get_brightness".into(),
                    0x03 => "get_render_mode".into(),
                    0x0D => "get_lighting_preset".into(),
                    0x0E => "get_color_value".into(),
                    0x0F => "get_lighting_period".into(),
                    0x10 => "get_lighting_count".into(),
                    0x41 => "get_keyboard_layout".into(),
                    _ => format!("get_property_{addr:02x}"),
                };
            }
        }
        format!("vendor_property_{}", key.replace(['/', '\\', ' '], "_"))
    }

    /// Reconstruct the GET command packet from a match key like "iface1_get02".
    fn build_get_command_from_key(key: &str) -> Option<Vec<u8>> {
        // Parse write_cmd from iface number and address from suffix.
        let rest = key.strip_prefix("iface")?;
        let (iface_str, addr_hex) = if let Some(s) = rest.strip_prefix("1_get") {
            ("1", s)
        } else if let Some(s) = rest.strip_prefix("2_get") {
            ("2", s)
        } else if let Some(s) = rest.strip_prefix("1_wget") {
            // wireless probe
            let addr = u8::from_str_radix(s, 16).ok()?;
            let mut pkt = vec![0u8; 65];
            pkt[0] = 0x00;
            pkt[1] = 0x09; // wireless
            pkt[2] = 0x02; // GET
            pkt[3] = addr;
            return Some(pkt);
        } else {
            return None;
        };

        let addr = u8::from_str_radix(addr_hex, 16).ok()?;
        let mut pkt = vec![0u8; 65];
        pkt[0] = 0x00;
        pkt[1] = 0x08; // wired
        pkt[2] = 0x02; // GET
        pkt[3] = addr;
        let _ = iface_str; // used only for the prefix matching
        Some(pkt)
    }

    /// Run all available analyses and return detected patterns.
    pub fn run_all_analyses(&self) -> Vec<SignalPattern> {
        log::info!("SignalAnalyzer: running analyses on {} step captures", self.captures.len());
        for (step_id, reports) in &self.captures {
            let feat = reports.iter().filter(|r| r.direction == op_core::signal::SignalDirection::FeatureReport).count();
            let vendor = reports.iter().filter(|r| r.direction == op_core::signal::SignalDirection::VendorResponse).count();
            let other = reports.len() - feat - vendor;
            log::info!("  step '{}': {} reports ({} feature, {} vendor, {} other)", step_id, reports.len(), feat, vendor, other);
        }

        // Log diffs between key step pairs for diagnostics.
        let pairs = [
            ("rgb_off", "rgb_white"),
            ("rgb_off", "rgb_red"),
            ("rgb_white", "rgb_red"),
            ("baseline_idle", "rgb_off"),
        ];
        for (a, b) in &pairs {
            if let Some(d) = self.compare(a, b) {
                log::info!("  diff {} → {}: {} byte differences", a, b, d.len());
                for dd in d.iter().take(10) {
                    log::info!(
                        "    report[{}] offset {} : 0x{:02X} → 0x{:02X}",
                        dd.report_index, dd.offset, dd.old_value, dd.new_value,
                    );
                }
                if d.len() > 10 {
                    log::info!("    ... and {} more", d.len() - 10);
                }
            }
        }

        let mut patterns = Vec::new();

        match self.analyze_rgb() {
            Some(p) => {
                log::info!("RGB pattern detected: {} params, confidence {}", p.parameters.len(), p.confidence);
                patterns.push(p);
            }
            None => log::info!("RGB analysis: no pattern found"),
        }
        match self.analyze_dpi() {
            Some(p) => {
                log::info!("DPI pattern detected: {} params, confidence {}", p.parameters.len(), p.confidence);
                patterns.push(p);
            }
            None => log::info!("DPI analysis: no pattern found"),
        }
        match self.analyze_polling_rate() {
            Some(p) => {
                log::info!("Polling rate pattern detected: {} params, confidence {}", p.parameters.len(), p.confidence);
                patterns.push(p);
            }
            None => log::info!("Polling rate analysis: no pattern found"),
        }

        // Vendor property analysis: detect any V2 properties that changed
        // between capture steps (brightness, mode, etc.).
        let vendor_props = self.analyze_vendor_properties();
        if vendor_props.is_empty() {
            log::info!("Vendor property analysis: no changed properties found");
        } else {
            for p in &vendor_props {
                log::info!(
                    "Vendor property detected: '{}' ({} params, confidence {:.2})",
                    p.name, p.parameters.len(), p.confidence,
                );
            }
            patterns.extend(vendor_props);
        }

        log::info!("Total patterns detected: {}", patterns.len());
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
