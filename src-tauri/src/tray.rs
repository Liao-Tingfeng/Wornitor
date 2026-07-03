//! System tray for WorkFlow — macOS menu-bar icon with context menu.
//!
//! Provides 4 icon states (recording / paused / idle / error) generated
//! programmatically so there is no dependency on external icon files.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{
    Emitter, Manager,
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent, TrayIcon},
    AppHandle, Runtime,
};

// ── Shared tray state ───────────────────────────────────────────

/// Runtime state tracked by the tray icon.
#[derive(Clone)]
pub(crate) struct TrayState {
    inner: Arc<TrayStateInner>,
}

struct TrayStateInner {
    paused: AtomicBool,
    recording: AtomicBool,
    error: AtomicBool,
}

impl TrayState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TrayStateInner {
                paused: AtomicBool::new(false),
                recording: AtomicBool::new(false),
                error: AtomicBool::new(false),
            }),
        }
    }

    pub fn set_recording(&self, v: bool) {
        self.inner.recording.store(v, Ordering::Release);
    }

    pub fn set_paused(&self, v: bool) {
        self.inner.paused.store(v, Ordering::Release);
    }

    pub fn set_error(&self, v: bool) {
        self.inner.error.store(v, Ordering::Release);
    }

    fn icon_kind(&self) -> IconKind {
        if self.inner.error.load(Ordering::Acquire) {
            return IconKind::Error;
        }
        if self.inner.recording.load(Ordering::Acquire) {
            if self.inner.paused.load(Ordering::Acquire) {
                return IconKind::Paused;
            }
            return IconKind::Recording;
        }
        IconKind::Idle
    }
}

impl Default for TrayState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Icon generation ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum IconKind {
    Recording,
    Paused,
    Idle,
    Error,
}

impl IconKind {
    /// Return RGBA pixel data (row-major) for a 22×22 icon.
    fn to_rgba(self) -> (Vec<u8>, u32, u32) {
        let size = 22u32;
        let (cx, cy, r) = (size / 2, size / 2, size / 2 - 2);
        let mut pixels = vec![0u8; (size * size * 4) as usize];

        let (cr, cg, cb) = match self {
            IconKind::Recording => (232, 57, 57),   // red
            IconKind::Paused => (149, 149, 149),     // grey
            IconKind::Idle => (76, 175, 80),         // green
            IconKind::Error => (255, 160, 0),        // amber
        };

        for y in 0..size {
            for x in 0..size {
                let idx = ((y * size + x) * 4) as usize;
                let dx = (x as i32 - cx as i32).abs();
                let dy = (y as i32 - cy as i32).abs();
                let dist = ((dx * dx + dy * dy) as f64).sqrt();

                if self == IconKind::Idle {
                    // Ring: inner radius r-3, outer radius r-1
                    let inner_r = r as f64 - 3.0;
                    let outer_r = r as f64 - 1.0;
                    if dist >= inner_r - 0.5 && dist <= outer_r + 0.5 {
                        let mut alpha = 1.0f64;
                        if dist < inner_r {
                            alpha = 1.0 - (inner_r - dist);
                        } else if dist > outer_r {
                            alpha = 1.0 - (dist - outer_r);
                        }
                        let a = (240.0 * alpha) as u8;
                        pixels[idx] = cr;
                        pixels[idx + 1] = cg;
                        pixels[idx + 2] = cb;
                        pixels[idx + 3] = a;
                    }
                } else {
                    // Filled circle
                    if dist <= r as f64 + 0.5 {
                        let alpha = if dist <= r as f64 - 0.5 {
                            1.0
                        } else {
                            1.0 - (dist - (r as f64 - 0.5))
                        };
                        let a = (240.0 * alpha) as u8;
                        pixels[idx] = cr;
                        pixels[idx + 1] = cg;
                        pixels[idx + 2] = cb;
                        pixels[idx + 3] = a;
                    }
                }
            }
        }

        (pixels, size, size)
    }
}

// ── Menu item IDs ───────────────────────────────────────────────

const MENU_SHOW: &str = "show";
const MENU_PAUSE: &str = "pause";
const MENU_QUIT: &str = "quit";

// ── Tray builder ────────────────────────────────────────────────

/// Create and register the system tray icon with its context menu.
pub(crate) fn create_tray<R: Runtime>(
    app: &AppHandle<R>,
    state: TrayState,
) -> Result<TrayIcon<R>, Box<dyn std::error::Error>> {
    let (rgba, w, h) = IconKind::Idle.to_rgba();
    let icon = Image::new_owned(rgba, w, h);

    let show = MenuItemBuilder::with_id(MENU_SHOW, "打开 WorkFlow").build(app)?;
    let pause = MenuItemBuilder::with_id(MENU_PAUSE, "暂停记录").build(app)?;
    let _separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id(MENU_QUIT, "退出").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&pause)
        .separator()
        .item(&quit)
        .build()?;

    eprintln!("[TRAY] Tray created with 4 menu items: show/pause/separator/quit");

    let pause_state = state.clone();

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("WorkFlow")
        .on_menu_event(move |app_handle, event| {
            match event.id().as_ref() {
                MENU_SHOW => toggle_window(app_handle),
                MENU_PAUSE => toggle_pause(app_handle, &pause_state),
                MENU_QUIT => {
                    app_handle.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(tray)
}

// ── Helpers ─────────────────────────────────────────────────────

/// Show or hide the main window.
fn toggle_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            eprintln!("[TRAY] Window toggle: now visible=false");
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            eprintln!("[TRAY] Window toggle: now visible=true");
        }
    }
}

/// Toggle pause/resume capture state.
fn toggle_pause<R: Runtime>(app: &AppHandle<R>, state: &TrayState) {
    let was_paused = state.inner.paused.load(Ordering::Acquire);
    let now_paused = !was_paused;
    state.inner.paused.store(now_paused, Ordering::Release);
    eprintln!("[TRAY] Pause toggled: now_paused={now_paused}");

    // Rebuild menu with updated label
    if let Ok(show) = MenuItemBuilder::with_id(MENU_SHOW, "打开 WorkFlow").build(app) {
        let label = if now_paused { "恢复记录" } else { "暂停记录" };
        if let Ok(pause) = MenuItemBuilder::with_id(MENU_PAUSE, label).build(app) {
            let _separator = PredefinedMenuItem::separator(app);
            let quit = MenuItemBuilder::with_id(MENU_QUIT, "退出").build(app);
            if let Ok(menu) = MenuBuilder::new(app)
                .item(&show)
                .item(&pause)
                .separator()
                .item(&quit.unwrap_or_else(|_| panic!("quit menu item")))
                .build()
            {
                // Find the tray by its id and update the menu
                if let Some(tray) = app.tray_by_id("main") {
                    let _ = tray.set_menu(Some(menu));
                }
            }
        }
    }

    // Emit an event so the frontend can react
    let _ = app.emit("tray-pause-changed", now_paused);
}

/// Refresh the tray icon to reflect the current state.
#[allow(dead_code)]
pub(crate) fn update_tray_icon<R: Runtime>(handle: &TrayIcon<R>, state: &TrayState) {
    let (rgba, w, h) = state.icon_kind().to_rgba();
    let icon = Image::new_owned(rgba, w, h);
    let _ = handle.set_icon(Some(icon));
}
