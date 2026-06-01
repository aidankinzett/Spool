import { toaster } from "@decky/api";
import { Navigation } from "@decky/ui";
import type { LaunchInfo, LibraryGame } from "../types";
import { buildInverseAppidMap, loadAppidMap, rememberAppid } from "./appid-map";
import {
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

// Launch a local-library game in Game Mode: ensure it's a non-Steam shortcut
// (created live via SteamClient.Apps — no Steam restart needed) then ask Steam
// to run it. Steam runs `spool --run "Name" "Exe"`, which triggers the existing
// attached-launch workflow (restore -> play -> backup).
export async function launchLibraryGame(base: string, gameId: string, shortcutAppId: number | null = null) {
  const apps = steamApps();
  if (!apps?.AddShortcut) {
    toaster.toast({ title: "Spool", body: "Launching needs Steam Game Mode." });
    return;
  }

  let info: LaunchInfo;
  try {
    const res = await fetch(`${base}/games/${gameId}/steam-launch-info`);
    if (!res.ok) throw new Error("bad status");
    info = (await res.json()) as LaunchInfo;
  } catch (e) {
    console.error("[Spool] steam-launch-info fetch failed", e);
    toaster.toast({ title: "Spool", body: "Couldn't prepare launch." });
    return;
  }
  console.log("[Spool] launchLibraryGame", { gameId, shortcutAppId, info });

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
      return;
    }
    // Reinforce every field via the explicit setters. SetAppLaunchOptions is
    // the one that actually sticks — without it the launcher runs with no args.
    try { apps.SetShortcutName?.(appid, info.appName); } catch (e) { console.warn("[Spool] SetShortcutName", e); }
    try { apps.SetShortcutExe?.(appid, exeQ); } catch (e) { console.warn("[Spool] SetShortcutExe", e); }
    try { apps.SetShortcutStartDir?.(appid, dirQ); } catch (e) { console.warn("[Spool] SetShortcutStartDir", e); }
    try { apps.SetAppLaunchOptions?.(appid, info.launchOptions); } catch (e) { console.warn("[Spool] SetAppLaunchOptions", e); }
    try { apps.SetShortcutLaunchOptions?.(appid, info.launchOptions); } catch { /* fallback, may not exist */ }
    rememberAppid(gameId, appid);
    // Set library artwork live (portrait/hero/logo/wide). Best-effort — never
    // blocks the launch.
    await applyArtwork(base, gameId, appid, apps);
  }

  if (appid == null) {
    console.error("[Spool] could not resolve a Steam appid");
    toaster.toast({ title: "Spool", body: "Couldn't resolve Steam shortcut." });
    return;
  }

  // Resolve the authoritative gameid from the app store (waits for Steam to
  // register the shortcut) before navigating to launch it.
  const gameid = await resolveSteamGameId(appid);
  toaster.toast({ title: "Spool", body: `Launching ${info.appName}…` });
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
