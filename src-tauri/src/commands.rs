/// Tauri IPC command handlers.
/// These functions are callable from the frontend via invoke().

use crate::api_client::RaidhubApiClient;
use crate::app_state::{ConnectionStatus, LogEntry, LogLevel, Settings, SharedState};
use crate::lua_parser;
use crate::wow_detector::{self, WowAccount};
use std::path::PathBuf;
use tauri::{AppHandle, State};
use tauri_plugin_store::StoreExt;

/// Persist current settings to the store so they survive app restarts.
fn persist_settings(app: &AppHandle, settings: &Settings) {
    if let Ok(store) = app.store("settings.json") {
        if let Ok(val) = serde_json::to_value(settings) {
            store.set("settings", val);
        }
    }
}

// ============================================================
// Settings
// ============================================================

#[tauri::command]
pub fn get_settings(state: State<'_, SharedState>) -> Settings {
    state.lock().unwrap().settings.clone()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, SharedState>, settings: Settings) {
    state.lock().unwrap().settings = settings.clone();
    persist_settings(&app, &settings);
}

// ============================================================
// Connection
// ============================================================

#[tauri::command]
pub fn get_connection_status(state: State<'_, SharedState>) -> ConnectionStatus {
    state.lock().unwrap().connection.clone()
}

#[tauri::command]
pub async fn set_api_token(
    app: AppHandle,
    state: State<'_, SharedState>,
    token: String,
) -> Result<ConnectionStatus, String> {
    // Validate token format
    if !token.starts_with("ct_") || token.len() < 10 {
        return Err("Invalid token format. Token must start with 'ct_'.".to_string());
    }

    // Test the token by calling the status endpoint
    let client = RaidhubApiClient::new(&token);
    match client.get_status().await {
        Ok(status) => {
            let conn = ConnectionStatus {
                connected: true,
                user_name: Some(status.user.display_name.clone()),
                character_count: Some(status.character_count),
                last_sync_time: status.last_sync.as_ref().map(|s| s.timestamp.clone()),
                last_sync_result: status.last_sync.map(|s| {
                    format!("{} imported, {} updated", s.imported, s.updated)
                }),
            };

            let mut state = state.lock().unwrap();
            state.settings.api_token = Some(token);
            state.connection = conn.clone();
            state.add_log(
                LogLevel::Success,
                &format!("Connected as {}", status.user.display_name),
            );
            persist_settings(&app, &state.settings);

            Ok(conn)
        }
        Err(e) => {
            let mut state = state.lock().unwrap();
            state.connection = ConnectionStatus::default();
            state.add_log(LogLevel::Error, &format!("Connection failed: {}", e));
            Err(format!("Connection failed: {}", e))
        }
    }
}

#[tauri::command]
pub async fn check_connection(state: State<'_, SharedState>) -> Result<ConnectionStatus, String> {
    let token = {
        let app = state.lock().unwrap();
        app.settings.api_token.clone()
    };

    let token = token.ok_or("No API token configured")?;
    let client = RaidhubApiClient::new(&token);

    match client.get_status().await {
        Ok(status) => {
            let conn = ConnectionStatus {
                connected: true,
                user_name: Some(status.user.display_name),
                character_count: Some(status.character_count),
                last_sync_time: status.last_sync.as_ref().map(|s| s.timestamp.clone()),
                last_sync_result: status.last_sync.map(|s| {
                    format!("{} imported, {} updated", s.imported, s.updated)
                }),
            };
            state.lock().unwrap().connection = conn.clone();
            Ok(conn)
        }
        Err(e) => {
            let mut app = state.lock().unwrap();
            app.connection.connected = false;
            Err(format!("{}", e))
        }
    }
}

// ============================================================
// WoW Detection
// ============================================================

#[tauri::command]
pub fn detect_wow_path(app: AppHandle, state: State<'_, SharedState>) -> Result<Vec<WowAccount>, String> {
    let wow_root = wow_detector::detect_wow_root()
        .ok_or("Could not detect WoW installation. Please select the folder manually.")?;

    let accounts = wow_detector::scan_wow_accounts(&wow_root);

    if accounts.is_empty() {
        return Err(format!(
            "WoW found at {} but no Classic accounts detected.",
            wow_root.display()
        ));
    }

    // Store the WoW root path
    let mut state = state.lock().unwrap();
    state.settings.wow_path = Some(wow_root.to_string_lossy().to_string());

    // Auto-select first account with CharTracker
    if state.settings.wow_account.is_none() {
        if let Some(ct_account) = accounts.iter().find(|a| a.has_chartracker) {
            state.settings.wow_account = Some(ct_account.account_name.clone());
        } else if let Some(first) = accounts.first() {
            state.settings.wow_account = Some(first.account_name.clone());
        }
    }

    state.add_log(
        LogLevel::Info,
        &format!(
            "Found WoW at {}. {} account(s) detected.",
            wow_root.display(),
            accounts.len()
        ),
    );

    persist_settings(&app, &state.settings);

    Ok(accounts)
}

#[tauri::command]
pub fn set_wow_path(app: AppHandle, state: State<'_, SharedState>, path: String) -> Result<Vec<WowAccount>, String> {
    let wow_root = PathBuf::from(&path);
    if !wow_root.exists() {
        return Err("Path does not exist".to_string());
    }

    let accounts = wow_detector::scan_wow_accounts(&wow_root);
    let mut state = state.lock().unwrap();
    state.settings.wow_path = Some(path);
    persist_settings(&app, &state.settings);

    Ok(accounts)
}

// ============================================================
// Sync
// ============================================================

#[tauri::command]
pub async fn sync_now(state: State<'_, SharedState>) -> Result<String, String> {
    let (token, wow_path, wow_account) = {
        let app = state.lock().unwrap();
        (
            app.settings.api_token.clone(),
            app.settings.wow_path.clone(),
            app.settings.wow_account.clone(),
        )
    };

    let token = token.ok_or("No API token configured")?;
    let wow_path = wow_path.ok_or("WoW path not configured")?;
    let wow_account = wow_account.ok_or("WoW account not selected")?;

    state
        .lock()
        .unwrap()
        .add_log(LogLevel::Info, "Starting sync...");

    // Read CharTracker.lua
    let chartracker_path =
        wow_detector::get_chartracker_path(&PathBuf::from(&wow_path), &wow_account);

    if !chartracker_path.exists() {
        let msg = format!(
            "CharTracker.lua not found at {}. Make sure the addon is installed and you've logged in at least once.",
            chartracker_path.display()
        );
        state.lock().unwrap().add_log(LogLevel::Error, &msg);
        return Err(msg);
    }

    // Read file with retries (may be locked by WoW)
    let file_content = read_file_with_retry(&chartracker_path, 3).await?;

    // Parse Lua
    let export_data = lua_parser::parse_chartracker_db(&file_content).map_err(|e| {
        let msg = format!("Failed to parse CharTracker.lua: {}", e);
        state.lock().unwrap().add_log(LogLevel::Error, &msg);
        msg
    })?;

    // Validate basic structure
    if export_data.get("characters").is_none() {
        let msg = "Invalid CharTracker data: no characters field".to_string();
        state.lock().unwrap().add_log(LogLevel::Error, &msg);
        return Err(msg);
    }

    // Sync to API
    let client = RaidhubApiClient::new(&token);
    match client.sync_characters(&export_data).await {
        Ok(response) => {
            let result_msg = format!(
                "Synced {} new, {} updated characters",
                response.synced, response.updated
            );

            let mut app = state.lock().unwrap();
            app.connection.connected = true;
            app.connection.last_sync_time = Some(response.timestamp);
            app.connection.last_sync_result = Some(result_msg.clone());
            app.add_log(LogLevel::Success, &result_msg);

            Ok(result_msg)
        }
        Err(crate::api_client::ApiError::Unauthorized) => {
            let mut app = state.lock().unwrap();
            app.connection.connected = false;
            app.add_log(LogLevel::Error, "Token expired or revoked");
            Err("Token expired or revoked. Please generate a new token on the website.".to_string())
        }
        Err(e) => {
            let msg = format!("Sync failed: {}", e);
            state.lock().unwrap().add_log(LogLevel::Error, &msg);
            Err(msg)
        }
    }
}

async fn read_file_with_retry(path: &PathBuf, max_retries: u32) -> Result<String, String> {
    for attempt in 0..max_retries {
        match std::fs::read_to_string(path) {
            Ok(content) => return Ok(content),
            Err(e) => {
                if attempt < max_retries - 1 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                } else {
                    return Err(format!(
                        "Could not read {}: {} (tried {} times)",
                        path.display(),
                        e,
                        max_retries
                    ));
                }
            }
        }
    }
    unreachable!()
}

// ============================================================
// Log
// ============================================================

#[tauri::command]
pub fn get_log_entries(state: State<'_, SharedState>) -> Vec<LogEntry> {
    state.lock().unwrap().log_entries.clone()
}

#[tauri::command]
pub fn clear_log(state: State<'_, SharedState>) {
    state.lock().unwrap().log_entries.clear();
}

// ============================================================
// Watcher status
// ============================================================

#[tauri::command]
pub fn get_watcher_status(state: State<'_, SharedState>) -> bool {
    state.lock().unwrap().watcher_running
}
