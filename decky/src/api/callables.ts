import { callable } from "@decky/api";
import type { ProtonVersion, SaveRevision, Settings } from "../types";

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

// Pulls a game's latest cloud saves down to this device and restores them to
// disk, without launching ("Sync now"). Pull-only — never uploads. `outcome`
// is "pulled" (cloud was ahead, now applied), "up_to_date", "local_newer"
// (local is ahead, left alone), or "unconfigured" (no cloud remote). `ok` is
// false (with a `reason`) when the server is down or a true local-vs-cloud
// divergence needs resolving in the desktop app.
export const pullCloudSaves = callable<
  [id: string],
  {
    ok: boolean;
    outcome?: "pulled" | "up_to_date" | "local_newer" | "unconfigured";
    game_count?: number;
    reason?: string;
  }
>("pull_cloud_saves");

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

// Deletes a game's install folder from disk and removes its library entry.
// `ok` is false (with a `reason`) when the server is unavailable or the
// game has no known install folder.
export const deleteGame = callable<
  [id: string],
  { ok: boolean; reason?: string }
>("delete_game");

// Installs Windows runtime deps (winetricks verbs, e.g. "vcrun2022 dotnet48")
// into a game's Proton prefix. Long-running — the caller shows a spinner and
// the Python side uses a generous timeout. `ok` is false (with a `reason`)
// when the server is unavailable, no UMU/GE Proton is set, or winetricks fails.
export const installDeps = callable<
  [game_id: string, verbs: string],
  { ok: boolean; message?: string; reason?: string }
>("install_deps");

// Lists the Proton builds installed on this machine (newest-first) for the
// per-game Proton picker. Empty when the server is down or none are installed.
export const listProtonVersions = callable<[], ProtonVersion[]>(
  "list_proton_versions",
);

// Pins a game's Proton version. Pass an empty string to clear the override
// (back to "auto", letting umu-run pick its default). `ok` is false (with a
// `reason`) when the server is unavailable or the game isn't in the library.
export const setProtonVersion = callable<
  [game_id: string, proton_version_path: string],
  { ok: boolean; reason?: string }
>("set_proton_version");

// Lists the save revisions ludusavi retains locally for a game, newest-first,
// with the tip flagged (`is_current`). Backs the "restore an earlier save"
// picker. Reflects the local backup store, so cloud-only revisions this device
// hasn't pulled aren't listed. `ok` is false (with a `reason`) when the server
// is down or the game has no tracked saves.
// Backend: GET /games/<id>/revisions.
export const listSaveRevisions = callable<
  [game_id: string],
  { ok: boolean; revisions?: SaveRevision[]; reason?: string }
>("list_save_revisions");

// Rolls a game back to an earlier save revision and pins it as the new tip: it
// restores the chosen backup into the live save dir, then cloud-synced-backs-up
// so the rolled-back state becomes the latest revision (can't be clobbered by
// the next launch, and propagates to other devices). Destructive — replaces the
// current saves. Takes a few seconds (restore + backup + upload), so the caller
// shows a spinner. `ok` is false (with a `reason`) on failure.
// Backend: POST /games/<id>/restore  { backup_name }.
export const restoreSaveRevision = callable<
  [game_id: string, backup_name: string],
  { ok: boolean; game_count?: number; reason?: string }
>("restore_save_revision");
