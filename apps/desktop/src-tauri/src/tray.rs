//! Menu bar item, popover and dashboard windows.

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::utils::config::WindowEffectsConfig;
use tauri::window::{Effect, EffectState};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_positioner::{Position, WindowExt};

pub const TRAY_ID: &str = "underlane";
pub const POPOVER: &str = "popover";
pub const DASHBOARD: &str = "dashboard";

/// The last title set on the status item (what the user sees next to the icon).
pub static TITLE: Mutex<String> = Mutex::new(String::new());

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Dashboard", true, None::<&str>)?;
    let start_all = MenuItem::with_id(app, "start_all", "Start All Tunnels", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, "stop_all", "Stop All Tunnels", true, None::<&str>)?;
    let restart_all = MenuItem::with_id(app, "restart_all", "Restart Running Tunnels", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Underlane", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &start_all, &stop_all, &restart_all, &PredefinedMenuItem::separator(app)?, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(include_bytes!("../icons/tray-template@2x.png"))?)
        .icon_as_template(true)
        .tooltip("Underlane")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => open_dashboard(app, None),
            "start_all" => crate::commands::start_all_inner(app),
            "restart_all" => crate::commands::restart_all_inner(app),
            "stop_all" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { crate::commands::stop_all_inner(&app).await });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_popover(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Show the active-tunnel count next to the icon (hidden at zero).
pub fn set_count<R: Runtime>(app: &AppHandle<R>, count: usize) {
    let title = if count == 0 { String::new() } else { count.to_string() };
    *TITLE.lock().unwrap() = title.clone();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(if title.is_empty() { None } else { Some(title.as_str()) });
        let tip = match count {
            0 => "Underlane: no tunnels running".to_string(),
            1 => "Underlane: 1 tunnel running".to_string(),
            n => format!("Underlane: {n} tunnels running"),
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

/// Read the title straight from the `NSStatusItem` button (used by the e2e check).
#[cfg_attr(not(all(feature = "e2e", debug_assertions)), allow(dead_code))]
pub fn status_item_title<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let tray = app.tray_by_id(TRAY_ID)?;
        let (tx, rx) = std::sync::mpsc::channel();
        let t2 = tray.clone();
        app.run_on_main_thread(move || {
            let title = t2
                .with_inner_tray_icon(|inner| {
                    inner.ns_status_item().and_then(|item| {
                        let mtm = objc2::MainThreadMarker::new()?;
                        item.button(mtm).map(|b| b.title().to_string())
                    })
                })
                .ok()
                .flatten();
            let _ = tx.send(title);
        })
        .ok()?;
        rx.recv_timeout(std::time::Duration::from_secs(2)).ok().flatten()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Some(TITLE.lock().unwrap().clone())
    }
}

fn theme_of(theme: &str) -> Option<tauri::Theme> {
    match theme {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    }
}

/// Native window appearance follows the app theme, so vibrancy materials match the CSS.
pub fn apply_window_theme<R: Runtime>(app: &AppHandle<R>, theme: &str) {
    for w in app.webview_windows().values() {
        let _ = w.set_theme(theme_of(theme));
    }
}

fn current_theme<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::Theme> {
    let core = app.try_state::<std::sync::Arc<crate::state::Core>>()?;
    let t = core.settings.read().unwrap().theme.clone();
    theme_of(&t)
}

fn popover<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    if let Some(w) = app.get_webview_window(POPOVER) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, POPOVER, WebviewUrl::App("popover.html".into()))
        .title("Underlane")
        .inner_size(360.0, 500.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .shadow(true)
        .visible_on_all_workspaces(true)
        .theme(current_theme(app))
        .effects(WindowEffectsConfig { effects: vec![Effect::Popover], state: Some(EffectState::Active), radius: Some(12.0), color: None, ..Default::default() })
        .build()?;
    let w2 = w.clone();
    w.on_window_event(move |e| {
        if let WindowEvent::Focused(false) = e {
            let _ = w2.hide();
        }
    });
    Ok(w)
}

pub fn toggle_popover<R: Runtime>(app: &AppHandle<R>) {
    let Ok(w) = popover(app) else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    let _ = w.move_window(Position::TrayCenter);
    let _ = w.show();
    let _ = w.set_focus();
    let _ = w.emit("popover-shown", ());
}

pub fn hide_popover<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(POPOVER) {
        let _ = w.hide();
    }
}

/// Open (or focus) the dashboard. `view` selects a section, e.g. "requests:<tunnel-id>".
pub fn open_dashboard<R: Runtime>(app: &AppHandle<R>, view: Option<String>) {
    hide_popover(app);
    if let Some(w) = app.get_webview_window(DASHBOARD) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(v) = view {
            let _ = w.emit("navigate", v);
        }
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        crate::platform::set_dock_icon();
    }
    let url = match &view {
        Some(v) => format!("index.html#{v}"),
        None => "index.html".into(),
    };
    let builder = WebviewWindowBuilder::new(app, DASHBOARD, WebviewUrl::App(url.into()))
        .title("Underlane")
        .inner_size(1080.0, 700.0)
        // onboarding_status enables resizing only for the completed dashboard.
        .resizable(false)
        // 960 = sidebar (200) + the Requests split's column minimums (420 + 340).
        .min_inner_size(960.0, 560.0)
        .center()
        // Shown after build, once the window-state plugin has restored the last size and position.
        .visible(false)
        .theme(current_theme(app))
        .transparent(true)
        .effects(WindowEffectsConfig { effects: vec![Effect::Sidebar], state: Some(EffectState::FollowsWindowActiveState), radius: None, color: None, ..Default::default() });
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    match builder.build() {
        Ok(w) => {
            let app2 = app.clone();
            w.on_window_event(move |e| {
                if let WindowEvent::Destroyed = e {
                    // Back to a pure menu bar app once the dashboard is gone.
                    #[cfg(target_os = "macos")]
                    let _ = app2.set_activation_policy(tauri::ActivationPolicy::Accessory);
                    let _ = &app2;
                }
            });
            let _ = w.show();
            let _ = w.set_focus();
        }
        Err(e) => tracing::error!("could not open dashboard: {e}"),
    }
}
