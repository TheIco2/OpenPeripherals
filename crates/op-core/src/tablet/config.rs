use serde::{Deserialize, Serialize};

/// Configuration sourced from an OpenTabletDriver tablet definition.
///
/// OpenTabletDriver stores tablet definitions as JSON files with fields like
/// `Name`, `DigitizerIdentifiers`, `AuxiliaryDeviceIdentifiers`, pen specs, etc.
/// This struct maps those into OpenPeripheral's representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtdTabletConfig {
    /// Tablet model name (from OTD's "Name" field).
    pub name: String,
    /// Vendor ID from the digitizer identifier.
    pub vendor_id: u16,
    /// Product ID from the digitizer identifier.
    pub product_id: u16,
    /// Input report length expected by the device.
    pub input_report_length: Option<u32>,
    /// Output report length the device accepts.
    pub output_report_length: Option<u32>,
    /// Maximum X coordinate the digitizer reports.
    pub max_x: f64,
    /// Maximum Y coordinate the digitizer reports.
    pub max_y: f64,
    /// Maximum pressure level the pen reports.
    pub max_pressure: u32,
    /// Physical width of the active area in mm.
    pub width_mm: f64,
    /// Physical height of the active area in mm.
    pub height_mm: f64,
    /// Auxiliary device identifiers (e.g., tablet buttons pad).
    pub auxiliary_ids: Vec<OtdDeviceId>,
    /// Pen button count.
    pub pen_buttons: u32,
    /// Auxiliary button count (express keys on the tablet itself).
    pub aux_buttons: u32,
}

/// A USB device identifier from OpenTabletDriver configs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtdDeviceId {
    pub vendor_id: u16,
    pub product_id: u16,
    pub input_report_length: Option<u32>,
    pub output_report_length: Option<u32>,
}

/// Parsed representation of OTD's JSON format for tablet configurations.
///
/// This matches the schema that OpenTabletDriver uses in its
/// `OpenTabletDriver.Configurations/Configurations/` directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtdRawConfig {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "DigitizerIdentifiers")]
    pub digitizer_identifiers: Vec<OtdRawIdentifier>,
    #[serde(rename = "AuxiliaryDeviceIdentifiers")]
    #[serde(default)]
    pub auxiliary_device_identifiers: Vec<OtdRawIdentifier>,
    #[serde(rename = "Attributes")]
    #[serde(default)]
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtdRawIdentifier {
    #[serde(rename = "VendorID")]
    pub vendor_id: u16,
    #[serde(rename = "ProductID")]
    pub product_id: u16,
    #[serde(rename = "InputReportLength")]
    pub input_report_length: Option<u32>,
    #[serde(rename = "OutputReportLength")]
    pub output_report_length: Option<u32>,
    #[serde(rename = "MaxX")]
    #[serde(default)]
    pub max_x: Option<f64>,
    #[serde(rename = "MaxY")]
    #[serde(default)]
    pub max_y: Option<f64>,
    #[serde(rename = "MaxPressure")]
    #[serde(default)]
    pub max_pressure: Option<u32>,
    #[serde(rename = "Width")]
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(rename = "Height")]
    #[serde(default)]
    pub height: Option<f64>,
}

impl OtdRawConfig {
    /// Parse from an OpenTabletDriver JSON config file.
    pub fn from_json(data: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(data)
    }

    /// Convert into our internal tablet config.
    pub fn into_tablet_config(self) -> Option<OtdTabletConfig> {
        let primary = self.digitizer_identifiers.first()?;

        Some(OtdTabletConfig {
            name: self.name,
            vendor_id: primary.vendor_id,
            product_id: primary.product_id,
            input_report_length: primary.input_report_length,
            output_report_length: primary.output_report_length,
            max_x: primary.max_x.unwrap_or(0.0),
            max_y: primary.max_y.unwrap_or(0.0),
            max_pressure: primary.max_pressure.unwrap_or(0),
            width_mm: primary.width.unwrap_or(0.0),
            height_mm: primary.height.unwrap_or(0.0),
            auxiliary_ids: self
                .auxiliary_device_identifiers
                .into_iter()
                .map(|id| OtdDeviceId {
                    vendor_id: id.vendor_id,
                    product_id: id.product_id,
                    input_report_length: id.input_report_length,
                    output_report_length: id.output_report_length,
                })
                .collect(),
            pen_buttons: 2, // default — most pens have 2 buttons
            aux_buttons: 0,
        })
    }
}
