import { Navigation, ReactRouter } from "@decky/ui";
import type { SteamApps } from "../types";
import { sleep } from "./format";

// Extracts params from Steam's internal React Router (memory-based, not
// window.location). Same pattern as OMGDuke/protondb-decky.
export const useParams = Object.values(ReactRouter).find((val) =>
  /return (\w)\?\1\.params:{}/.test(`${val}`)
) as <T>() => T;

export function steamApps(): SteamApps | undefined {
  return (SteamClient as unknown as { Apps?: SteamApps }).Apps;
}

// Steam's ELibraryAssetType. We set the four that matter for a polished tile
// (icon is noisy/optional for non-Steam shortcuts, so we skip it).
const STEAM_ASSET: Record<string, number> = {
  capsule: 0, // portrait tile
  hero: 1, // banner behind the page
  logo: 2, // transparent title logo
  header: 3, // wide capsule
};

// Pull each art kind from the backend (base64) and apply it live. Best-effort:
// any kind the backend 404s (no SteamGridDB art, etc.) is silently skipped, and
// art failures never block the launch.
export async function applyArtwork(base: string, gameId: string, appid: number, apps: SteamApps) {
  if (!apps.SetCustomArtworkForApp) return;
  for (const [kind, assetType] of Object.entries(STEAM_ASSET)) {
    try {
      const res = await fetch(`${base}/games/${gameId}/steam-art/${kind}`);
      if (!res.ok) continue;
      const { imageType, base64 } = (await res.json()) as {
        imageType: string;
        base64: string;
      };
      await apps.SetCustomArtworkForApp(appid, base64, imageType, assetType);
    } catch {
      /* best-effort per asset */
    }
  }
}

// Best-effort: does Steam still know this appid? (The user may have removed the
// shortcut.) If we can't tell, assume yes and let the launch attempt proceed.
export function appStillExists(appid: number): boolean {
  try {
    const store = (
      window as unknown as { appStore?: { GetAppOverviewByAppID?: (id: number) => unknown } }
    ).appStore;
    return !store?.GetAppOverviewByAppID ? true : !!store.GetAppOverviewByAppID(appid);
  } catch {
    return true;
  }
}

// Steam's gameid for a non-Steam shortcut: (appid << 32) | 0x02000000.
function shortcutGameId(appid: number): string {
  return ((BigInt(appid) << 32n) | 0x02000000n).toString();
}

// Steam's in-memory app store. `m_mapApps` maps appid -> overview, whose
// `m_gameid` is the authoritative launch id (a string). Reading it also
// confirms Steam has registered the shortcut.
function appStore():
  | { m_mapApps?: { get?(id: number): { m_gameid?: string | number } | undefined } }
  | undefined {
  return (window as unknown as { appStore?: ReturnType<typeof appStore> }).appStore;
}

// Resolve the gameid Steam assigned to a freshly-created shortcut. Mirrors the
// NonSteamLaunchers approach: read `appStore.m_mapApps.get(appid).m_gameid`
// rather than computing the bit-shift, polling briefly for the shortcut to
// register. Falls back to the computed id if the store never surfaces it.
export async function resolveSteamGameId(appid: number): Promise<string> {
  for (let i = 0; i < 25; i++) {
    const details = appStore()?.m_mapApps?.get?.(appid);
    if (details?.m_gameid != null) {
      console.log(`[Spool] resolved m_gameid=${details.m_gameid} for appid=${appid} (try ${i})`);
      return String(details.m_gameid);
    }
    await sleep(100);
  }
  const computed = shortcutGameId(appid);
  console.warn(`[Spool] m_gameid never appeared for appid=${appid}; using computed ${computed}`);
  return computed;
}

// Actually trigger the launch of a registered shortcut by its gameid. Tries
// the in-UI APIs in order of reliability:
//   1. SteamClient.Apps.RunGame — the canonical programmatic launch.
//   2. SteamClient.URL.ExecuteSteamURL — runs the steam:// protocol handler.
//   3. Navigation.Navigate — last resort; mostly drives the SPA router, which
//      is why it silently did nothing before.
// Returns the method that was used (for logging).
export function runSteamGame(gameid: string): string {
  const client = SteamClient as unknown as {
    Apps?: { RunGame?: (g: string, s: string, a: number, b: number) => void };
    URL?: { ExecuteSteamURL?: (url: string) => void };
  };
  if (typeof client.Apps?.RunGame === "function") {
    client.Apps.RunGame(gameid, "", -1, 100);
    return "Apps.RunGame";
  }
  if (typeof client.URL?.ExecuteSteamURL === "function") {
    client.URL.ExecuteSteamURL(`steam://rungameid/${gameid}`);
    return "URL.ExecuteSteamURL";
  }
  Navigation.Navigate(`steam://rungameid/${gameid}`);
  return "Navigation.Navigate";
}
