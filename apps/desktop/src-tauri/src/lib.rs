//! Underlane: a menu bar app that runs and supervises cloudflared tunnels.

mod commands;
mod e2e;
mod hostnames;
mod setup;
mod separate_tunnel;
mod dns;
mod platform;
mod state;
mod tokens;
mod tray;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Manager, RunEvent};

use crate::state::Core;

/// Data directory: `UNDERLANE_DATA_DIR` (tests, e2e) or the platform app-data dir.
fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Some(d) = std::env::var_os("UNDERLANE_DATA_DIR") {
        return PathBuf::from(d);
    }
    app.path().app_data_dir().unwrap_or_else(|_| underlane_core::config::home_dir().join(".underlane"))
}

/// CLI entry for `--login-item`. Prints the resulting status as JSON.
pub fn login_item_cli(action: &str) -> i32 {
    let item = match action {
        "enable" | "on" => platform::set_login_item(true),
        "disable" | "off" => platform::set_login_item(false),
        _ => platform::login_item(),
    };
    println!("{}", serde_json::to_string(&item).unwrap_or_default());
    if item.message.as_deref().map(|m| m.starts_with("macOS refused")).unwrap_or(false) {
        1
    } else {
        0
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("UNDERLANE_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    platform::disable_state_restoration();
    let isolated = std::env::var_os("UNDERLANE_DATA_DIR").is_some();
    let mut builder = tauri::Builder::default();
    if !isolated {
        // A second launch (e.g. from Finder) just opens the dashboard of the running instance.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::open_dashboard(app, None);
        }));
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None));
    }

    let app = builder
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        // The dashboard is destroyed on close; this brings back its size and position next time.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::SIZE | tauri_plugin_window_state::StateFlags::POSITION)
                .with_denylist(&[tray::POPOVER])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::check_config_change,
            commands::load_hostnames,
            commands::save_hostnames,
            commands::check_hostname_dns,
            commands::create_hostname_dns,
            commands::connect_hostname_dns,
            commands::open_config_in_editor,
            commands::scan_default,
            commands::import_paths,
            commands::project_directory,
            commands::preview_project_config,
            commands::create_project,
            commands::setup_project_tunnel,
            commands::preview_separate_tunnel,
            commands::create_separate_tunnel,
            commands::cancel_project_setup,
            commands::add_quick_tunnel,
            commands::add_token_tunnel,
            commands::rename_group,
            commands::update_tunnel,
            commands::delete_tunnel,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::restart_tunnel,
            commands::start_all,
            commands::stop_all,
            commands::restart_all,
            commands::validate_tunnel,
            commands::probe_services,
            commands::query_logs,
            commands::clear_logs,
            commands::list_requests,
            commands::request_capture_paused,
            commands::set_request_capture_paused,
            commands::get_request,
            commands::clear_requests,
            commands::save_settings,
            commands::detect_cloudflared,
            commands::login_item_status,
            commands::set_launch_at_login,
            commands::open_dashboard,
            commands::hide_popover,
            commands::quit,
            commands::e2e_report,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            // setup runs inside applicationDidFinishLaunching, where the launch event is current.
            let at_login = platform::launched_as_login_item();

            let handle = app.handle().clone();
            let dir = data_dir(&handle);
            let shim = std::env::current_exe().ok();
            let core = tauri::async_runtime::block_on(Core::init(handle.clone(), dir, shim))?;
            app.manage(core.clone());
            let launch = serde_json::json!({ "at": underlane_core::proxy::now_ms(), "reason": if at_login { "login_item" } else { "normal" } });
            let _ = core.db.kv_set("last_launch", &launch.to_string());
            tracing::info!("launched {}", if at_login { "as a login item" } else { "normally" });
            tray::create(&handle)?;
            tray::set_count(&handle, 0);
            core.start_autostart_tunnels();

            #[cfg(all(feature = "e2e", debug_assertions))]
            if let Some(port) = std::env::var("UNDERLANE_E2E_PORT").ok().and_then(|p| p.parse().ok()) {
                e2e::start(handle.clone(), port);
            }

            // First run (nothing imported yet): open the dashboard so there's something to see.
            // Never pop a window when macOS opened Underlane at login.
            if !at_login && core.db.tunnels().map(|t| t.is_empty()).unwrap_or(false) && std::env::var_os("UNDERLANE_E2E_PORT").is_none() {
                tray::open_dashboard(&handle, None);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Underlane");

    app.run(|handle, event| match event {
        // Closing the dashboard must not quit a menu bar app; only an explicit quit does.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            // Stop every tunnel before the process goes away (logout won't wait for async work).
            if let Some(core) = handle.try_state::<Arc<Core>>() {
                let core = core.inner().clone();
                tauri::async_runtime::block_on(async move {
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(8), core.sup.stop_all()).await;
                });
            }
        }
        _ => {}
    });
}
