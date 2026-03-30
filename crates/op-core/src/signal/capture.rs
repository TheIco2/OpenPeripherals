use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::hid::{HidHandle, HidResult};

/// Direction of a captured signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalDirection {
    /// Data sent from host to device.
    HostToDevice,
    /// Data received from device to host.
    DeviceToHost,
    /// A feature report read from the device (stateful snapshot).
    FeatureReport,
    /// Response to a vendor-specific probe command (write-then-read).
    VendorResponse,
}

/// A single captured HID report with timing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedReport {
    /// Milliseconds since capture session started.
    pub timestamp_ms: u64,
    /// The raw bytes of the report.
    pub data: Vec<u8>,
    /// Whether this was sent or received.
    pub direction: SignalDirection,
    /// Optional key for matching this report across captures.
    /// For vendor responses this is "iface{N}_cmd{XX}" so the analyzer
    /// can pair identical queries from different steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_key: Option<String>,
}

/// Diagnostic summary from a capture run.
#[derive(Debug, Clone, Default)]
pub struct CaptureDiagnostics {
    /// How many HID interfaces were passed in.
    pub interfaces_used: usize,
    /// Feature report IDs that were probed.
    pub feature_probes_attempted: usize,
    /// Feature report probes that returned data.
    pub feature_probes_succeeded: usize,
    /// Interrupt report reads that returned data.
    pub interrupt_reads: usize,
    /// Vendor probe commands sent (write-then-read).
    pub vendor_probes_sent: usize,
    /// Vendor probe commands that got a response.
    pub vendor_probes_responded: usize,
}

/// The result from [`SignalCapture::capture_full`].
pub struct CaptureResult {
    pub reports: Vec<CapturedReport>,
    pub diagnostics: CaptureDiagnostics,
}

/// A capture session that records all HID traffic for a device during a time window.
pub struct SignalCapture {
    reports: Vec<CapturedReport>,
    start_time: Option<Instant>,
}

impl SignalCapture {
    pub fn new() -> Self {
        Self {
            reports: Vec::new(),
            start_time: None,
        }
    }

    /// Begin a new capture session, clearing any existing data.
    pub fn start(&mut self) {
        self.reports.clear();
        self.start_time = Some(Instant::now());
    }

    /// Record a report that was read from the device.
    pub fn record_incoming(&mut self, data: &[u8]) {
        if let Some(start) = self.start_time {
            self.reports.push(CapturedReport {
                timestamp_ms: start.elapsed().as_millis() as u64,
                data: data.to_vec(),
                direction: SignalDirection::DeviceToHost,
                match_key: None,
            });
        }
    }

    /// Record a report that was sent to the device.
    pub fn record_outgoing(&mut self, data: &[u8]) {
        if let Some(start) = self.start_time {
            self.reports.push(CapturedReport {
                timestamp_ms: start.elapsed().as_millis() as u64,
                data: data.to_vec(),
                direction: SignalDirection::HostToDevice,
                match_key: None,
            });
        }
    }

    /// Stop the capture session and return all recorded reports.
    pub fn stop(&mut self) -> Vec<CapturedReport> {
        self.start_time = None;
        std::mem::take(&mut self.reports)
    }

    /// Passively capture incoming reports for the given duration.
    pub fn capture_passive(
        handle: &HidHandle,
        duration: Duration,
    ) -> HidResult<Vec<CapturedReport>> {
        let mut capture = Self::new();
        capture.start();

        let deadline = Instant::now() + duration;
        let mut buf = [0u8; 256];

        while Instant::now() < deadline {
            match handle.read(&mut buf, 50) {
                Ok(n) if n > 0 => {
                    capture.record_incoming(&buf[..n]);
                }
                Ok(_) => {} // no data, continue
                Err(e) => {
                    log::warn!("Read error during capture: {e}");
                }
            }
        }

        Ok(capture.stop())
    }

    /// Comprehensive capture across multiple interfaces.
    ///
    /// 1. Probes feature reports (IDs 0–32) from each handle.
    /// 2. Reads interrupt reports from all handles for `duration`.
    /// 3. Merges everything into one report vec.
    pub fn capture_full(
        handles: &[HidHandle],
        duration: Duration,
    ) -> CaptureResult {
        let mut capture = Self::new();
        capture.start();
        let mut feature_count = 0usize;
        let mut feature_probes = 0usize;

        // Phase 1: probe feature reports 0–255 from each handle.
        let mut feat_buf = [0u8; 256];
        for (hi, handle) in handles.iter().enumerate() {
            let mut iface_hits = 0usize;
            for report_id in 0u8..=255 {
                feat_buf[0] = report_id;
                feature_probes += 1;
                match handle.get_feature_report(&mut feat_buf) {
                    Ok(n) if n > 1 => {
                        log::debug!(
                            "Feature report 0x{:02X} from iface {} ({}): {} bytes",
                            report_id,
                            hi,
                            handle.interface(),
                            n,
                        );
                        if let Some(start) = capture.start_time {
                            capture.reports.push(CapturedReport {
                                timestamp_ms: start.elapsed().as_millis() as u64,
                                data: feat_buf[..n].to_vec(),
                                direction: SignalDirection::FeatureReport,
                                match_key: None,
                            });
                        }
                        feature_count += 1;
                        iface_hits += 1;
                    }
                    Ok(n) => {
                        log::trace!(
                            "Feature 0x{:02X} iface {} ({}): only {} byte(s)",
                            report_id, hi, handle.interface(), n,
                        );
                    }
                    Err(e) => {
                        log::trace!(
                            "Feature 0x{:02X} iface {} ({}) failed: {e}",
                            report_id, hi, handle.interface(),
                        );
                    }
                }
            }
            log::info!(
                "Interface {} ({}): {iface_hits} feature reports out of 256 probes",
                hi, handle.interface(),
            );
        }

        // Phase 2: active vendor probing (write-then-read) for vendor interfaces.
        //
        // Vendor-specific interfaces (usage_page >= 0xFF00) use a command/response
        // protocol.  For Corsair V2 devices this means GET (0x02) requests to
        // property addresses.  The diff analysis then spots addresses whose
        // value changes between capture steps.
        let mut vendor_sent = 0usize;
        let mut vendor_responded = 0usize;

        for (_hi, handle) in handles.iter().enumerate() {
            if !handle.is_vendor_interface() {
                continue;
            }
            log::info!(
                "Vendor probing iface {} ({:#06x}/{:#06x}) vid={:#06x}",
                handle.interface(), handle.usage_page(), handle.usage(), handle.vid(),
            );

            // Flush any stale data first.
            let mut flush_buf = [0u8; 256];
            while handle.read(&mut flush_buf, 5).unwrap_or(0) > 0 {}

            let probes = build_vendor_probes(handle.vid(), handle.usage_page(), handle.usage());

            for (label, probe_bytes) in &probes {
                vendor_sent += 1;

                if let Err(e) = handle.write(probe_bytes) {
                    log::trace!(
                        "Vendor probe '{}' iface {} write failed: {e}",
                        label, handle.interface(),
                    );
                    continue;
                }

                // Read response with 50ms timeout (matches Corsair V2 timing).
                let mut resp = [0u8; 256];
                match handle.read(&mut resp, 50) {
                    Ok(n) if n > 0 => {
                        log::debug!(
                            "Vendor probe '{}' iface {}: {} byte response",
                            label, handle.interface(), n,
                        );
                        if let Some(start) = capture.start_time {
                            let key = format!(
                                "iface{}_{label}",
                                handle.interface(),
                            );
                            capture.reports.push(CapturedReport {
                                timestamp_ms: start.elapsed().as_millis() as u64,
                                data: resp[..n].to_vec(),
                                direction: SignalDirection::VendorResponse,
                                match_key: Some(key),
                            });
                        }
                        vendor_responded += 1;
                    }
                    Ok(_) => {
                        log::trace!(
                            "Vendor probe '{}' iface {}: no response",
                            label, handle.interface(),
                        );
                    }
                    Err(e) => {
                        log::trace!(
                            "Vendor probe '{}' iface {} read failed: {e}",
                            label, handle.interface(),
                        );
                    }
                }
            }

            log::info!(
                "Vendor probing iface {}: {vendor_responded} responses to {} probes",
                handle.interface(), vendor_sent,
            );
        }

        // Phase 3: read interrupt reports from all handles for the duration.
        let deadline = Instant::now() + duration;
        let mut buf = [0u8; 256];
        let mut interrupt_count = 0usize;

        while Instant::now() < deadline {
            for handle in handles {
                match handle.read(&mut buf, 10) {
                    Ok(n) if n > 0 => {
                        capture.record_incoming(&buf[..n]);
                        interrupt_count += 1;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::trace!("Read error during capture: {e}");
                    }
                }
            }
        }

        let reports = capture.stop();
        log::info!(
            "capture_full: {} total ({} feature, {} vendor, {} interrupt) from {} iface(s)",
            reports.len(),
            feature_count,
            vendor_responded,
            interrupt_count,
            handles.len(),
        );
        CaptureResult {
            reports,
            diagnostics: CaptureDiagnostics {
                interfaces_used: handles.len(),
                feature_probes_attempted: feature_probes,
                feature_probes_succeeded: feature_count,
                interrupt_reads: interrupt_count,
                vendor_probes_sent: vendor_sent,
                vendor_probes_responded: vendor_responded,
            },
        }
    }

    pub fn reports(&self) -> &[CapturedReport] {
        &self.reports
    }
}

impl Default for SignalCapture {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Vendor-aware probe generation
// ---------------------------------------------------------------------------

/// Build a labelled list of probe packets for a vendor interface.
///
/// For known vendors we use the correct protocol framing so the device
/// returns meaningful state data instead of NACK / error bytes.
fn build_vendor_probes(vid: u16, usage_page: u16, usage: u16) -> Vec<(String, Vec<u8>)> {
    // Corsair V2 protocol (usage_page 0xFF42).
    if vid == 0x1B1C && usage_page == 0xFF42 {
        return build_corsair_v2_probes(usage);
    }

    // Generic fallback: raw command IDs 0x00-0x0F.
    (0x00u8..=0x0Fu8)
        .map(|cmd| {
            let mut pkt = vec![0u8; 65];
            pkt[0] = 0x00;
            pkt[1] = cmd;
            (format!("cmd{cmd:02x}"), pkt)
        })
        .collect()
}

/// Corsair V2 protocol packet format (CorsairPeripheralV2 / BRAGI):
///   byte 0 = 0x00       (HID report ID)
///   byte 1 = write_cmd  (0x08 = wired, 0x09 = wireless)
///   byte 2 = command    (0x02 = GET, 0x01 = SET, 0x0D = START_TX, 0x05 = STOP_TX)
///   byte 3 = address    (property address to read/write)
///   byte 4+ = data / padding (zeroes)
///
/// Response (Windows hidapi, report-ID 0 prepended):
///   byte 0 = 0x00       (report ID)
///   byte 1 = CMD echo   (e.g. 0x02 for GET)
///   byte 2 = status     (0x00 = success, nonzero = error)
///   byte 3+ = data
///
/// Reference: OpenRGB CorsairPeripheralV2Controller
fn build_corsair_v2_probes(usage: u16) -> Vec<(String, Vec<u8>)> {
    const CMD_GET: u8 = 0x02;

    let mut probes = Vec::new();

    // Sweep property addresses with the GET command.
    // Known addresses (from OpenRGB):
    //   0x03 = rendering mode (SW / HW)
    //   0x11 = vendor subsystem ID
    //   0x12 = product ID
    //   0x41 = keyboard layout
    // Many other addresses may hold brightness, effect, colour data, etc.
    // We probe broadly so the diff analysis can detect any state changes.
    let addr_end: u8 = if usage == 0x0002 { 0x30 } else { 0x50 };

    // Wired mode (write_cmd = 0x08) — primary attempt.
    for addr in 0x00..=addr_end {
        let mut pkt = vec![0u8; 65];
        pkt[0] = 0x00;   // HID report ID
        pkt[1] = 0x08;   // write_cmd wired
        pkt[2] = CMD_GET;
        pkt[3] = addr;
        probes.push((format!("get{addr:02x}"), pkt));
    }

    // Wireless mode (write_cmd = 0x09) — fallback for a few key addresses.
    for addr in [0x03, 0x11, 0x12] {
        let mut pkt = vec![0u8; 65];
        pkt[0] = 0x00;
        pkt[1] = 0x09;   // write_cmd wireless
        pkt[2] = CMD_GET;
        pkt[3] = addr;
        probes.push((format!("wget{addr:02x}"), pkt));
    }

    probes
}
