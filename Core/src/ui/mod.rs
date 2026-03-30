mod pages;

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use openrender_runtime::gpu::context::GpuContext;
use openrender_runtime::gpu::renderer::Renderer;
use openrender_runtime::scene::app_host::{AppEvent, AppHost, PageSource, Route};
use openrender_runtime::scene::input_handler::{
    KeyCode, Modifiers, MouseButton as CxMouseButton, RawInputEvent,
};
use openrender_runtime::capabilities::{CapabilitySet, NetworkAccess, TrayAccess, SingleInstance};
use openrender_runtime::instance::{self, InstanceLockResult};
use openrender_runtime::tray::TrayConfig;
use op_addon::AddonRegistry;
use op_ai::LearningSession;
use op_core::device::{DeviceRegistry, DeviceType};
use op_core::hid;
use op_core::profile::ProfileStore;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

/// Build the AppHost with all pages and launch the OpenRender renderer.
pub fn launch(
    device_registry: Arc<DeviceRegistry>,
    profile_store: ProfileStore,
    addon_registry: AddonRegistry,
) -> Result<()> {
    // Enforce single-instance: if OpenPeripheral is already running,
    // signal the existing instance to come to foreground and exit.
    let instance_guard = match instance::acquire_single_instance("OpenPeripheral") {
        InstanceLockResult::Acquired(guard) => Some(guard),
        InstanceLockResult::AlreadyRunning => {
            log::info!("Another OpenPeripheral instance is already running — focusing it.");
            return Ok(());
        }
    };

    let mut host = build_app_host();

    // Hand the single-instance guard to the host so it polls for focus
    // requests from future launches.
    if let Some(guard) = instance_guard {
        host.set_instance_guard(guard);
    }

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = OpenPeripheralApp::new(host, device_registry, profile_store, addon_registry);

    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("Event loop error: {e}");
    }

    Ok(())
}

/// Load an icon declared via `<include type="icon">` from the compiled document.
/// `target` should be "window", "system", or "" to match either.
fn load_declared_icon(host: &AppHost, target: &str) -> Option<(Vec<u8>, u32, u32)> {
    for decl in host.icon_declarations() {
        // Match: empty target means both, or target matches exactly.
        if decl.target.is_empty() || decl.target == target {
            let path = std::path::Path::new(&decl.path);
            if path.exists() {
                if let Ok(img) = image::open(path) {
                    let rgba = img.into_rgba8();
                    let w = rgba.width();
                    let h = rgba.height();
                    return Some((rgba.into_raw(), w, h));
                } else {
                    log::warn!("Failed to load icon: {}", decl.path);
                }
            } else {
                log::warn!("Icon file not found: {}", decl.path);
            }
        }
    }
    None
}

fn build_app_host() -> AppHost {
    let mut host = AppHost::new("OpenPeripheral");
    host.sidebar_width = 0.0; // Sidebar is part of the page HTML

    host.set_capabilities(
        CapabilitySet::new()
            .declare(TrayAccess)
            .declare(NetworkAccess)
            .declare(SingleInstance),
    );

    // Single route: base.html is the template with sidebar + page-content.
    // Individual pages (devices, addons, etc.) are loaded as content fragments
    // inside the <page-content> container via data-navigate sidebar clicks.
    host.add_route(Route {
        id: "home".into(),
        label: "Home".into(),
        icon: None,
        source: PageSource::HtmlFile(pages::base_page()),
        separator: false,
    });

    // Load custom title bar from pages/title-bar.html if it exists.
    host.load_title_bar(&pages::base_page().parent().unwrap_or(std::path::Path::new(".")));

    host.navigate_to("home");
    host
}

// ---------------------------------------------------------------------------
// Application state (mirrors OpenRender's own main.rs pattern)
// ---------------------------------------------------------------------------

struct OpenPeripheralApp {
    host: AppHost,
    window: Option<Arc<Window>>,
    gpu_ctx: Option<GpuContext>,
    renderer: Option<Renderer>,
    last_frame: Instant,
    frame_count: u64,
    fps_timer: Instant,
    current_modifiers: winit::keyboard::ModifiersState,
    /// Set to `true` when the app should exit at the next event-loop iteration.
    exit_requested: bool,
    /// Last known cursor position in logical pixels.
    cursor_pos: (f32, f32),
    /// Live subsystems.
    device_registry: Arc<DeviceRegistry>,
    profile_store: ProfileStore,
    addon_registry: AddonRegistry,
    /// Cached HID scan results (deduplicated by VID:PID).
    cached_devices: Vec<hid::HidDeviceEntry>,
    /// When the last automatic scan was performed.
    last_scan_time: Instant,
    /// Whether to show devices with unknown manufacturers.
    show_unknown_devices: bool,
    /// Active AI learning session (if any), with cached device identifiers.
    learning_session: Option<(LearningSession, u16, u16)>, // (session, vid, pid)
    /// AI setup: selected device (vid, pid, name, brand) — set before start-session.
    ai_selected_device: Option<(u16, u16, String, String)>,
    /// AI setup: selected device type string.
    ai_selected_type: Option<String>,
    /// Currently-editing device (vid, pid) for the device_edit page.
    edit_device: Option<(u16, u16)>,
}

impl OpenPeripheralApp {
    fn new(
        host: AppHost,
        device_registry: Arc<DeviceRegistry>,
        profile_store: ProfileStore,
        addon_registry: AddonRegistry,
    ) -> Self {
        Self {
            host,
            window: None,
            gpu_ctx: None,
            renderer: None,
            last_frame: Instant::now(),
            frame_count: 0,
            fps_timer: Instant::now(),
            current_modifiers: winit::keyboard::ModifiersState::empty(),
            exit_requested: false,
            cursor_pos: (0.0, 0.0),
            device_registry,
            profile_store,
            addon_registry,
            cached_devices: Vec::new(),
            last_scan_time: Instant::now(),
            show_unknown_devices: false,
            learning_session: None,
            ai_selected_device: None,
            ai_selected_type: None,
            edit_device: None,
        }
    }

    fn dispatch_input(&mut self, raw: RawInputEvent) {
        let (vw, vh) = self.viewport_size();
        self.host.handle_input(raw, vw, vh);
    }

    fn viewport_size(&self) -> (f32, f32) {
        let ctx = match self.gpu_ctx.as_ref() {
            Some(c) => c,
            None => return (1280.0, 800.0),
        };
        let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
        (ctx.size.0 as f32 / scale, ctx.size.1 as f32 / scale)
    }

    // -----------------------------------------------------------------------
    // IPC dispatch
    // -----------------------------------------------------------------------

    fn handle_ipc(&mut self, ns: &str, cmd: &str, args: Option<serde_json::Value>) {
        match (ns, cmd) {
            // ── Devices ────────────────────────────────────────────────
            ("devices", "scan") => self.ipc_devices_scan(),
            ("devices", "toggle-unknown") => self.ipc_devices_toggle_unknown(),
            ("devices", "edit") => self.ipc_devices_edit(args),

            // ── Device Control ─────────────────────────────────────────
            ("device", "set-property") => self.ipc_device_set_property(args),

            // ── AI Learning ────────────────────────────────────────────
            ("ai", "select-device") => self.ipc_ai_select_device(args),
            ("ai", "select-type") => self.ipc_ai_select_type(args),
            ("ai", "start-session") => self.ipc_ai_start_session(args),
            ("ai", "continue") => self.ipc_ai_continue(),
            ("ai", "skip") => self.ipc_ai_skip(),
            ("ai", "cancel") => self.ipc_ai_cancel(),
            ("ai", "toggle-advanced") => self.ipc_ai_toggle_advanced(),
            ("ai", "answer-yes") => self.ipc_ai_answer(true),
            ("ai", "answer-no") => self.ipc_ai_answer(false),
            ("ai", "verify-yes") => self.ipc_ai_verify(true),
            ("ai", "verify-no") => self.ipc_ai_verify(false),

            // ── Addons ─────────────────────────────────────────────────
            ("addons", "refresh") => self.ipc_addons_refresh(),
            ("addons", "open-folder") => {
                let dir = self.addon_registry.addons_dir().to_path_buf();
                if dir.exists() {
                    #[cfg(target_os = "windows")]
                    { let _ = std::process::Command::new("explorer").arg(&dir).spawn(); }
                } else {
                    self.host.execute_js(
                        "if(typeof showToast==='function')showToast('Addons folder not found','warning');",
                    );
                }
            }

            // ── Profiles ───────────────────────────────────────────────
            ("profiles", "refresh") => self.ipc_profiles_refresh(),
            ("profiles", "delete") => self.ipc_profiles_delete(args),
            ("profiles", "open-folder") => {
                let dir = self.profile_store.base_dir().to_path_buf();
                if dir.exists() {
                    #[cfg(target_os = "windows")]
                    { let _ = std::process::Command::new("explorer").arg(&dir).spawn(); }
                } else {
                    self.host.execute_js(
                        "if(typeof showToast==='function')showToast('Profiles folder not found','warning');",
                    );
                }
            }

            // ── App ────────────────────────────────────────────────────
            ("app", "exit") => {
                self.exit_requested = true;
            }

            _ => {
                log::debug!("Unhandled IPC: {ns}/{cmd}");
            }
        }
    }

    // -----------------------------------------------------------------------
    // Devices: scan + populate
    // -----------------------------------------------------------------------

    /// Perform a fresh HID enumeration and cache the results, then update the UI.
    fn ipc_devices_scan(&mut self) {
        self.ipc_devices_scan_inner(true);
    }

    /// Silent scan (no toast) — used for auto-refresh.
    fn devices_auto_scan(&mut self) {
        self.ipc_devices_scan_inner(false);
    }

    fn ipc_devices_scan_inner(&mut self, show_toast: bool) {
        log::info!("IPC: devices/scan — enumerating HID devices");

        // 1. Enumerate raw HID devices on the system.
        let hid_entries = match hid::enumerate_hid_devices() {
            Ok(entries) => entries,
            Err(e) => {
                log::error!("HID enumeration failed: {e}");
                if show_toast {
                    self.host.execute_js(&format!(
                        "if(typeof showToast==='function')showToast('Scan failed: {}','error');",
                        Self::escape_js(&e.to_string()),
                    ));
                }
                return;
            }
        };

        // 2. De-duplicate HID entries by (vid, pid) — keep first occurrence.
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<_> = hid_entries
            .into_iter()
            .filter(|e| seen.insert((e.vendor_id, e.product_id)))
            .collect();

        self.cached_devices = unique;
        self.last_scan_time = Instant::now();

        // 3. Update UI from cache.
        self.populate_devices_ui(show_toast);
        self.populate_ai_device_selector();
    }

    /// Rebuild the devices page DOM from cached_devices (respecting the unknown filter).
    fn populate_devices_ui(&mut self, show_toast: bool) {
        let registered = self.device_registry.list();
        let show_unknown = self.show_unknown_devices;

        let visible: Vec<_> = self
            .cached_devices
            .iter()
            .filter(|e| show_unknown || !e.manufacturer.is_empty())
            .collect();

        let mut cards = String::new();
        for entry in &visible {
            let name = if entry.product_name.is_empty() {
                format!("Unknown Device ({:04X}:{:04X})", entry.vendor_id, entry.product_id)
            } else {
                entry.product_name.clone()
            };
            let manufacturer = if entry.manufacturer.is_empty() {
                "Unknown".to_string()
            } else {
                entry.manufacturer.clone()
            };

            let has_driver = registered.iter().any(|d| {
                d.vendor_id == entry.vendor_id && d.product_id == entry.product_id
            });

            let has_profile = self.profile_store.find_by_vid_pid(
                entry.vendor_id, entry.product_id,
            ).is_some();

            let status_class = if has_driver { "device-status" } else { "device-status offline" };
            let icon_char = name.chars().next().unwrap_or('D');

            if has_profile {
                let args_json = format!(
                    r#"{{"vid":{},"pid":{}}}"#,
                    entry.vendor_id, entry.product_id,
                );
                cards.push_str(&format!(
                    concat!(
                        "<div class=\"device-card device-card-clickable\" data-vid=\"{vid}\" data-pid=\"{pid}\" ",
                          "data-action=\"ipc\" data-ns=\"devices\" data-cmd=\"edit\" ",
                          "data-args='{args}'>",
                          "<div class=\"device-icon\">{icon}</div>",
                          "<div class=\"device-info\">",
                            "<span class=\"device-name\">{name}</span>",
                            "<span class=\"device-brand\">{mfg} · {vid:04X}:{pid:04X}</span>",
                          "</div>",
                          "<span class=\"tag tag-accent\">Profile</span>",
                          "<div class=\"{sc}\"></div>",
                        "</div>",
                    ),
                    vid = entry.vendor_id,
                    pid = entry.product_id,
                    args = args_json,
                    icon = Self::escape_html(&icon_char.to_string()),
                    name = Self::escape_html(&name),
                    mfg = Self::escape_html(&manufacturer),
                    sc = status_class,
                ));
            } else {
                cards.push_str(&format!(
                    concat!(
                        "<div class=\"device-card\" data-vid=\"{vid}\" data-pid=\"{pid}\">",
                          "<div class=\"device-icon\">{icon}</div>",
                          "<div class=\"device-info\">",
                            "<span class=\"device-name\">{name}</span>",
                            "<span class=\"device-brand\">{mfg} · {vid:04X}:{pid:04X}</span>",
                          "</div>",
                          "<div class=\"{sc}\"></div>",
                        "</div>",
                    ),
                    vid = entry.vendor_id,
                    pid = entry.product_id,
                    icon = Self::escape_html(&icon_char.to_string()),
                    name = Self::escape_html(&name),
                    mfg = Self::escape_html(&manufacturer),
                    sc = status_class,
                ));
            }
        }

        let hidden_count = self.cached_devices.len() - visible.len();
        let device_count = visible.len();
        let addon_count = self.addon_registry.count();
        let profile_count = self.profile_store.list().len();

        let toggle_label = if show_unknown { "Hide Unknown" } else { "Show Unknown" };
        let toggle_class = if show_unknown { "btn-secondary active" } else { "btn-secondary" };
        let hidden_note = if !show_unknown && hidden_count > 0 {
            format!(" ({hidden_count} hidden)")
        } else {
            String::new()
        };

        let toast = if show_toast {
            format!(
                "if(typeof showToast==='function')showToast('Found {} device(s){}','success');",
                device_count,
                Self::escape_js(&hidden_note),
            )
        } else {
            String::new()
        };

        let js = format!(
            concat!(
                "var dl=document.getElementById('device-list');",
                "if(dl)dl.innerHTML='{cards}';",
                "var dc=document.getElementById('device-count');if(dc)dc.textContent='{dev_n}';",
                "var ac=document.getElementById('addon-count');if(ac)ac.textContent='{addon_n}';",
                "var pc=document.getElementById('profile-count');if(pc)pc.textContent='{prof_n}';",
                "var tb=document.getElementById('btn-toggle-unknown');",
                "if(tb){{tb.textContent='{toggle_label}';tb.className='{toggle_class}';}}",
                "{toast}",
            ),
            cards = Self::escape_js(&cards),
            dev_n = device_count,
            addon_n = addon_count,
            prof_n = profile_count,
            toggle_label = toggle_label,
            toggle_class = toggle_class,
            toast = toast,
        );

        self.host.execute_js(&js);
        log::info!("Device list updated: {} visible, {} hidden", device_count, hidden_count);
    }

    /// Rebuild the AI learning device selector from cached_devices.
    fn populate_ai_device_selector(&mut self) {
        let refs: Vec<_> = self.cached_devices.iter().collect();
        let selector_html = Self::build_device_selector_html(&refs);
        let js = format!(
            "var sel=document.getElementById('ai-device-selector');if(sel)sel.innerHTML='{}';",
            Self::escape_js(&selector_html),
        );
        self.host.execute_js(&js);
    }

    /// Toggle visibility of devices from unknown manufacturers.
    fn ipc_devices_toggle_unknown(&mut self) {
        self.show_unknown_devices = !self.show_unknown_devices;
        log::info!("Toggle unknown devices: {}", self.show_unknown_devices);
        self.populate_devices_ui(false);
    }

    // -----------------------------------------------------------------------
    // Device Edit / Control
    // -----------------------------------------------------------------------

    /// Navigate to the device edit page for a specific VID:PID.
    fn ipc_devices_edit(&mut self, args: Option<serde_json::Value>) {
        let (vid, pid) = match args.as_ref().and_then(|a| {
            Some((a.get("vid")?.as_u64()? as u16, a.get("pid")?.as_u64()? as u16))
        }) {
            Some(v) => v,
            None => {
                log::warn!("devices/edit: missing vid/pid args");
                return;
            }
        };
        log::info!("devices/edit: navigating to device_edit for {:04X}:{:04X}", vid, pid);
        self.edit_device = Some((vid, pid));
        // device_edit is a content fragment inside <page-content>, NOT a
        // top-level route.  Use request_content_swap() so the active page
        // ("home") stays loaded and only the fragment is swapped in.
        self.host.request_content_swap("device_edit");
        // populate_device_edit_ui will run on the next tick when the
        // ContentSwap event is processed and the fragment DOM is ready.
    }

    /// Send a SET command for a signal property to the device.
    fn ipc_device_set_property(&mut self, args: Option<serde_json::Value>) {
        let (vid, pid) = match self.edit_device {
            Some(v) => v,
            None => {
                log::warn!("device/set-property: no device selected");
                return;
            }
        };

        let (signal_name, value) = match args.as_ref().and_then(|a| {
            Some((
                a.get("signal")?.as_str()?.to_string(),
                a.get("value")?.as_u64()?,
            ))
        }) {
            Some(v) => v,
            None => {
                log::warn!("device/set-property: missing signal/value args");
                return;
            }
        };

        // Look up the profile and signal pattern.
        let profile = match self.profile_store.find_by_vid_pid(vid, pid) {
            Some(p) => p,
            None => {
                log::warn!("device/set-property: no profile for {:04X}:{:04X}", vid, pid);
                return;
            }
        };

        let pattern = match profile.signals.get(&signal_name) {
            Some(p) => p,
            None => {
                log::warn!("device/set-property: signal '{}' not found in profile", signal_name);
                return;
            }
        };

        // Build a SET command from the pattern's command_bytes (which is a GET).
        // Corsair V2: byte[2] = GET(0x02) → SET(0x01), byte[3] = address,
        // param bytes at offsets specified in parameters.
        let mut cmd = pattern.command_bytes.clone();
        if cmd.len() >= 3 && cmd[1] == 0x08 || cmd.len() >= 3 && cmd[1] == 0x09 {
            // Convert GET → SET.
            cmd[2] = 0x01;
        }

        // Write the value into all parameter offsets (little-endian).
        let val_bytes = (value as u16).to_le_bytes();
        for (i, param) in pattern.parameters.iter().enumerate() {
            if param.offset < cmd.len() && i < val_bytes.len() {
                cmd[param.offset] = val_bytes[i];
            }
        }

        log::info!(
            "device/set-property: {} = {} → writing {} bytes to {:04X}:{:04X}",
            signal_name, value, cmd.len(), vid, pid,
        );

        // Write to the device's software vendor interface.
        let handles = hid::HidHandle::open_all_interfaces(vid, pid);
        let mut sent = false;
        for h in &handles {
            if h.is_vendor_interface() && h.usage() == 0x0001 {
                match h.write(&cmd) {
                    Ok(_) => {
                        // Read the response.
                        let mut resp = [0u8; 256];
                        match h.read(&mut resp, 50) {
                            Ok(n) if n > 0 => {
                                let hex: String = resp[..n.min(16)]
                                    .iter()
                                    .map(|b| format!("{:02X}", b))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                log::info!("SET response ({n}B): {hex}");
                            }
                            _ => {}
                        }
                        sent = true;
                        break;
                    }
                    Err(e) => log::warn!("SET write failed: {e}"),
                }
            }
        }

        if sent {
            self.host.execute_js(&format!(
                "if(typeof showToast==='function')showToast('{} set to {}','success');",
                Self::escape_js(&signal_name), value,
            ));
        } else {
            self.host.execute_js(
                "if(typeof showToast==='function')showToast('Failed to send command','error');",
            );
        }
    }

    /// Populate the device edit page with controls for all confirmed signals.
    fn populate_device_edit_ui(&mut self) {
        let (vid, pid) = match self.edit_device {
            Some(v) => v,
            None => return,
        };

        let profile = match self.profile_store.find_by_vid_pid(vid, pid) {
            Some(p) => p.clone(),
            None => {
                self.host.execute_js(
                    "if(typeof showToast==='function')showToast('No profile found','warning');",
                );
                return;
            }
        };

        // --- Header ---
        let name_js = format!(
            concat!(
                "var dn=document.getElementById('de-device-name');if(dn)dn.textContent='{}';",
                "var di=document.getElementById('de-device-info');if(di)di.textContent='{}';",
            ),
            Self::escape_js(&profile.device_name),
            Self::escape_js(&format!(
                "{} · {:04X}:{:04X} · {} signal(s)",
                profile.brand, vid, pid, profile.signals.len(),
            )),
        );

        // --- Build control cards for each signal ---
        let mut controls_html = String::from("<div class=\"control-panel\">");

        if profile.signals.is_empty() {
            controls_html.push_str(concat!(
                "<div class=\"empty-state\">",
                    "<div class=\"empty-state-icon\">C</div>",
                    "<p>No confirmed controls</p>",
                    "<p class=\"hint\">Use AI Learning to discover device controls.</p>",
                "</div>",
            ));
        } else {
            let mut sorted_signals: Vec<_> = profile.signals.iter().collect();
            sorted_signals.sort_by_key(|(k, _)| (*k).clone());

            for (sig_name, pattern) in &sorted_signals {
                let display_name = Self::signal_display_name(sig_name);
                let confidence_pct = (pattern.confidence * 100.0) as u32;

                // Determine value range from parameter type.
                let (min_val, max_val) = Self::signal_range(pattern);

                // Build preset buttons across the range.
                let presets = Self::build_preset_buttons(sig_name, min_val, max_val);

                controls_html.push_str(&format!(
                    concat!(
                        "<div class=\"control-card\">",
                          "<div class=\"control-card-header\">",
                            "<h3>{display}</h3>",
                            "<span class=\"control-confidence\">{conf}% confidence</span>",
                          "</div>",
                          "<p class=\"setting-description\">{desc}</p>",
                          "<div class=\"control-presets\">",
                            "{presets}",
                          "</div>",
                        "</div>",
                    ),
                    display = Self::escape_html(&display_name),
                    conf = confidence_pct,
                    desc = Self::escape_html(&pattern.description),
                    presets = presets,
                ));
            }
        }

        controls_html.push_str("</div>");

        let controls_js = format!(
            "var dc=document.getElementById('de-controls');if(dc)dc.innerHTML='{}';",
            Self::escape_js(&controls_html),
        );

        let full_js = format!("{name_js}{controls_js}");
        self.host.execute_js(&full_js);

        log::info!(
            "Device edit page populated for {} ({:04X}:{:04X}) with {} controls",
            profile.device_name, vid, pid, profile.signals.len(),
        );
    }

    /// Human-readable display name for a signal key.
    fn signal_display_name(key: &str) -> String {
        match key {
            "get_brightness" => "Brightness".to_string(),
            "get_render_mode" => "Render Mode".to_string(),
            _ => {
                // Convert snake_case "get_foo_bar" → "Foo Bar"
                key.strip_prefix("get_")
                    .unwrap_or(key)
                    .split('_')
                    .map(|w| {
                        let mut c = w.chars();
                        match c.next() {
                            Some(f) => f.to_uppercase().to_string() + c.as_str(),
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }

    /// Determine the (min, max) range for a signal based on its parameter types.
    fn signal_range(pattern: &op_core::signal::SignalPattern) -> (u64, u64) {
        use op_core::signal::ParameterType;
        // If any parameter has a UInt type with explicit range, use it.
        for param in &pattern.parameters {
            if let ParameterType::UInt { min, max, .. } = &param.param_type {
                return (*min, *max);
            }
        }
        // Default: based on total parameter byte count.
        let total_bytes: usize = pattern.parameters.iter().map(|p| p.length).sum();
        match total_bytes {
            0 => (0, 255),
            1 => (0, 255),
            2 => (0, 1000), // Corsair brightness is 0-1000
            _ => (0, 65535),
        }
    }

    /// Build a row of preset value buttons for a signal control.
    fn build_preset_buttons(sig_name: &str, min: u64, max: u64) -> String {
        // Generate sensible preset values.
        let presets: Vec<(String, u64)> = if sig_name.contains("brightness") {
            vec![
                ("Off".into(), 0),
                ("25%".into(), max / 4),
                ("50%".into(), max / 2),
                ("75%".into(), max * 3 / 4),
                ("100%".into(), max),
            ]
        } else if sig_name.contains("render_mode") {
            vec![
                ("Hardware".into(), 1),
                ("Software".into(), 2),
            ]
        } else {
            // Generic: min, 25%, 50%, 75%, max
            vec![
                (format!("{}", min), min),
                ("25%".into(), min + (max - min) / 4),
                ("50%".into(), min + (max - min) / 2),
                ("75%".into(), min + (max - min) * 3 / 4),
                (format!("{}", max), max),
            ]
        };

        let mut html = String::new();
        for (label, value) in &presets {
            let args = format!(
                r#"{{"signal":"{}","value":{}}}"#,
                Self::escape_html(sig_name),
                value,
            );
            html.push_str(&format!(
                concat!(
                    "<button class=\"btn-secondary btn-sm\" ",
                      "data-action=\"ipc\" data-ns=\"device\" data-cmd=\"set-property\" ",
                      "data-args='{args}'>",
                      "{label}",
                    "</button>",
                ),
                args = args,
                label = Self::escape_html(label),
            ));
        }
        html
    }

    fn build_device_selector_html(devices: &[&hid::HidDeviceEntry]) -> String {
        if devices.is_empty() {
            return concat!(
                "<div class=\"empty-state\">",
                  "<div class=\"empty-state-icon\">U</div>",
                  "<p>No USB devices found</p>",
                  "<p class=\"hint\">Connect a peripheral and click Scan on the Devices page</p>",
                "</div>",
            ).to_string();
        }
        let mut html = String::new();
        for d in devices {
            let name = if d.product_name.is_empty() {
                format!("Device {:04X}:{:04X}", d.vendor_id, d.product_id)
            } else {
                d.product_name.clone()
            };
            let mfg = if d.manufacturer.is_empty() {
                "Unknown".to_string()
            } else {
                d.manufacturer.clone()
            };
            // Build JSON args for the IPC command. Since we use single-quoted
            // HTML attributes, double quotes are fine inside.
            let args_json = format!(
                r#"{{"vid":{},"pid":{},"name":"{}","brand":"{}"}}"#,
                d.vendor_id,
                d.product_id,
                Self::escape_js(&name),
                Self::escape_js(&mfg),
            );
            html.push_str(&format!(
                concat!(
                    "<button class=\"device-option\" ",
                      "data-vid=\"{vid}\" data-pid=\"{pid}\" ",
                      "data-action=\"ipc\" data-ns=\"ai\" data-cmd=\"select-device\" ",
                      "data-args='{args}'>",
                      "<span class=\"device-option-name\">{name}</span>",
                      "<span class=\"device-option-id\">{mfg} — {vid:04X}:{pid:04X}</span>",
                    "</button>",
                ),
                vid = d.vendor_id,
                pid = d.product_id,
                name = Self::escape_html(&name),
                mfg = Self::escape_html(&mfg),
                args = args_json,
            ));
        }
        html
    }

    // -----------------------------------------------------------------------
    // AI Learning session
    // -----------------------------------------------------------------------

    fn ipc_ai_select_device(&mut self, args: Option<serde_json::Value>) {
        if let Some(a) = args {
            let vid = a.get("vid").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let pid = a.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
            let brand = a.get("brand").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
            self.ai_selected_device = Some((vid, pid, name, brand));
            log::info!("AI: device selected — {vid:04X}:{pid:04X}");
            // Highlight the selected button in JS.
            self.host.execute_js(&format!(
                concat!(
                    "var btns=document.querySelectorAll('.device-option');",
                    "for(var i=0;i<btns.length;i++)btns[i].classList.remove('selected');",
                    "var sel=document.querySelector('.device-option[data-vid=\"{vid}\"][data-pid=\"{pid}\"]');",
                    "if(sel)sel.classList.add('selected');",
                ),
                vid = vid,
                pid = pid,
            ));
        }
    }

    fn ipc_ai_select_type(&mut self, args: Option<serde_json::Value>) {
        if let Some(a) = args {
            let dtype = a.get("type").and_then(|v| v.as_str()).unwrap_or("other").to_string();
            self.ai_selected_type = Some(dtype.clone());
            log::info!("AI: type selected — {dtype}");
            // Highlight the selected button in JS.
            self.host.execute_js(&format!(
                concat!(
                    "var btns=document.querySelectorAll('.type-btn');",
                    "for(var i=0;i<btns.length;i++)btns[i].classList.remove('selected');",
                    "var sel=document.querySelector('.type-btn[data-type=\"{t}\"]');",
                    "if(sel)sel.classList.add('selected');",
                ),
                t = Self::escape_js(&dtype),
            ));
        }
    }

    fn ipc_ai_start_session(&mut self, args: Option<serde_json::Value>) {
        // Gather device info from args or from previously stored selection.
        let (vid, pid, name, brand, dtype_str) = if let Some(ref a) = args {
            let vid = a.get("vid").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let pid = a.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
            let brand = a.get("brand").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
            let dt = a.get("device_type").and_then(|v| v.as_str()).unwrap_or("other").to_string();
            (vid, pid, name, brand, dt)
        } else if let (Some((vid, pid, name, brand)), Some(dtype)) =
            (self.ai_selected_device.clone(), self.ai_selected_type.clone())
        {
            (vid, pid, name, brand, dtype)
        } else {
            self.host.execute_js(
                "if(typeof showToast==='function')showToast('Select a device and type first','warning');",
            );
            return;
        };

        if vid == 0 || pid == 0 {
            self.host.execute_js(
                "if(typeof showToast==='function')showToast('Please select a device first','warning');",
            );
            return;
        }

        let device_type = match dtype_str.as_str() {
            "keyboard" => DeviceType::Keyboard,
            "mouse" => DeviceType::Mouse,
            "headset" => DeviceType::Headset,
            "mousepad" => DeviceType::MousePad,
            "light" => DeviceType::SmartLight,
            "tablet" => DeviceType::Tablet,
            other => DeviceType::Other(other.to_string()),
        };

        log::info!("IPC: ai/start-session — {name} ({vid:04X}:{pid:04X}) type={dtype_str}");

        let mut session = LearningSession::new(device_type, name, brand, vid, pid);
        let update = session.start();
        self.learning_session = Some((session, vid, pid));

        self.push_session_update(&update, true);
    }

    fn ipc_ai_continue(&mut self) {
        let (session, vid, pid) = match self.learning_session.as_mut() {
            Some((s, v, p)) => (s, *v, *p),
            None => {
                self.host.execute_js(
                    "if(typeof showToast==='function')showToast('No active session','warning');",
                );
                return;
            }
        };

        // Enumerate interfaces first for diagnostics.
        let all_interfaces = hid::find_device_interfaces(vid, pid).unwrap_or_default();
        let iface_count = all_interfaces.len();

        // Build a description string for the Advanced panel.
        let iface_details: Vec<String> = all_interfaces
            .iter()
            .map(|e| {
                format!(
                    "iface {} — usage {:#06x}/{:#06x}",
                    e.interface_number, e.usage_page, e.usage,
                )
            })
            .collect();
        let iface_detail_str = if iface_details.is_empty() {
            "none found".to_string()
        } else {
            iface_details.join("\\n")
        };

        log::info!(
            "Device {vid:04X}:{pid:04X} — found {} interface(s): {:?}",
            iface_count,
            iface_details,
        );

        // Log interface enumeration into the session for the YAML profile.
        session.log(format!(
            "enumerate: {vid:04X}:{pid:04X} found {iface_count} interface(s)",
        ));
        for detail in &iface_details {
            session.log(format!("  {detail}"));
        }

        // Open ALL HID interfaces for comprehensive capture.
        let handles = hid::HidHandle::open_all_interfaces(vid, pid);
        let opened_count = handles.len();

        session.log(format!(
            "open: {opened_count} of {iface_count} interface(s) opened successfully",
        ));

        // Push interface enumeration info to the Advanced panel immediately.
        self.host.execute_js(&format!(
            concat!(
                "var af=document.getElementById('adv-ifaces-found');if(af)af.textContent='{found}';",
                "var ao=document.getElementById('adv-ifaces-opened');if(ao)ao.textContent='{opened}';",
                "var ad=document.getElementById('adv-iface-details');if(ad)ad.textContent='{details}';",
            ),
            found = iface_count,
            opened = opened_count,
            details = Self::escape_js(&iface_detail_str),
        ));

        if handles.is_empty() {
            log::error!("No HID interfaces could be opened for {vid:04X}:{pid:04X}");
            session.log("ERROR: no interfaces could be opened — step skipped");
            self.host.execute_js(
                "if(typeof showToast==='function')showToast('Could not open any device interface — skipping step','warning');",
            );
            let update = session.skip_step();
            self.push_session_update(&update, false);
            return;
        }

        self.host.execute_js(&format!(
            "if(typeof showToast==='function')showToast('Capturing across {} of {} interface(s)…','info');",
            opened_count, iface_count,
        ));
        let update = session.user_ready_multi(&handles);
        self.push_session_update(&update, false);
    }

    fn ipc_ai_skip(&mut self) {
        let (session, _, _) = match self.learning_session.as_mut() {
            Some(s) => s,
            None => return,
        };
        let update = session.skip_step();
        self.push_session_update(&update, false);
    }

    fn ipc_ai_cancel(&mut self) {
        self.learning_session = None;
        self.host.execute_js(concat!(
            "var ov=document.getElementById('session-overlay');if(ov)ov.style.display='none';",
            "var ss=document.getElementById('setup-sections');if(ss)ss.style.display='';",
            "if(typeof showToast==='function')showToast('Session cancelled','info');",
        ));
    }

    fn ipc_ai_toggle_advanced(&mut self) {
        self.host.execute_js(concat!(
            "var p=document.getElementById('advanced-panel');",
            "var b=document.getElementById('btn-advanced-toggle');",
            "if(p&&b){if(p.style.display==='none'){p.style.display='';b.textContent='Hide Advanced';}",
            "else{p.style.display='none';b.textContent='Show Advanced';}}",
        ));
    }

    fn ipc_ai_answer(&mut self, yes: bool) {
        let (session, _, _) = match self.learning_session.as_mut() {
            Some(s) => s,
            None => return,
        };
        let update = session.answer_question(yes);
        self.push_session_update(&update, false);
    }

    fn ipc_ai_verify(&mut self, confirmed: bool) {
        let (vid, pid) = match self.learning_session.as_ref() {
            Some((_, v, p)) => (*v, *p),
            None => return,
        };

        let session = &mut self.learning_session.as_mut().unwrap().0;
        let update = session.verify_result(confirmed);

        if update.state == op_ai::SessionState::Complete {
            // Verification done — build and save the profile.
            let profile = session.build_profile();
            log::info!(
                "AI Learning complete — verified profile '{}' with {} signal(s)",
                profile.id,
                profile.signals.len(),
            );

            if let Err(e) = self.profile_store.save_yaml(&profile) {
                log::error!("Failed to save profile: {e}");
                self.host.execute_js(&format!(
                    "if(typeof showToast==='function')showToast('Failed to save profile: {}','error');",
                    Self::escape_js(&e.to_string()),
                ));
            } else {
                self.host.execute_js(&format!(
                    concat!(
                        "var ov=document.getElementById('session-overlay');if(ov)ov.style.display='none';",
                        "var ss=document.getElementById('setup-sections');if(ss)ss.style.display='';",
                        "if(typeof showToast==='function')showToast('Profile saved: {}','success');",
                    ),
                    Self::escape_js(&profile.device_name),
                ));
            }
            self.learning_session = None;
            return;
        }

        // Send test command for the next pattern being verified.
        if let Some(pattern) = session.current_verification_pattern() {
            let (cmd_bytes, desc) = op_ai::test_command_for_pattern(pattern);
            // Try to write the test command to the device's vendor interface.
            let handles = hid::HidHandle::open_all_interfaces(vid, pid);
            for h in &handles {
                if h.is_vendor_interface() && h.usage() == 0x0001 {
                    match h.write(&cmd_bytes) {
                        Err(e) => log::warn!("Verify write failed: {e}"),
                        Ok(_) => {
                            // Read the SET response so the device processes it.
                            let mut resp = [0u8; 256];
                            match h.read(&mut resp, 50) {
                                Ok(n) if n > 0 => {
                                    let hex: String = resp[..n.min(16)].iter()
                                        .map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
                                    log::info!("Verify: SET response ({n}B): {hex}");
                                }
                                _ => {}
                            }
                            log::info!("Verify: sent test command — {desc}");
                            break;
                        }
                    }
                }
            }
        }

        self.push_session_update(&update, false);
    }

    fn push_session_update(&mut self, update: &op_ai::SessionUpdate, show_overlay: bool) {
        // Panel visibility: only one of the three panels is shown at a time.
        let (show_q, show_l, show_v) = match &update.state {
            op_ai::SessionState::AskingQuestions { .. } => (true, false, false),
            op_ai::SessionState::Verifying { .. } => (false, false, true),
            _ => (false, true, false),
        };

        let panel_js = format!(
            concat!(
                "var qp=document.getElementById('questionnaire-panel');if(qp)qp.style.display='{}';",
                "var lp=document.getElementById('learning-panel');if(lp)lp.style.display='{}';",
                "var vp=document.getElementById('verification-panel');if(vp)vp.style.display='{}';",
            ),
            if show_q { "" } else { "none" },
            if show_l { "" } else { "none" },
            if show_v { "" } else { "none" },
        );

        // Show/hide the overlay itself.
        let overlay_js = if show_overlay {
            concat!(
                "var ss=document.getElementById('setup-sections');if(ss)ss.style.display='none';",
                "var ov=document.getElementById('session-overlay');if(ov)ov.style.display='';",
            )
        } else {
            ""
        };

        // Questionnaire state updates.
        let q_js = if show_q {
            let q_text = update.current_question.as_ref()
                .map(|q| q.question.as_str())
                .unwrap_or("Loading...");
            let qi = update.question_index + 1;
            let qt = update.total_questions;
            let q_pct = if qt > 0 { (qi as f64 / qt as f64 * 100.0) as u32 } else { 0 };
            format!(
                concat!(
                    "var qt=document.getElementById('question-text');if(qt)qt.textContent='{}';",
                    "var qc=document.getElementById('q-current');if(qc)qc.textContent='{}';",
                    "var qtl=document.getElementById('q-total');if(qtl)qtl.textContent='{}';",
                    "var qpf=document.getElementById('q-progress-fill');if(qpf)qpf.style.width='{}%';",
                ),
                Self::escape_js(q_text), qi, qt, q_pct,
            )
        } else {
            String::new()
        };

        // Verification state updates.
        let v_js = if show_v {
            let v_name = update.verify_pattern_name.as_deref().unwrap_or("—");
            let v_desc = update.verify_description.as_deref().unwrap_or("Did you notice any change?");
            let vi = update.verify_index + 1;
            let vt = update.verify_total;
            let v_pct = if vt > 0 { (vi as f64 / vt as f64 * 100.0) as u32 } else { 0 };
            format!(
                concat!(
                    "var vpn=document.getElementById('verify-pattern-name');if(vpn)vpn.textContent='{}';",
                    "var vd=document.getElementById('verify-description');if(vd)vd.textContent='{}';",
                    "var vc=document.getElementById('v-current');if(vc)vc.textContent='{}';",
                    "var vtl=document.getElementById('v-total');if(vtl)vtl.textContent='{}';",
                    "var vpf=document.getElementById('v-progress-fill');if(vpf)vpf.style.width='{}%';",
                ),
                Self::escape_js(v_name), Self::escape_js(v_desc), vi, vt, v_pct,
            )
        } else {
            String::new()
        };

        // Learning panel updates (step counter, instruction, badge, etc.).
        let instruction = update
            .current_step
            .as_ref()
            .map(|s| s.instruction.clone())
            .unwrap_or_else(|| update.message.clone());

        let (state_text, state_class) = match &update.state {
            op_ai::SessionState::AskingQuestions { .. } => ("Quick Setup", ""),
            op_ai::SessionState::WaitingForUser { .. } => ("Waiting for you", ""),
            op_ai::SessionState::Capturing { .. } => ("Capturing signals", "capturing"),
            op_ai::SessionState::Analyzing => ("Analyzing", "analyzing"),
            op_ai::SessionState::Verifying { .. } => ("Testing", "capturing"),
            op_ai::SessionState::Complete => ("Complete", ""),
            _ => ("Preparing", ""),
        };

        let (category, capture_mode, step_id, capture_dur, capture_during) =
            if let Some(ref step) = update.current_step {
                let cat = format!("{:?}", step.category);
                let mode = if step.capture_during_action {
                    "Live (during action)"
                } else {
                    "Snapshot (after action)"
                };
                (
                    cat,
                    mode.to_string(),
                    step.id.clone(),
                    format!("{}ms", step.capture_duration_ms),
                    if step.capture_during_action { "Yes" } else { "No" }.to_string(),
                )
            } else {
                ("—".into(), "—".into(), "—".into(), "—".into(), "—".into())
            };

        let pct = if update.total_steps > 0 {
            (update.completed_steps as f64 / update.total_steps as f64 * 100.0) as u32
        } else {
            0
        };

        let learn_js = format!(
            concat!(
                "var st=document.getElementById('step-current');if(st)st.textContent='{completed}';",
                "var tt=document.getElementById('step-total');if(tt)tt.textContent='{total}';",
                "var it=document.getElementById('instruction-text');if(it)it.textContent='{instr}';",
                "var pf=document.getElementById('progress-fill');if(pf)pf.style.width='{pct}%';",
                "var sb=document.getElementById('session-state-badge');",
                "if(sb){{sb.textContent='{state_text}';sb.className='session-state-badge {state_class}';}}",
                "var sc=document.getElementById('signal-category');if(sc)sc.textContent='{category}';",
                "var sm=document.getElementById('signal-capture-mode');if(sm)sm.textContent='{capture_mode}';",
                "var asi=document.getElementById('adv-step-id');if(asi)asi.textContent='{step_id}';",
                "var acd=document.getElementById('adv-capture-duration');if(acd)acd.textContent='{capture_dur}';",
                "var adc=document.getElementById('adv-capture-during');if(adc)adc.textContent='{capture_during}';",
                "var ass=document.getElementById('adv-session-state');if(ass)ass.textContent='{state_debug}';",
                "var asd=document.getElementById('adv-steps-done');if(asd)asd.textContent='{completed}';",
                "var ast=document.getElementById('adv-steps-total');if(ast)ast.textContent='{total}';",
                "var acc=document.getElementById('adv-capture-count');if(acc)acc.textContent='{capture_count}';",
                "var afp=document.getElementById('adv-feature-probes');if(afp)afp.textContent='{feature_probes}';",
                "var air=document.getElementById('adv-interrupt-reads');if(air)air.textContent='{interrupt_reads}';",
            ),
            completed = update.completed_steps + 1,
            total = update.total_steps,
            instr = Self::escape_js(&instruction),
            pct = pct,
            state_text = Self::escape_js(state_text),
            state_class = state_class,
            category = Self::escape_js(&category),
            capture_mode = Self::escape_js(&capture_mode),
            step_id = Self::escape_js(&step_id),
            capture_dur = Self::escape_js(&capture_dur),
            capture_during = Self::escape_js(&capture_during),
            state_debug = Self::escape_js(&format!("{:?}", update.state)),
            capture_count = update.last_capture_count,
            feature_probes = format!(
                "{}/{}",
                update.last_feature_probes.1, update.last_feature_probes.0,
            ),
            interrupt_reads = update.last_interrupt_reads,
        );

        // Execute all JS in one batch.
        let full_js = format!("{overlay_js}{panel_js}{q_js}{v_js}{learn_js}");
        self.host.execute_js(&full_js);

        // If session reached Analyzing state, run the analyzer.
        if update.state == op_ai::SessionState::Analyzing {
            // Extract vid/pid before the mutable borrow.
            let (vid, pid) = match self.learning_session.as_ref() {
                Some((_, v, p)) => (*v, *p),
                None => return,
            };

            let session = &mut self.learning_session.as_mut().unwrap().0;
            let analyze_update = session.analyze();

            match &analyze_update.state {
                op_ai::SessionState::Verifying { .. } => {
                    // Patterns detected — send the first test command, then show
                    // verification panel.
                    if let Some(pattern) = session.current_verification_pattern() {
                        let (cmd_bytes, desc) = op_ai::test_command_for_pattern(pattern);
                        let handles = hid::HidHandle::open_all_interfaces(vid, pid);
                        for h in &handles {
                            if h.is_vendor_interface() && h.usage() == 0x0001 {
                                match h.write(&cmd_bytes) {
                                    Err(e) => log::warn!("Verify write failed: {e}"),
                                    Ok(_) => {
                                        let mut resp = [0u8; 256];
                                        match h.read(&mut resp, 50) {
                                            Ok(n) if n > 0 => {
                                                let hex: String = resp[..n.min(16)].iter()
                                                    .map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
                                                log::info!("Verify: SET response ({n}B): {hex}");
                                            }
                                            _ => {}
                                        }
                                        log::info!("Verify: sent test command — {desc}");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    self.push_session_update(&analyze_update, false);
                }
                op_ai::SessionState::Complete => {
                    // No patterns — build and save an empty profile.
                    let profile = session.build_profile();
                    log::info!(
                        "AI Learning complete — profile '{}' with {} signal(s)",
                        profile.id,
                        profile.signals.len(),
                    );

                    if let Err(e) = self.profile_store.save_yaml(&profile) {
                        log::error!("Failed to save profile: {e}");
                        self.host.execute_js(&format!(
                            "if(typeof showToast==='function')showToast('Failed to save profile: {}','error');",
                            Self::escape_js(&e.to_string()),
                        ));
                    } else {
                        self.host.execute_js(&format!(
                            concat!(
                                "var ov=document.getElementById('session-overlay');if(ov)ov.style.display='none';",
                                "var ss=document.getElementById('setup-sections');if(ss)ss.style.display='';",
                                "if(typeof showToast==='function')showToast('Profile saved: {}','success');",
                            ),
                            Self::escape_js(&profile.device_name),
                        ));
                    }

                    self.learning_session = None;
                }
                _ => {
                    // Unexpected state — just show whatever we got.
                    self.push_session_update(&analyze_update, false);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Addons: refresh
    // -----------------------------------------------------------------------

    fn ipc_addons_refresh(&mut self) {
        log::info!("IPC: addons/refresh — listing installed addons");

        let addons = self.addon_registry.list_addons();
        let count = addons.len();

        let cards = if addons.is_empty() {
            concat!(
                "<div class=\"empty-state\">",
                  "<div class=\"empty-state-icon\">A</div>",
                  "<p>No addons installed</p>",
                  "<p class=\"hint\">Download addons for your peripherals from the community, ",
                  "or create your own with the OpenPeripheral SDK.</p>",
                "</div>",
            ).to_string()
        } else {
            let mut html = String::new();
            for addon in &addons {
                let device_count = addon.supported_devices.len();
                let device_summary: String = addon
                    .supported_devices
                    .iter()
                    .take(3)
                    .map(|d| Self::escape_html(&d.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                let device_extra = if device_count > 3 {
                    format!(" +{} more", device_count - 3)
                } else {
                    String::new()
                };

                html.push_str(&format!(
                    concat!(
                        "<div class=\"addon-card\">",
                          "<div class=\"addon-meta\">",
                            "<div class=\"addon-icon\">A</div>",
                            "<div class=\"addon-info\">",
                              "<span class=\"addon-name\">{name}</span>",
                              "<span class=\"addon-version\">v{version} · {author}</span>",
                              "<span class=\"addon-devices\">{dev_n} device(s): {dev_summary}{dev_extra}</span>",
                            "</div>",
                          "</div>",
                        "</div>",
                    ),
                    name = Self::escape_html(&addon.name),
                    version = Self::escape_html(&addon.version),
                    author = Self::escape_html(&addon.author),
                    dev_n = device_count,
                    dev_summary = device_summary,
                    dev_extra = device_extra,
                ));
            }
            html
        };

        let js = format!(
            concat!(
                "var al=document.getElementById('addon-list');",
                "if(al)al.innerHTML='{cards}';",
                "var tag=al?al.closest('.panel'):null;",
                "if(tag){{var t=tag.querySelector('.tag');if(t)t.textContent='{count} addon(s)';}}",
            ),
            cards = Self::escape_js(&cards),
            count = count,
        );

        self.host.execute_js(&js);
        log::info!("Addons refresh complete: {} addon(s)", count);
    }

    // -----------------------------------------------------------------------
    // Profiles: refresh
    // -----------------------------------------------------------------------

    fn ipc_profiles_refresh(&mut self) {
        log::info!("IPC: profiles/refresh — listing saved profiles");

        let profiles = self.profile_store.list();
        let count = profiles.len();

        let cards = if profiles.is_empty() {
            concat!(
                "<div class=\"empty-state\">",
                  "<div class=\"empty-state-icon\">P</div>",
                  "<p>No profiles saved yet</p>",
                  "<p class=\"hint\">Complete an AI Learning session to generate a device profile, ",
                  "or import a community profile.</p>",
                "</div>",
            ).to_string()
        } else {
            let mut html = String::new();
            for profile in &profiles {
                let cap_names: Vec<&str> = profile
                    .capabilities
                    .iter()
                    .map(|c| Self::capability_label(c))
                    .collect();
                let cap_summary = if cap_names.is_empty() {
                    "No capabilities".to_string()
                } else {
                    cap_names.join(", ")
                };

                let pids: String = profile
                    .product_ids
                    .iter()
                    .map(|p| format!("{p:04X}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                let dtype_initial = profile.device_type.to_string();
                let icon_char = dtype_initial.chars().next().unwrap_or('?');

                let delete_args = format!(r#"{{"id":"{}"}}"#, Self::escape_js(&profile.id));

                html.push_str(&format!(
                    concat!(
                        "<div class=\"profile-card\">",
                          "<div class=\"profile-meta\">",
                            "<div class=\"profile-icon\">{icon}</div>",
                            "<div class=\"profile-info\">",
                              "<span class=\"profile-name\">{name}</span>",
                              "<span class=\"profile-detail\">{brand} · {dtype} · {vid:04X}:{pids}</span>",
                              "<span class=\"profile-detail\">{caps} · {sig_n} signal(s)</span>",
                            "</div>",
                          "</div>",
                          "<div class=\"addon-controls\">",
                            "<button class=\"btn-icon btn-danger\" title=\"Delete\" ",
                              "data-action=\"ipc\" data-ns=\"profiles\" data-cmd=\"delete\" ",
                              "data-args='{del_args}'>✕</button>",
                          "</div>",
                        "</div>",
                    ),
                    icon = icon_char,
                    name = Self::escape_html(&profile.device_name),
                    brand = Self::escape_html(&profile.brand),
                    dtype = Self::escape_html(&profile.device_type.to_string()),
                    vid = profile.vendor_id,
                    pids = pids,
                    caps = Self::escape_html(&cap_summary),
                    sig_n = profile.signals.len(),
                    del_args = delete_args,
                ));
            }
            html
        };

        let js = format!(
            concat!(
                "var pl=document.getElementById('profile-list');",
                "if(pl)pl.innerHTML='{cards}';",
                "var tag=pl?pl.closest('.panel'):null;",
                "if(tag){{var t=tag.querySelector('.tag');if(t)t.textContent='{count} profile(s)';}}",
            ),
            cards = Self::escape_js(&cards),
            count = count,
        );

        self.host.execute_js(&js);
        log::info!("Profiles refresh complete: {} profile(s)", count);
    }

    // -----------------------------------------------------------------------
    // Profiles: delete
    // -----------------------------------------------------------------------

    fn ipc_profiles_delete(&mut self, args: Option<serde_json::Value>) {
        let id = match args.as_ref().and_then(|a| a.get("id")).and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => {
                self.host.execute_js(
                    "if(typeof showToast==='function')showToast('Missing profile ID','error');",
                );
                return;
            }
        };

        log::info!("IPC: profiles/delete — removing '{id}'");

        match self.profile_store.delete(&id) {
            Ok(()) => {
                self.host.execute_js(&format!(
                    "if(typeof showToast==='function')showToast('Profile deleted','success');",
                ));
                // Refresh the list.
                self.ipc_profiles_refresh();
            }
            Err(e) => {
                log::error!("Failed to delete profile '{id}': {e}");
                self.host.execute_js(&format!(
                    "if(typeof showToast==='function')showToast('Delete failed: {}','error');",
                    Self::escape_js(&e.to_string()),
                ));
            }
        }
    }

    fn capability_label(cap: &op_core::device::Capability) -> &'static str {
        use op_core::device::Capability;
        match cap {
            Capability::Rgb { .. } => "RGB",
            Capability::Dpi { .. } => "DPI",
            Capability::PollingRate { .. } => "Polling Rate",
            Capability::Battery => "Battery",
            Capability::Equalizer { .. } => "Equalizer",
            Capability::Sidetone { .. } => "Sidetone",
            Capability::Macro => "Macro",
            Capability::KeyRemap => "Key Remap",
            Capability::MediaControl => "Media",
            Capability::Brightness { .. } => "Brightness",
            Capability::FirmwareUpdate => "Firmware",
            Capability::PressureSensitivity { .. } => "Pressure",
            Capability::ActiveArea => "Active Area",
            Capability::Custom { .. } => "Custom",
        }
    }

    // -----------------------------------------------------------------------
    // Utilities
    // -----------------------------------------------------------------------

    fn escape_js(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "")
    }

    fn escape_html(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    fn render_frame(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // FPS counter (every 128 frames).
        self.frame_count += 1;
        if self.frame_count & 0x7F == 0 {
            let elapsed = self.fps_timer.elapsed().as_secs_f64();
            if elapsed >= 2.0 {
                let fps = self.frame_count as f64 / elapsed;
                log::debug!("FPS: {fps:.1}");
                self.frame_count = 0;
                self.fps_timer = Instant::now();
            }
        }

        // Auto-scan HID devices every 8 seconds.
        if self.last_scan_time.elapsed().as_secs() >= 8 {
            self.devices_auto_scan();
        }

        // Tick the AppHost — needs mutable access to renderer.font_system.
        let (vw, vh, scale, ctx_w, ctx_h, events) = {
            let (ctx, renderer) = match (self.gpu_ctx.as_ref(), self.renderer.as_mut()) {
                (Some(c), Some(r)) => (c, r),
                _ => return,
            };

            let scale = self
                .window
                .as_ref()
                .map(|w| w.scale_factor() as f32)
                .unwrap_or(1.0);

            let vw = ctx.size.0 as f32 / scale;
            let vh = ctx.size.1 as f32 / scale;
            let ctx_w = ctx.size.0;
            let ctx_h = ctx.size.1;

            let events = self.host.tick(vw, vh, dt, &mut renderer.font_system);
            (vw, vh, scale, ctx_w, ctx_h, events)
        };

        for event in events {
            match event {
                AppEvent::NavigateTo(page_id) => {
                    log::info!("Navigated to: {page_id}");
                    if self.host.active_page() != Some(&page_id) {
                        self.host.navigate_to(&page_id);
                        self.host.init_js_for_active_page(ctx_w, ctx_h);
                    }
                    // Auto-populate data when landing on specific pages.
                    match page_id.as_str() {
                        "devices" => {
                            if self.cached_devices.is_empty() {
                                self.ipc_devices_scan();
                            } else {
                                self.populate_devices_ui(false);
                            }
                        }
                        "ai_learning" => {
                            if self.cached_devices.is_empty() {
                                // Need a scan first so the device selector has data.
                                self.ipc_devices_scan();
                            } else {
                                self.populate_ai_device_selector();
                            }
                        }
                        "addons" => self.ipc_addons_refresh(),
                        "profiles" => self.ipc_profiles_refresh(),
                        _ => {}
                    }
                }
                AppEvent::ContentSwapped { content_id } => {
                    log::info!("Content swapped to: {content_id}");
                    match content_id.as_str() {
                        "device_edit" => self.populate_device_edit_ui(),
                        "devices" => {
                            if self.cached_devices.is_empty() {
                                self.ipc_devices_scan();
                            } else {
                                self.populate_devices_ui(false);
                            }
                        }
                        _ => {}
                    }
                }
                AppEvent::PageReloaded(page_id) => {
                    log::info!("Page reloaded: {page_id}");
                    self.host.init_js_for_active_page(ctx_w, ctx_h);
                }
                AppEvent::OpenExternal(url) => {
                    log::info!("Open external: {url}");
                    #[cfg(target_os = "windows")]
                    {
                        let _ =
                            std::process::Command::new("cmd").args(["/C", "start", &url]).spawn();
                    }
                }
                AppEvent::TrayShowWindow => {
                    if let Some(ref win) = self.window {
                        win.set_visible(true);
                        win.focus_window();
                    }
                }
                AppEvent::TrayToggleWindow => {
                    if let Some(ref win) = self.window {
                        if win.is_visible().unwrap_or(true) {
                            win.set_visible(false);
                        } else {
                            win.set_visible(true);
                            win.focus_window();
                        }
                    }
                }
                AppEvent::TrayAction(action) => {
                    log::info!("Tray action: {action}");
                }
                AppEvent::CloseRequested => {
                    self.exit_requested = true;
                }
                AppEvent::SetTitle(title) => {
                    if let Some(ref win) = self.window {
                        win.set_title(&title);
                    }
                }
                AppEvent::MinimizeRequested => {
                    if let Some(ref win) = self.window {
                        win.set_minimized(true);
                    }
                }
                AppEvent::MaximizeToggleRequested => {
                    if let Some(ref win) = self.window {
                        win.set_maximized(!win.is_maximized());
                    }
                }
                AppEvent::WindowDragRequested => {
                    if let Some(ref win) = self.window {
                        let _ = win.drag_window();
                    }
                }
                AppEvent::IpcCommand { ns, cmd, args } => {
                    self.handle_ipc(&ns, &cmd, args);
                }
                _ => {}
            }
        }

        // Re-borrow for rendering.
        let (ctx, renderer) = match (self.gpu_ctx.as_ref(), self.renderer.as_mut()) {
            (Some(c), Some(r)) => (c, r),
            _ => return,
        };

        // Upload dirty canvas textures.
        let dirty = self.host.dirty_canvases();
        for (canvas_id, _node, width, height, rgba) in dirty {
            let slot = self.host.canvas_slot(canvas_id);
            renderer.upload_canvas_texture(&ctx.device, &ctx.queue, slot, width, height, &rgba);
        }
        self.host.commit_canvas_uploads();

        // Get scene instances and DevTools instances separately for correct z-layering.
        let (scene_instances, devtools_instances, clear_color) =
            self.host.split_instances(vw, vh);

        // Prepare scene text areas.
        let mut text_areas = if let Some(scene) = self.host.active_scene() {
            scene.text_areas()
        } else {
            Vec::new()
        };

        // Include title bar text areas if present.
        if let Some(tb_scene) = self.host.title_bar_scene() {
            text_areas.extend(tb_scene.text_areas());
        }

        // Prepare DevTools text entries (middle layer — above scene, below context menu).
        let devtools_entries = self.host.devtools_text_entries(vw, vh);
        let mut devtools_buffers: Vec<glyphon::Buffer> = Vec::new();
        for entry in &devtools_entries {
            let font_size = entry.font_size;
            let line_height = font_size * 1.3;
            let metrics = glyphon::Metrics::new(font_size, line_height);
            let mut buffer = glyphon::Buffer::new(&mut renderer.font_system, metrics);
            let weight = if entry.bold {
                glyphon::Weight(700)
            } else {
                glyphon::Weight(400)
            };
            let attrs = glyphon::Attrs::new()
                .family(glyphon::Family::SansSerif)
                .weight(weight);
            buffer.set_size(&mut renderer.font_system, Some(entry.width), None);
            buffer.set_text(
                &mut renderer.font_system,
                &entry.text,
                &attrs,
                glyphon::Shaping::Advanced,
                None,
            );
            buffer.shape_until_scroll(&mut renderer.font_system, false);
            devtools_buffers.push(buffer);
        }
        let mut devtools_text_areas: Vec<glyphon::TextArea<'_>> = Vec::new();
        for (i, entry) in devtools_entries.iter().enumerate() {
            if let Some(buf) = devtools_buffers.get(i) {
                let c = entry.color;
                devtools_text_areas.push(glyphon::TextArea {
                    buffer: buf,
                    left: entry.x,
                    top: entry.y,
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: entry.x as i32,
                        top: entry.y as i32,
                        right: (entry.x + entry.width) as i32,
                        bottom: (entry.y + entry.font_size * 2.0) as i32,
                    },
                    default_color: glyphon::Color::rgba(
                        (c.r * 255.0) as u8,
                        (c.g * 255.0) as u8,
                        (c.b * 255.0) as u8,
                        (c.a * 255.0) as u8,
                    ),
                    custom_glyphs: &[],
                });
            }
        }
        // DevTools text stays separate — don't merge with scene text.

        // Build context menu overlay layer (instances + text) for z-correct rendering.
        let ctx_menu_instances = self.host.context_menu_instances();
        let ctx_menu_entries = self.host.context_menu_text_entries();
        let mut ctx_menu_buffers: Vec<glyphon::Buffer> = Vec::new();
        for entry in &ctx_menu_entries {
            let font_size = entry.font_size;
            let line_height = font_size * 1.3;
            let metrics = glyphon::Metrics::new(font_size, line_height);
            let mut buffer = glyphon::Buffer::new(&mut renderer.font_system, metrics);
            let weight = if entry.bold { glyphon::Weight(700) } else { glyphon::Weight(400) };
            let attrs = glyphon::Attrs::new()
                .family(glyphon::Family::SansSerif)
                .weight(weight);
            buffer.set_size(&mut renderer.font_system, Some(entry.width), None);
            buffer.set_text(
                &mut renderer.font_system,
                &entry.text,
                &attrs,
                glyphon::Shaping::Advanced,
                None,
            );
            buffer.shape_until_scroll(&mut renderer.font_system, false);
            ctx_menu_buffers.push(buffer);
        }
        let mut ctx_menu_text_areas: Vec<glyphon::TextArea<'_>> = Vec::new();
        for (i, entry) in ctx_menu_entries.iter().enumerate() {
            if let Some(buf) = ctx_menu_buffers.get(i) {
                let c = entry.color;
                ctx_menu_text_areas.push(glyphon::TextArea {
                    buffer: buf,
                    left: entry.x,
                    top: entry.y,
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: entry.x as i32,
                        top: entry.y as i32,
                        right: (entry.x + entry.width) as i32,
                        bottom: (entry.y + entry.font_size * 2.0) as i32,
                    },
                    default_color: glyphon::Color::rgba(
                        (c.r * 255.0) as u8,
                        (c.g * 255.0) as u8,
                        (c.b * 255.0) as u8,
                        (c.a * 255.0) as u8,
                    ),
                    custom_glyphs: &[],
                });
            }
        }

        // Render with triple-layered pipeline:
        //   scene instances → scene text → devtools rects → devtools text → context menu rects → context menu text
        renderer.begin_frame(ctx, dt, scale);
        match renderer.render_triple_layered(
            ctx,
            &scene_instances,
            text_areas,
            &devtools_instances,
            devtools_text_areas,
            &ctx_menu_instances,
            ctx_menu_text_areas,
            clear_color,
        ) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                if let Some(ref mut gpu) = self.gpu_ctx {
                    let (w, h) = gpu.size;
                    gpu.resize(w, h);
                }
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("GPU out of memory — exiting");
                std::process::exit(1);
            }
            Err(e) => {
                log::warn!("Surface error: {e:?}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// winit ApplicationHandler
// ---------------------------------------------------------------------------

impl ApplicationHandler for OpenPeripheralApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        log::info!("Initialising OpenPeripheral window...");

        // Generate window icon — prefer declared icon from HTML, fall back to generated.
        let (icon_rgba, icon_w, icon_h) = load_declared_icon(&self.host, "window")
            .unwrap_or_else(|| crate::icon::generate_window_icon());
        let window_icon = winit::window::Icon::from_rgba(icon_rgba, icon_w, icon_h).ok();

        let attrs = WindowAttributes::default()
            .with_title("OpenPeripheral")
            .with_inner_size(PhysicalSize::new(1280u32, 800u32))
            .with_decorations(!self.host.has_custom_title_bar)
            .with_window_icon(window_icon);

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Failed to create window: {e}");
                event_loop.exit();
                return;
            }
        };

        let gpu_ctx = match pollster::block_on(GpuContext::new(window.clone())) {
            Ok(ctx) => ctx,
            Err(e) => {
                log::error!("GPU init failed: {e}");
                event_loop.exit();
                return;
            }
        };

        let renderer = match Renderer::new(&gpu_ctx) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Renderer init failed: {e}");
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window.clone());
        self.gpu_ctx = Some(gpu_ctx);
        self.renderer = Some(renderer);
        self.last_frame = Instant::now();
        self.fps_timer = Instant::now();

        // Initialize system tray (if TrayAccess capability declared).
        let (tray_rgba, tray_w, tray_h) = load_declared_icon(&self.host, "system")
            .unwrap_or_else(|| crate::icon::generate_tray_icon());
        self.host.init_tray_with_config(TrayConfig {
            enabled: true,
            tooltip: "OpenPeripheral".to_string(),
            icon_rgba: Some((tray_rgba, tray_w, tray_h)),
            ..TrayConfig::default()
        });

        // Initialize JS runtime for the active page.
        let (w, h) = self.gpu_ctx.as_ref().map(|c| c.size).unwrap_or((1280, 800));
        self.host.init_js_for_active_page(w, h);

        // Populate DevTools GPU info.
        if let Some(ref ctx) = self.gpu_ctx {
            let info = ctx.adapter.get_info();
            self.host.set_gpu_info(format!(
                "{} ({:?})",
                info.name,
                info.backend,
            ));
        }

        window.request_redraw();
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                if self.host.has_active_tray() {
                    log::info!("Minimizing to system tray.");
                    if let Some(ref win) = self.window {
                        win.set_visible(false);
                    }
                } else {
                    log::info!("Close requested — shutting down.");
                    event_loop.exit();
                }
            }

            WindowEvent::Resized(new_size) => {
                if let Some(ref mut ctx) = self.gpu_ctx {
                    ctx.resize(new_size.width, new_size.height);
                    if let Some(scene) = self.host.active_scene_mut() {
                        scene.invalidate_layout();
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                self.render_frame();
                if self.exit_requested {
                    event_loop.exit();
                    return;
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
                let lx = position.x as f32 / scale;
                let ly = position.y as f32 / scale;
                self.cursor_pos = (lx, ly);
                self.dispatch_input(RawInputEvent::MouseMove {
                    x: lx,
                    y: ly,
                });
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    winit::event::MouseButton::Left => CxMouseButton::Left,
                    winit::event::MouseButton::Right => CxMouseButton::Right,
                    winit::event::MouseButton::Middle => CxMouseButton::Middle,
                    _ => return,
                };
                // AppHost doesn't expose mouse_pos directly, use (0,0) as placeholder.
                // The InputHandler inside AppHost tracks position from MouseMove events.
                let raw = match state {
                    winit::event::ElementState::Pressed => {
                        RawInputEvent::MouseDown { x: 0.0, y: 0.0, button: btn }
                    }
                    winit::event::ElementState::Released => {
                        RawInputEvent::MouseUp { x: 0.0, y: 0.0, button: btn }
                    }
                };
                self.dispatch_input(raw);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 40.0, y * 40.0),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        (pos.x as f32, pos.y as f32)
                    }
                };
                self.dispatch_input(RawInputEvent::MouseWheel {
                    x: self.cursor_pos.0,
                    y: self.cursor_pos.1,
                    delta_x: dx,
                    delta_y: dy,
                });
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == winit::event::ElementState::Pressed {
                    let mods = Modifiers {
                        ctrl: self.current_modifiers.control_key(),
                        shift: self.current_modifiers.shift_key(),
                        alt: self.current_modifiers.alt_key(),
                    };
                    let key = winit_key_to_cx(&event.logical_key);
                    self.dispatch_input(RawInputEvent::KeyDown {
                        key,
                        modifiers: mods,
                    });

                    if let Some(text) = &event.text {
                        let s = text.to_string();
                        if !s.is_empty() && !mods.ctrl && !mods.alt {
                            let ch = s.chars().next().unwrap_or('\0');
                            if !ch.is_control() {
                                self.dispatch_input(RawInputEvent::TextInput { text: s });
                            }
                        }
                    }
                }
            }

            WindowEvent::ModifiersChanged(modifiers) => {
                self.current_modifiers = modifiers.state();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Poll tray events directly — request_redraw() may not trigger
        // RedrawRequested for hidden windows on Windows.
        for event in self.host.poll_tray() {
            match event {
                AppEvent::TrayShowWindow => {
                    if let Some(ref win) = self.window {
                        win.set_visible(true);
                        win.focus_window();
                    }
                }
                AppEvent::TrayToggleWindow => {
                    if let Some(ref win) = self.window {
                        if win.is_visible().unwrap_or(true) {
                            win.set_visible(false);
                        } else {
                            win.set_visible(true);
                            win.focus_window();
                        }
                    }
                }
                AppEvent::CloseRequested => {
                    self.exit_requested = true;
                }
                _ => {}
            }
        }

        // Ensure redraws continue even when window is hidden (for tray event polling).
        if let Some(ref w) = self.window {
            w.request_redraw();
        }
        if self.exit_requested {
            event_loop.exit();
        }
    }
}

fn winit_key_to_cx(key: &winit::keyboard::Key) -> KeyCode {
    use winit::keyboard::{Key as WKey, NamedKey};
    match key {
        WKey::Named(NamedKey::Enter) => KeyCode::Enter,
        WKey::Named(NamedKey::Tab) => KeyCode::Tab,
        WKey::Named(NamedKey::Escape) => KeyCode::Escape,
        WKey::Named(NamedKey::Backspace) => KeyCode::Backspace,
        WKey::Named(NamedKey::Delete) => KeyCode::Delete,
        WKey::Named(NamedKey::ArrowLeft) => KeyCode::Left,
        WKey::Named(NamedKey::ArrowRight) => KeyCode::Right,
        WKey::Named(NamedKey::ArrowUp) => KeyCode::Up,
        WKey::Named(NamedKey::ArrowDown) => KeyCode::Down,
        WKey::Named(NamedKey::Home) => KeyCode::Home,
        WKey::Named(NamedKey::End) => KeyCode::End,
        WKey::Named(NamedKey::PageUp) => KeyCode::PageUp,
        WKey::Named(NamedKey::PageDown) => KeyCode::PageDown,
        WKey::Named(NamedKey::Space) => KeyCode::Space,
        WKey::Character(c) => match c.as_str() {
            "a" | "A" => KeyCode::A,
            "c" | "C" => KeyCode::C,
            "v" | "V" => KeyCode::V,
            "x" | "X" => KeyCode::X,
            "z" | "Z" => KeyCode::Z,
            _ => KeyCode::Other(c.chars().next().unwrap_or('\0') as u32),
        },
        _ => KeyCode::Other(0),
    }
}
