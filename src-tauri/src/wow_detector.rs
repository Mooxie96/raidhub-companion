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
    let root = normalize_wow_root(wow_root);

    let wtf_account_path = root.join("_classic_").join("WTF").join("Account");
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

/// Normalize a WoW path to always be the root directory (without _classic_ suffix).
/// Users might browse to `World of Warcraft\_classic_` instead of `World of Warcraft`,
/// which would cause doubled paths like `_classic_\_classic_`.
pub fn normalize_wow_root(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    // Strip _classic_, _classic_era_, _retail_ suffixes if present
    for suffix in &["_classic_", "_classic_era_", "_retail_"] {
        if path_str.ends_with(suffix) || path_str.ends_with(&format!("{}\\", suffix)) || path_str.ends_with(&format!("{}/", suffix)) {
            let trimmed = path_str.trim_end_matches('\\').trim_end_matches('/');
            if let Some(stripped) = trimmed.strip_suffix(suffix) {
                return PathBuf::from(stripped);
            }
        }
    }
    path.to_path_buf()
}

/// Get the full path to CharTracker.lua for a given account.
pub fn get_chartracker_path(wow_root: &Path, account_name: &str) -> PathBuf {
    normalize_wow_root(wow_root)
        .join("_classic_")
        .join("WTF")
        .join("Account")
        .join(account_name)
        .join("SavedVariables")
        .join("CharTracker.lua")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_wow_root_already_correct() {
        let path = PathBuf::from(r"C:\Program Files (x86)\World of Warcraft");
        assert_eq!(normalize_wow_root(&path), path);
    }

    #[test]
    fn test_normalize_wow_root_strips_classic() {
        let path = PathBuf::from(r"C:\Program Files (x86)\World of Warcraft\_classic_");
        let expected = PathBuf::from(r"C:\Program Files (x86)\World of Warcraft");
        assert_eq!(normalize_wow_root(&path), expected);
    }

    #[test]
    fn test_normalize_wow_root_strips_classic_with_trailing_slash() {
        let path = PathBuf::from(r"C:\Program Files (x86)\World of Warcraft\_classic_\");
        let expected = PathBuf::from(r"C:\Program Files (x86)\World of Warcraft");
        assert_eq!(normalize_wow_root(&path), expected);
    }

    #[test]
    fn test_normalize_wow_root_strips_classic_era() {
        let path = PathBuf::from(r"C:\World of Warcraft\_classic_era_");
        let expected = PathBuf::from(r"C:\World of Warcraft");
        assert_eq!(normalize_wow_root(&path), expected);
    }

    #[test]
    fn test_normalize_wow_root_strips_retail() {
        let path = PathBuf::from(r"D:\Games\World of Warcraft\_retail_");
        let expected = PathBuf::from(r"D:\Games\World of Warcraft");
        assert_eq!(normalize_wow_root(&path), expected);
    }

    #[test]
    fn test_chartracker_path_with_classic_root() {
        let root = PathBuf::from(r"C:\WoW\_classic_");
        let path = get_chartracker_path(&root, "12345#1");
        // Should NOT have _classic_\_classic_
        assert!(path.to_string_lossy().contains(r"_classic_\WTF"));
        assert!(!path.to_string_lossy().contains(r"_classic_\_classic_"));
    }

    #[test]
    fn test_chartracker_path_with_normal_root() {
        let root = PathBuf::from(r"C:\WoW");
        let path = get_chartracker_path(&root, "12345#1");
        assert!(path.to_string_lossy().contains(r"_classic_\WTF"));
    }
}
