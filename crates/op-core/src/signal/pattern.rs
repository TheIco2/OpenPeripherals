use serde::{Deserialize, Serialize};

use super::CapturedReport;

/// A recognized signal pattern extracted from captured data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalPattern {
    /// Human-readable name (e.g., "set_dpi", "set_rgb_zone_0").
    pub name: String,
    /// Description of what this signal does.
    pub description: String,
    /// The byte pattern that triggers this action (sent to device).
    pub command_bytes: Vec<u8>,
    /// Expected response pattern, if any.
    pub expected_response: Option<Vec<u8>>,
    /// Which bytes are parameters (index + description).
    pub parameters: Vec<SignalParameter>,
    /// Confidence score from AI analysis (0.0 – 1.0).
    pub confidence: f32,
}

/// A parameter within a signal pattern — a byte (or range of bytes) that varies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalParameter {
    /// Byte offset in the command where this parameter starts.
    pub offset: usize,
    /// Number of bytes this parameter occupies.
    pub length: usize,
    /// Human-readable name (e.g., "dpi_value", "red", "green", "blue").
    pub name: String,
    /// The type/interpretation of this parameter.
    pub param_type: ParameterType,
}

/// How to interpret a signal parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterType {
    /// Unsigned integer (big-endian or little-endian).
    UInt { min: u64, max: u64, big_endian: bool },
    /// A single byte representing an RGB component.
    ColorComponent,
    /// An enumerated value with known mappings.
    Enum(Vec<EnumVariant>),
    /// Raw bytes with unknown interpretation.
    Raw,
}

/// A known value for an enum parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    pub value: u8,
    pub name: String,
}

/// Compare two sets of captured reports and find bytes that changed.
///
/// Feature reports are matched by their report ID (first byte) and data length.
/// Vendor responses are matched by their `match_key` (interface + command ID).
/// Interrupt/other reports are matched positionally within their own group.
pub fn diff_reports(baseline: &[CapturedReport], changed: &[CapturedReport]) -> Vec<ByteDiff> {
    use super::SignalDirection;
    let mut diffs = Vec::new();

    // Phase 1: match FeatureReports by (report_id, data_length).
    let base_features: Vec<_> = baseline
        .iter()
        .enumerate()
        .filter(|(_, r)| r.direction == SignalDirection::FeatureReport && !r.data.is_empty())
        .collect();
    let chg_features: Vec<_> = changed
        .iter()
        .enumerate()
        .filter(|(_, r)| r.direction == SignalDirection::FeatureReport && !r.data.is_empty())
        .collect();

    for &(bi, base) in &base_features {
        let report_id = base.data[0];
        let base_len = base.data.len();
        // Find matching feature report in the changed set.
        if let Some(&(_, chg)) = chg_features.iter().find(|(_, r)| {
            r.data.len() == base_len && r.data[0] == report_id
        }) {
            for offset in 1..base_len {
                if base.data[offset] != chg.data[offset] {
                    diffs.push(ByteDiff {
                        report_index: bi,
                        offset,
                        old_value: base.data[offset],
                        new_value: chg.data[offset],
                    });
                }
            }
        }
    }

    // Phase 2: match VendorResponse reports by match_key.
    let base_vendor: Vec<_> = baseline
        .iter()
        .enumerate()
        .filter(|(_, r)| r.direction == SignalDirection::VendorResponse && r.match_key.is_some())
        .collect();
    let chg_vendor: Vec<_> = changed
        .iter()
        .filter(|r| r.direction == SignalDirection::VendorResponse && r.match_key.is_some())
        .collect();

    for &(bi, base) in &base_vendor {
        let key = base.match_key.as_ref().unwrap();
        if let Some(chg) = chg_vendor.iter().find(|r| r.match_key.as_ref() == Some(key)) {
            let len = base.data.len().min(chg.data.len());
            for offset in 0..len {
                if base.data[offset] != chg.data[offset] {
                    diffs.push(ByteDiff {
                        report_index: bi,
                        offset,
                        old_value: base.data[offset],
                        new_value: chg.data[offset],
                    });
                }
            }
        }
    }

    // Phase 3: positionally diff non-feature, non-vendor reports (interrupt reads).
    let base_other: Vec<_> = baseline
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.direction != SignalDirection::FeatureReport
                && r.direction != SignalDirection::VendorResponse
        })
        .collect();
    let chg_other: Vec<_> = changed
        .iter()
        .filter(|r| {
            r.direction != SignalDirection::FeatureReport
                && r.direction != SignalDirection::VendorResponse
        })
        .collect();

    for (&(bi, base), chg) in base_other.iter().zip(chg_other.iter()) {
        let len = base.data.len().min(chg.data.len());
        for offset in 0..len {
            if base.data[offset] != chg.data[offset] {
                diffs.push(ByteDiff {
                    report_index: bi,
                    offset,
                    old_value: base.data[offset],
                    new_value: chg.data[offset],
                });
            }
        }
    }

    diffs
}

/// A single byte that differs between two captured report sets.
#[derive(Debug, Clone)]
pub struct ByteDiff {
    pub report_index: usize,
    pub offset: usize,
    pub old_value: u8,
    pub new_value: u8,
}
