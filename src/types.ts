export interface Settings {
  api_token: string | null;
  wow_path: string | null;
  wow_account: string | null;
  auto_start: boolean;
  auto_sync: boolean;
}

export interface ConnectionStatus {
  connected: boolean;
  user_name: string | null;
  character_count: number | null;
  last_sync_time: string | null;
  last_sync_result: string | null;
}

export interface WowAccount {
  account_name: string;
  saved_variables_path: string;
  has_chartracker: boolean;
}

export type LogLevel = "Info" | "Success" | "Warning" | "Error";

export interface LogEntry {
  timestamp: string;
  level: LogLevel;
  message: string;
}
