import { toaster } from "@decky/api";
import { Navigation } from "@decky/ui";
import type { LaunchInfo, LibraryGame } from "../types";
import { buildInverseAppidMap, loadAppidMap, rememberAppid } from "./appid-map";
import { getSteamLaunchInfo } from "./server";
import {
  addToSpoolCollection,
  appStillExists,
  applyArtwork,
  resolveSteamGameId,
  runSteamGame,
  steamApps,
} from "./steam";

// Returns the Spool game matching a Steam appid, or null if not found.
// Checks two sources:
//   1. game.steam_id matches — native Steam game Spool also tracks
//   2. localStorage inverse map — non-Steam shortcut created via Spool
export function findSpoolGame(games: LibraryGame[], appid: number): LibraryGame | null {
  // 1. Native Steam game Spool also tracks (steam_id from SteamGridDB lookup).
  const direct = games.find((g) => g.steam_id != null && g.steam_id === appid);
  if (direct) return direct;
  // 2. Non-Steam shortcut created via desktop-mode "Add to Steam" — appid
  //    computed server-side with the same CRC formula Steam uses.
  const byShortcut = games.find((g) => g.shortcut_app_id != null && g.shortcut_app_id === appid);
  if (byShortcut) return byShortcut;
  // 3. Non-Steam shortcut created via Decky launchLibraryGame — appid stored
  //    in localStorage when Steam returned it from AddShortcut.
  const inverseMap = buildInverseAppidMap();
  const gameId = inverseMap[appid];
  if (gameId) return games.find((g) => g.id === gameId) ?? null;
  return null;
}

// The Steam appid of an existing non-Steam shortcut for this game, or null if
// Steam doesn't currently know one. Checks the localStorage map first (set when
// Spool created the shortcut), then the server-computed CRC `shortcut_app_id`,
// confirming each still exists in Steam (the user may have removed it).
export function existingShortcutAppId(game: LibraryGame): number | null {
  const stored = loadAppidMap()[game.id];
  if (stored != null && appStillExists(stored)) return stored;
  if (game.shortcut_app_id != null && appStillExists(game.shortcut_app_id)) {
    return game.shortcut_app_id;
  }
  return null;
}

// Ensure a Spool game has a live non-Steam shortcut (created via
// SteamClient.Apps — no Steam restart needed), reusing an existing one or
// creating it, then reinforce its fields and library artwork. Returns the
// resolved appid + launch info, or null if it couldn't be created. Shared by
// the launch flow and the QAM "Add to Steam" list.
async function ensureShortcut(
  base: string,
  gameId: string,
  shortcutAppId: number | null,
): Promise<{ appid: number; info: LaunchInfo } | null> {
  const apps = steamApps();
  if (!apps?.AddShortcut) {
    toaster.toast({ title: "Spool", body: "Adding to Steam needs Game Mode." });
    return null;
  }

  let info: LaunchInfo;
  try {
    info = await getSteamLaunchInfo(base, gameId);
  } catch (e) {
    console.error("[Spool] steam-launch-info fetch failed", e);
    toaster.toast({ title: "Spool", body: "Couldn't prepare the shortcut." });
    return null;
  }
  console.log("[Spool] ensureShortcut", { gameId, shortcutAppId, info });

  // Steam stores a shortcut's exe and start-dir *quoted*. Passing them quoted
  // (matching NonSteamLaunchers, and Spool's server-side CRC) keeps Steam's
  // returned appid in sync with `shortcut_app_id` and avoids the "browse button
  // exe has arguments" mis-parse that blanked Game Mode.
  const quote = (s: string) => `"${s.replace(/"/g, '\\"')}"`;
  const exeQ = quote(info.exe);
  const dirQ = quote(info.startDir);

  let appid: number | undefined = loadAppidMap()[gameId];
  if (appid != null && !appStillExists(appid)) {
    console.log(`[Spool] stored appid ${appid} no longer known to Steam; discarding`);
    appid = undefined;
  }
  // If localStorage is stale or was cleared, fall back to the server-computed
  // CRC id — built over the same quoted exe + name, so it matches the appid
  // Steam assigns for this shortcut.
  if (appid == null && shortcutAppId != null && appStillExists(shortcutAppId)) {
    console.log(`[Spool] reusing server shortcut_app_id ${shortcutAppId}`);
    appid = shortcutAppId;
    rememberAppid(gameId, appid);
  }

  if (appid == null) {
    toaster.toast({ title: "Spool", body: `Adding ${info.appName} to Steam…` });
    try {
      appid = await apps.AddShortcut(info.appName, exeQ, dirQ, info.launchOptions);
      console.log(`[Spool] AddShortcut -> appid=${appid}`);
    } catch (e) {
      console.error("[Spool] AddShortcut failed", e);
      toaster.toast({ title: "Spool", body: "Couldn't add to Steam." });
      return null;
    }
    rememberAppid(gameId, appid);
  }

  if (appid == null) {
    console.error("[Spool] could not resolve a Steam appid");
    toaster.toast({ title: "Spool", body: "Couldn't resolve Steam shortcut." });
    return null;
  }

  // Reinforce every field via the explicit setters for both new and reused
  // appids. SetAppLaunchOptions is the one that actually sticks — without it
  // the launcher runs with no args even on a reused shortcut.
  try { apps.SetShortcutName?.(appid, info.appName); } catch (e) { console.warn("[Spool] SetShortcutName", e); }
  try { apps.SetShortcutExe?.(appid, exeQ); } catch (e) { console.warn("[Spool] SetShortcutExe", e); }
  try { apps.SetShortcutStartDir?.(appid, dirQ); } catch (e) { console.warn("[Spool] SetShortcutStartDir", e); }
  try { apps.SetAppLaunchOptions?.(appid, info.launchOptions); } catch (e) { console.warn("[Spool] SetAppLaunchOptions", e); }
  try { apps.SetShortcutLaunchOptions?.(appid, info.launchOptions); } catch { /* fallback, may not exist */ }
  // Set library artwork live (portrait/hero/logo/wide). Fired in the background
  // so a slow or stalled loopback server cannot delay the caller.
  void applyArtwork(base, gameId, appid, apps);
  // Keep the "Spool" library collection in sync, mirroring desktop "Add to
  // Steam". Background + best-effort — never blocks adding/launching.
  void addToSpoolCollection(appid);

  return { appid, info };
}

// Add a Spool game to Steam as a non-Steam shortcut without launching it. Used
// by the QAM list so its details show on the Steam game page. Returns the new
// appid, or null on failure.
export async function addLibraryGameShortcut(
  base: string,
  gameId: string,
  shortcutAppId: number | null = null,
): Promise<number | null> {
  const r = await ensureShortcut(base, gameId, shortcutAppId);
  if (!r) return null;
  toaster.toast({ title: "Spool", body: `Added ${r.info.appName} to Steam ✓` });
  return r.appid;
}

// Launch a local-library game in Game Mode: ensure it's a non-Steam shortcut
// then ask Steam to run it. Steam runs `spool --run "Name" "Exe"`, which
// triggers the attached-launch workflow (restore -> play -> backup).
export async function launchLibraryGame(base: string, gameId: string, shortcutAppId: number | null = null) {
  const r = await ensureShortcut(base, gameId, shortcutAppId);
  if (!r) return;

  // Resolve the authoritative gameid from the app store (waits for Steam to
  // register the shortcut) before navigating to launch it.
  const gameid = await resolveSteamGameId(r.appid);
  toaster.toast({ title: "Spool", body: `Launching ${r.info.appName}…` });
  try {
    const method = runSteamGame(gameid);
    console.log(`[Spool] launched rungameid/${gameid} via ${method}`);
  } catch (e) {
    console.error("[Spool] launch failed", e);
    toaster.toast({ title: "Spool", body: "Couldn't start the game." });
    return;
  }
  Navigation.CloseSideMenus();
}
