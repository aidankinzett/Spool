import { callable } from "@decky/api";
import type { Settings } from "../types";

// `SteamClient` (incl. GameSessions.RegisterForAppLifetimeNotifications and the
// LifetimeNotification payload) is provided as an ambient global by @decky/ui.
// The stop callback's `n` has `unAppID` (CRC app id — for Spool's non-Steam
// shortcuts it equals the `steam_appid` in active-session.json) and `bRunning`
// (false on a stop event).

export const onAppStop = callable<[appid: number], { acted: boolean; game?: string }>(
  "on_app_stop",
);
export const backupNow = callable<
  [],
  { acted: boolean; ok: boolean; game?: string; reason?: string }
>("backup_now");
export const getStatus = callable<
  [],
  { hasSession: boolean; game?: string; backedUp?: boolean; startedAt?: string }
>("get_status");

export const getSettings = callable<[], Settings>("get_settings");
export const setSettings = callable<
  [spool_command: string, notify: boolean],
  Settings
>("set_settings");

// Hands the UI the headless server's loopback base URL (e.g.
// "http://127.0.0.1:47650") so it can fetch /library and <img>-load /covers/*
// directly. `baseUrl` is null when the server isn't running.
export const getServerBase = callable<[], { baseUrl: string | null }>(
  "get_server_base",
);
