// op-core/src/logging.rs
//
// Async file-based logger
//
// Implements the `log` crate's `Log` trait so every `log::info!()`, `log::warn!()`,
// etc. throughout the codebase is captured and written to:
//
//   ~/ProjectOpen/.Logs/OpenPeripheral/<source>/OpenPeripheral.log
//
// A background writer thread handles I/O so logging never blocks the render loop.

use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
        OnceLock,
    },
    thread,
};

use chrono;
use log::{Level, LevelFilter, Log, Metadata, Record};

/* =========================
   GLOBAL STATE
   ========================= */

/// Whether debug-level messages are enabled (set at init).
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Resolved log file path.
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Source identifier for log directory separation.
static LOG_SOURCE: OnceLock<String> = OnceLock::new();

/// Sender for the background writer thread.
static LOG_TX: OnceLock<Sender<String>> = OnceLock::new();

/// Singleton logger instance (required by `log::set_logger`).
static LOGGER: OpenPeripheralLogger = OpenPeripheralLogger;

/* =========================
   PUBLIC API
   ========================= */

/// Initialise the OpenPeripheral logger.
///
/// - `source`: identifies the binary ("App", "Server", "Dev", etc.).
///   Logs are written to `~/ProjectOpen/.Logs/OpenPeripheral/<source>/OpenPeripheral.log`.
/// - `debug`: if `true`, captures `Debug`-level and above; otherwise `Warn` and above.
///
/// Call once at startup before any `log::` macros. Panics if called more than once.
pub fn init(source: &str, debug: bool) {
    if LOG_TX.get().is_some() {
        panic!("logging::init() called more than once");
    }

    DEBUG_ENABLED.store(debug, Ordering::Relaxed);
    LOG_SOURCE.set(source.to_owned()).expect("LOG_SOURCE already set");

    let path = log_path().clone();
    let (tx, rx) = mpsc::channel::<String>();
    LOG_TX.set(tx).expect("LOG_TX already set");

    // Background writer thread — keeps file I/O off the render thread.
    thread::spawn(move || {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("Failed to open log file");

        while let Ok(line) = rx.recv() {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    });

    // Register as the global `log` backend.
    let max_level = if debug {
        LevelFilter::Debug
    } else {
        LevelFilter::Warn
    };

    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(max_level))
        .expect("Failed to set logger");
}

/// Returns `true` if debug-level logging is active.
#[inline]
pub fn debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

/* =========================
   log::Log IMPLEMENTATION
   ========================= */

struct OpenPeripheralLogger;

impl Log for OpenPeripheralLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if DEBUG_ENABLED.load(Ordering::Relaxed) {
            metadata.level() <= Level::Debug
        } else {
            metadata.level() <= Level::Warn
        }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level = record.level();
        let msg = format!("{}", record.args());

        // Also print to stderr for immediate visibility during development.
        if DEBUG_ENABLED.load(Ordering::Relaxed) {
            eprintln!("[{level}] {msg}");
        }

        enqueue(&level.to_string(), msg);
    }

    fn flush(&self) {
        // The background thread flushes after every write.
    }
}

/* =========================
   INTERNAL
   ========================= */

#[inline]
fn enqueue(level: &str, msg: String) {
    if let Some(tx) = LOG_TX.get() {
        let ts = timestamp();
        let _ = tx.send(format!("{ts} [{level}] {msg}"));
    }
}

fn timestamp() -> String {
    let now = chrono::Local::now();
    now.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/* =========================
   PATH RESOLUTION
   ========================= */

fn log_path() -> &'static PathBuf {
    LOG_PATH.get_or_init(|| {
        let logs_dir = resolve_logs_dir().unwrap_or_else(|| {
            // Fallback: logs/ next to the executable.
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("logs")))
                .unwrap_or_else(|| PathBuf::from("logs"))
        });

        let _ = std::fs::create_dir_all(&logs_dir);
        logs_dir.join("OpenPeripheral.log")
    })
}

/// Resolve `~/ProjectOpen/.Logs/OpenPeripheral/<source>/` using environment variables.
fn resolve_logs_dir() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE").ok().or_else(|| {
        let drive = std::env::var("HOMEDRIVE").ok()?;
        let path = std::env::var("HOMEPATH").ok()?;
        Some(format!("{drive}{path}"))
    })?;

    let source = LOG_SOURCE.get().map(|s| s.as_str()).unwrap_or("App");

    let logs = PathBuf::from(home)
        .join("ProjectOpen")
        .join(".Logs")
        .join("OpenPeripheral")
        .join(source);
    Some(logs)
}
