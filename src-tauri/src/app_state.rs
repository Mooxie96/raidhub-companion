/// Shared application state, managed behind a Mutex for thread safety.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub api_token: Option<String>,
    pub wow_path: Option<String>,
    pub wow_account: Option<String>,
    pub auto_start: bool,
    pub auto_sync: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_token: None,
            wow_path: None,
            wow_account: None,
            auto_start: false,
            auto_sync: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub user_name: Option<String>,
    pub character_count: Option<u32>,
    pub last_sync_time: Option<String>,
    pub last_sync_result: Option<String>,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self {
            connected: false,
            user_name: None,
            character_count: None,
            last_sync_time: None,
            last_sync_result: None,
        }
    }
}

pub struct AppState {
    pub settings: Settings,
    pub connection: ConnectionStatus,
    pub log_entries: Vec<LogEntry>,
    pub watcher_running: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            connection: ConnectionStatus::default(),
            log_entries: Vec::new(),
            watcher_running: false,
        }
    }
}

impl AppState {
    pub fn add_log(&mut self, level: LogLevel, message: &str) {
        let now = chrono::Local::now();
        self.log_entries.push(LogEntry {
            timestamp: now.format("%H:%M:%S").to_string(),
            level,
            message: message.to_string(),
        });
        // Keep last 50 entries
        if self.log_entries.len() > 50 {
            self.log_entries.remove(0);
        }
    }
}

pub type SharedState = Mutex<AppState>;
