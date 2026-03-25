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
pub fn diff_reports(baseline: &[CapturedReport], changed: &[CapturedReport]) -> Vec<ByteDiff> {
    let mut diffs = Vec::new();

    // Compare reports pairwise (same index)
    for (i, (base, chg)) in baseline.iter().zip(changed.iter()).enumerate() {
        if base.direction != chg.direction {
            continue;
        }
        let len = base.data.len().min(chg.data.len());
        for offset in 0..len {
            if base.data[offset] != chg.data[offset] {
                diffs.push(ByteDiff {
                    report_index: i,
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
