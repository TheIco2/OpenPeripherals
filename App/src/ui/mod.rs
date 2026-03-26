mod pages;

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use canvasx_runtime::gpu::context::GpuContext;
use canvasx_runtime::gpu::renderer::Renderer;
use canvasx_runtime::scene::app_host::{AppEvent, AppHost, PageSource, Route};
use canvasx_runtime::scene::input_handler::{
    KeyCode, Modifiers, MouseButton as CxMouseButton, RawInputEvent,
};
use canvasx_runtime::capabilities::{CapabilitySet, NetworkAccess, TrayAccess};
use canvasx_runtime::tray::TrayConfig;
use op_addon::AddonRegistry;
use op_core::device::DeviceRegistry;
use op_core::profile::ProfileStore;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

/// Build the AppHost with all pages and launch the CanvasX renderer.
pub fn launch(
    _device_registry: Arc<DeviceRegistry>,
    _profile_store: ProfileStore,
    _addon_registry: AddonRegistry,
) -> Result<()> {
    let host = build_app_host();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = OpenPeripheralApp::new(host);

    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("Event loop error: {e}");
    }

    Ok(())
}

fn build_app_host() -> AppHost {
    let mut host = AppHost::new("OpenPeripheral");
    host.sidebar_width = 0.0; // Sidebar is part of the page HTML

    host.set_capabilities(
        CapabilitySet::new()
            .declare(TrayAccess)
            .declare(NetworkAccess),
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
// Application state (mirrors CanvasX's own main.rs pattern)
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
}

impl OpenPeripheralApp {
    fn new(host: AppHost) -> Self {
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

        // Tick the AppHost — this drives layout → animate → paint on the active page.
        let events = self.host.tick(vw, vh, dt, &mut renderer.font_system);

        for event in events {
            match event {
                AppEvent::NavigateTo(page_id) => {
                    log::info!("Navigated to: {page_id}");
                    // Only re-init when navigating to a different page.
                    // The initial NavigateTo from build_app_host is already
                    // handled by resumed() calling init_js_for_active_page.
                    if self.host.active_page() != Some(&page_id) {
                        self.host.navigate_to(&page_id);
                        let w = ctx.size.0;
                        let h = ctx.size.1;
                        self.host.init_js_for_active_page(w, h);
                    }
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
                _ => {}
            }
        }

        // Upload dirty canvas textures.
        let dirty = self.host.dirty_canvases();
        for (canvas_id, _node, width, height, rgba) in dirty {
            let slot = self.host.canvas_slot(canvas_id);
            renderer.upload_canvas_texture(&ctx.device, &ctx.queue, slot, width, height, &rgba);
        }
        self.host.commit_canvas_uploads();

        // Get combined scene + DevTools instances.
        let (instances, clear_color) = self.host.combined_instances(vw, vh);

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

        // Prepare DevTools text entries.
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
        let mut all_text_areas = text_areas;
        all_text_areas.extend(devtools_text_areas);

        // Render.
        renderer.begin_frame(ctx, dt, scale);
        match renderer.render(ctx, &instances, all_text_areas, clear_color) {
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

        // Generate window icon.
        let (icon_rgba, icon_w, icon_h) = crate::icon::generate_window_icon();
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
        let (tray_rgba, tray_w, tray_h) = crate::icon::generate_tray_icon();
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
                self.dispatch_input(RawInputEvent::MouseMove {
                    x: position.x as f32,
                    y: position.y as f32,
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
                    x: 0.0,
                    y: 0.0,
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
