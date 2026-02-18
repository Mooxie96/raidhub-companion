pub mod api_client;
pub mod app_state;
pub mod commands;
pub mod file_watcher;
pub mod lua_parser;
pub mod wow_detector;

use app_state::{AppState, LogLevel, SharedState};
use std::sync::Mutex;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Mutex::new(AppState::default()) as SharedState)
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_connection_status,
            commands::set_api_token,
            commands::check_connection,
            commands::detect_wow_path,
            commands::set_wow_path,
            commands::sync_now,
            commands::get_log_entries,
            commands::clear_log,
            commands::get_watcher_status,
        ])
        .setup(|app| {
            // Build tray menu
            let show = MenuItemBuilder::with_id("show", "Show Window").build(app)?;
            let sync = MenuItemBuilder::with_id("sync", "Sync Now").build(app)?;
            let check_update = MenuItemBuilder::with_id("check_update", "Nach Updates suchen").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show)
                .separator()
                .item(&sync)
                .item(&check_update)
                .separator()
                .item(&quit)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Raidhub Companion")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "sync" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app_handle.state::<SharedState>();
                            let _ = commands::sync_now(state).await;
                            let _ = app_handle.emit("sync-complete", ());
                        });
                    }
                    "check_update" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit("check-for-updates", ());
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Hide to tray on window close instead of quitting
            let window = app.get_webview_window("main").unwrap();
            let window_clone = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_clone.hide();
                }
            });

            // Restore persisted settings from store
            {
                use tauri_plugin_store::StoreExt;
                if let Ok(store) = app.store("settings.json") {
                    if let Some(val) = store.get("settings") {
                        if let Ok(settings) = serde_json::from_value::<app_state::Settings>(val) {
                            let state = app.state::<SharedState>();
                            state.lock().unwrap().settings = settings;
                        }
                    }
                }
            }

            // Log startup
            {
                let state = app.state::<SharedState>();
                state.lock().unwrap().add_log(LogLevel::Info, "Raidhub Companion started");
            }

            // Auto-detect WoW on first run (only if no persisted path)
            {
                let state = app.state::<SharedState>();
                let has_wow_path = state.lock().unwrap().settings.wow_path.is_some();
                if !has_wow_path {
                    let _ = commands::detect_wow_path(app.handle().clone(), app.state());
                }
            }

            // Start file watcher if configured
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                start_auto_sync(app_handle).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn start_auto_sync(app: tauri::AppHandle) {
    let (token, wow_path, wow_account, auto_sync) = {
        let state = app.state::<SharedState>();
        let s = state.lock().unwrap();
        (
            s.settings.api_token.clone(),
            s.settings.wow_path.clone(),
            s.settings.wow_account.clone(),
            s.settings.auto_sync,
        )
    };

    if !auto_sync {
        return;
    }
    let token = match token {
        Some(t) => t,
        None => return,
    };
    let wow_path = match wow_path {
        Some(p) => p,
        None => return,
    };
    let wow_account = match wow_account {
        Some(a) => a,
        None => return,
    };

    let _ = token; // Used later for validation if needed

    let sv_dir = std::path::PathBuf::from(&wow_path)
        .join("_classic_")
        .join("WTF")
        .join("Account")
        .join(&wow_account)
        .join("SavedVariables");

    if !sv_dir.exists() {
        let state = app.state::<SharedState>();
        state.lock().unwrap().add_log(
            LogLevel::Warning,
            &format!("SavedVariables directory not found: {}", sv_dir.display()),
        );
        return;
    }

    match file_watcher::start_watching(&sv_dir) {
        Ok((rx, _debouncer)) => {
            {
                let state = app.state::<SharedState>();
                let mut s = state.lock().unwrap();
                s.watcher_running = true;
                s.add_log(LogLevel::Info, "File watcher started — auto-sync enabled");
            }

            loop {
                match rx.recv() {
                    Ok(file_watcher::WatcherEvent::FileChanged(_path)) => {
                        {
                            let state = app.state::<SharedState>();
                            state.lock().unwrap().add_log(
                                LogLevel::Info,
                                "CharTracker.lua changed, syncing...",
                            );
                        }
                        let _ = commands::sync_now(app.state()).await;
                        let _ = app.emit("sync-complete", ());
                    }
                    Ok(file_watcher::WatcherEvent::Error(e)) => {
                        let state = app.state::<SharedState>();
                        state
                            .lock()
                            .unwrap()
                            .add_log(LogLevel::Error, &format!("File watcher error: {}", e));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            let state = app.state::<SharedState>();
            state.lock().unwrap().watcher_running = false;
        }
        Err(e) => {
            let state = app.state::<SharedState>();
            state
                .lock()
                .unwrap()
                .add_log(LogLevel::Error, &format!("Failed to start file watcher: {}", e));
        }
    }
}
