/// WoW installation path detection for Windows.
/// Checks registry, common paths, and scans for CharTracker addon data.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WowAccount {
    pub account_name: String,
    pub saved_variables_path: String,
    pub has_chartracker: bool,
}

/// Try to find the WoW installation directory.
pub fn detect_wow_root() -> Option<PathBuf> {
    // 1. Check Windows Registry
    if let Some(path) = check_registry() {
        if path.exists() {
            return Some(path);
        }
    }

    // 2. Check common installation paths
    let common_paths = [
        r"C:\Program Files (x86)\World of Warcraft",
        r"C:\Program Files\World of Warcraft",
        r"D:\World of Warcraft",
        r"D:\Games\World of Warcraft",
        r"E:\World of Warcraft",
        r"E:\Games\World of Warcraft",
    ];

    for path_str in &common_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn check_registry() -> Option<PathBuf> {
    // Try both 64-bit and 32-bit registry views
    let paths_to_try = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Blizzard Entertainment\World of Warcraft"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Blizzard Entertainment\World of Warcraft"),
    ];

    for (hive, path) in &paths_to_try {
        if let Ok(key) = RegKey::predef(*hive).open_subkey(path) {
            // Try InstallPath first, then GamePath
            for value_name in &["InstallPath", "GamePath"] {
                if let Ok(install_path) = key.get_value::<String, _>(value_name) {
                    let path = PathBuf::from(&install_path);
                    if path.exists() {
                        return Some(path);
                    }
                }
            }
        }
    }

    None
}

/// Scan for WoW accounts with SavedVariables directories.
/// Returns all accounts found under _classic_\WTF\Account\*.
pub fn scan_wow_accounts(wow_root: &Path) -> Vec<WowAccount> {
    let mut accounts = Vec::new();

    let wtf_account_path = wow_root.join("_classic_").join("WTF").join("Account");
    if !wtf_account_path.exists() {
        return accounts;
    }

    if let Ok(entries) = std::fs::read_dir(&wtf_account_path) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let account_name = entry.file_name().to_string_lossy().to_string();

                // Skip non-account directories
                if account_name.starts_with('.') || account_name == "SavedVariables" {
                    continue;
                }

                let sv_path = entry.path().join("SavedVariables");
                let chartracker_path = sv_path.join("CharTracker.lua");

                accounts.push(WowAccount {
                    account_name,
                    saved_variables_path: sv_path.to_string_lossy().to_string(),
                    has_chartracker: chartracker_path.exists(),
                });
            }
        }
    }

    accounts
}

/// Get the full path to CharTracker.lua for a given account.
pub fn get_chartracker_path(wow_root: &Path, account_name: &str) -> PathBuf {
    wow_root
        .join("_classic_")
        .join("WTF")
        .join("Account")
        .join(account_name)
        .join("SavedVariables")
        .join("CharTracker.lua")
}
