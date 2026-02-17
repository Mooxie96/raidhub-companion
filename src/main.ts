import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import * as api from "./api";
import type { LogEntry, WowAccount } from "./types";

// ============================================================
// DOM Elements
// ============================================================

const $ = (id: string) => document.getElementById(id)!;

const statusDot = $("status-dot");
const statusText = $("status-text");
const statusDetails = $("status-details");
const charCount = $("char-count");
const lastSync = $("last-sync");
const watcherBadge = $("watcher-badge");

const tokenEmpty = $("token-empty");
const tokenSet = $("token-set");
const tokenInput = $("token-input") as HTMLInputElement;
const tokenPrefix = $("token-prefix");
const btnSetToken = $("btn-set-token");
const btnClearToken = $("btn-clear-token");

const wowPath = $("wow-path") as HTMLInputElement;
const btnBrowse = $("btn-browse");
const accountField = $("account-field");
const accountSelect = $("account-select") as HTMLSelectElement;

const toggleAutoSync = $("toggle-autosync") as HTMLInputElement;
const toggleAutoStart = $("toggle-autostart") as HTMLInputElement;

const logScroll = $("log-scroll");
const logEmpty = $("log-empty");
const btnClearLog = $("btn-clear-log");

const btnSync = $("btn-sync") as HTMLButtonElement;
const linkWebsite = $("link-website");

// ============================================================
// State
// ============================================================

let syncing = false;

// ============================================================
// UI Update Functions
// ============================================================

function updateConnectionUI(connected: boolean, userName?: string | null) {
  statusDot.className = `status-dot ${connected ? "connected" : "disconnected"}`;
  statusText.textContent = connected
    ? `Connected as ${userName || "User"}`
    : "Not connected";
}

function updateStatusDetails(
  characters?: number | null,
  syncTime?: string | null,
  syncResult?: string | null
) {
  if (characters != null || syncTime != null) {
    statusDetails.style.display = "flex";
    if (characters != null) charCount.textContent = String(characters);
    if (syncTime) {
      const date = new Date(syncTime);
      lastSync.textContent = formatTimeAgo(date);
    } else if (syncResult) {
      lastSync.textContent = syncResult;
    }
  }
}

function formatTimeAgo(date: Date): string {
  const seconds = Math.floor((Date.now() - date.getTime()) / 1000);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return date.toLocaleDateString();
}

function showTokenSet(token: string) {
  tokenEmpty.style.display = "none";
  tokenSet.style.display = "flex";
  tokenPrefix.textContent = token.substring(0, 11) + "••••••••";
  btnSync.disabled = false;
}

function showTokenEmpty() {
  tokenEmpty.style.display = "flex";
  tokenSet.style.display = "none";
  tokenInput.value = "";
  btnSync.disabled = true;
}

function populateAccounts(accounts: WowAccount[]) {
  accountSelect.innerHTML = "";
  if (accounts.length === 0) {
    accountField.style.display = "none";
    return;
  }

  accountField.style.display = "block";
  for (const acc of accounts) {
    const opt = document.createElement("option");
    opt.value = acc.account_name;
    opt.textContent = acc.account_name + (acc.has_chartracker ? " (CharTracker found)" : "");
    accountSelect.appendChild(opt);
  }
}

function renderLogs(entries: LogEntry[]) {
  if (entries.length === 0) {
    logEmpty.style.display = "block";
    logScroll.querySelectorAll(".log-entry").forEach((el) => el.remove());
    return;
  }

  logEmpty.style.display = "none";
  const existing = logScroll.querySelectorAll(".log-entry");
  existing.forEach((el) => el.remove());

  for (const entry of entries) {
    const div = document.createElement("div");
    div.className = "log-entry";

    const levelClass =
      entry.level === "Success"
        ? "success"
        : entry.level === "Error"
          ? "error"
          : entry.level === "Warning"
            ? "warning"
            : "";

    div.innerHTML = `
      <span class="log-time">[${entry.timestamp}]</span>
      <span class="log-msg ${levelClass}">${escapeHtml(entry.message)}</span>
    `;
    logScroll.appendChild(div);
  }

  logScroll.scrollTop = logScroll.scrollHeight;
}

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function setSyncing(active: boolean) {
  syncing = active;
  btnSync.disabled = active;
  btnSync.textContent = active ? "Syncing..." : "Sync Now";
  if (active) {
    statusDot.className = "status-dot syncing";
  }
}

function updateWatcherBadge(running: boolean) {
  watcherBadge.className = `badge ${running ? "badge-active" : "badge-inactive"}`;
  watcherBadge.textContent = running ? "Watching" : "Watcher Off";
}

// ============================================================
// Event Handlers
// ============================================================

btnSetToken.addEventListener("click", async () => {
  const token = tokenInput.value.trim();
  if (!token) return;
  if (!token.startsWith("ct_")) {
    alert("Invalid token. Must start with 'ct_'.");
    return;
  }

  btnSetToken.textContent = "Connecting...";
  try {
    const status = await api.setApiToken(token);
    showTokenSet(token);
    updateConnectionUI(status.connected, status.user_name);
    updateStatusDetails(
      status.character_count,
      status.last_sync_time,
      status.last_sync_result
    );
  } catch (e) {
    alert(`Connection failed: ${e}`);
  } finally {
    btnSetToken.textContent = "Connect";
  }

  await refreshLogs();
});

btnClearToken.addEventListener("click", async () => {
  showTokenEmpty();
  updateConnectionUI(false);
  statusDetails.style.display = "none";

  const settings = await api.getSettings();
  settings.api_token = null;
  await api.saveSettings(settings);
});

btnBrowse.addEventListener("click", async () => {
  try {
    const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
    const selected = await openDialog({
      directory: true,
      title: "Select World of Warcraft folder",
    });
    if (selected) {
      const path = typeof selected === "string" ? selected : (selected as { path: string }).path;
      wowPath.value = path;
      const accounts = await api.setWowPath(path);
      populateAccounts(accounts);
      await refreshLogs();
    }
  } catch (e) {
    console.error("Browse error:", e);
  }
});

accountSelect.addEventListener("change", async () => {
  const settings = await api.getSettings();
  settings.wow_account = accountSelect.value;
  await api.saveSettings(settings);
});

toggleAutoSync.addEventListener("change", async () => {
  const settings = await api.getSettings();
  settings.auto_sync = toggleAutoSync.checked;
  await api.saveSettings(settings);
});

toggleAutoStart.addEventListener("change", async () => {
  const settings = await api.getSettings();
  settings.auto_start = toggleAutoStart.checked;
  await api.saveSettings(settings);
});

btnSync.addEventListener("click", async () => {
  if (syncing) return;
  setSyncing(true);
  try {
    await api.syncNow();
  } catch (e) {
    console.error("Sync error:", e);
  } finally {
    setSyncing(false);
    await refreshAll();
  }
});

btnClearLog.addEventListener("click", async () => {
  await api.clearLog();
  renderLogs([]);
});

linkWebsite.addEventListener("click", (e) => {
  e.preventDefault();
  open("https://goldschweine.de/dashboard/settings");
});

tokenInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    btnSetToken.click();
  }
});

// ============================================================
// Data Refresh
// ============================================================

async function refreshLogs() {
  const entries = await api.getLogEntries();
  renderLogs(entries);
}

async function refreshAll() {
  try {
    const settings = await api.getSettings();

    // Token UI
    if (settings.api_token) {
      showTokenSet(settings.api_token);
    } else {
      showTokenEmpty();
    }

    // WoW path
    if (settings.wow_path) {
      wowPath.value = settings.wow_path;
    }

    // Toggles
    toggleAutoSync.checked = settings.auto_sync;
    toggleAutoStart.checked = settings.auto_start;

    // Connection status
    const conn = await api.getConnectionStatus();
    updateConnectionUI(conn.connected, conn.user_name);
    updateStatusDetails(
      conn.character_count,
      conn.last_sync_time,
      conn.last_sync_result
    );

    // Watcher
    const watcherRunning = await api.getWatcherStatus();
    updateWatcherBadge(watcherRunning);

    // Logs
    await refreshLogs();
  } catch (e) {
    console.error("Refresh error:", e);
  }
}

// ============================================================
// Tauri Events
// ============================================================

listen("sync-complete", () => {
  setSyncing(false);
  refreshAll();
});

// ============================================================
// Init
// ============================================================

refreshAll();

// Refresh periodically (every 30 seconds) for "time ago" updates
setInterval(() => {
  refreshAll();
}, 30_000);
