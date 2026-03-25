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

    pub fn reports(&self) -> &[CapturedReport] {
        &self.reports
    }
}

impl Default for SignalCapture {
    fn default() -> Self {
        Self::new()
    }
}
