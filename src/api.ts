import { invoke } from "@tauri-apps/api/core";
import type {
  Settings,
  ConnectionStatus,
  WowAccount,
  LogEntry,
} from "./types";

export async function getSettings(): Promise<Settings> {
  return invoke("get_settings");
}

export async function saveSettings(settings: Settings): Promise<void> {
  return invoke("save_settings", { settings });
}

export async function getConnectionStatus(): Promise<ConnectionStatus> {
  return invoke("get_connection_status");
}

export async function setApiToken(
  token: string
): Promise<ConnectionStatus> {
  return invoke("set_api_token", { token });
}

export async function checkConnection(): Promise<ConnectionStatus> {
  return invoke("check_connection");
}

export async function detectWowPath(): Promise<WowAccount[]> {
  return invoke("detect_wow_path");
}

export async function setWowPath(path: string): Promise<WowAccount[]> {
  return invoke("set_wow_path", { path });
}

export async function syncNow(): Promise<string> {
  return invoke("sync_now");
}

export async function getLogEntries(): Promise<LogEntry[]> {
  return invoke("get_log_entries");
}

export async function clearLog(): Promise<void> {
  return invoke("clear_log");
}

export async function getWatcherStatus(): Promise<boolean> {
  return invoke("get_watcher_status");
}

export async function syncGdkp(): Promise<string> {
  return invoke("sync_gdkp");
}
