import {
  callable,
  definePlugin,
  toaster,
  routerHook,
  addEventListener,
  removeEventListener,
} from "@decky/api";
import {
  ButtonItem,
  Focusable,
  Navigation,
  PanelSection,
  PanelSectionRow,
  TextField,
  ToggleField,
  staticClasses,
  afterPatch,
  findInReactTree,
  appDetailsClasses,
  createReactTreePatcher,
  ReactRouter,
} from "@decky/ui";
import { useEffect, useRef, useState, createElement, type ReactElement } from "react";
import { FaFloppyDisk } from "react-icons/fa6";

// Extracts params from Steam's internal React Router (memory-based, not
// window.location). Same pattern as OMGDuke/protondb-decky.
const useParams = Object.values(ReactRouter).find((val) =>
  /return (\w)\?\1\.params:{}/.test(`${val}`)
) as <T>() => T;

// Full-screen library route registered via routerHook. The QAM "Browse
// Library" button navigates here.
const SPOOL_ROUTE = "/spool";

// `SteamClient` (incl. GameSessions.RegisterForAppLifetimeNotifications and the
// LifetimeNotification payload) is provided as an ambient global by @decky/ui.
// The stop callback's `n` has `unAppID` (CRC app id — for Spool's non-Steam
// shortcuts it equals the `steam_appid` in active-session.json) and `bRunning`
// (false on a stop event).

const onAppStop = callable<[appid: number], { acted: boolean; game?: string }>(
  "on_app_stop",
);
const backupNow = callable<
  [],
  { acted: boolean; ok: boolean; game?: string; reason?: string }
>("backup_now");
const getStatus = callable<
  [],
  { hasSession: boolean; game?: string; backedUp?: boolean; startedAt?: string }
>("get_status");

interface Settings {
  spool_command: string;
  notify: boolean;
}
const getSettings = callable<[], Settings>("get_settings");
const setSettings = callable<
  [spool_command: string, notify: boolean],
  Settings
>("set_settings");

// Hands the UI the headless server's loopback base URL (e.g.
// "http://127.0.0.1:47650") so it can fetch /library and <img>-load /covers/*
// directly. `baseUrl` is null when the server isn't running.
const getServerBase = callable<[], { baseUrl: string | null }>(
  "get_server_base",
);

// Mirror of the fields the grid needs from the Rust `GameEntry`.
interface LibraryGame {
  id: string;
  game_name: string;
  cover_image_path: string | null;
  accent_color: string | null;
  steam_id: number | null;
  playtime_minutes: number;
  shortcut_app_id: number | null;
  last_played_at: string | null;
  sync_badge: string | null;
}

// Shortcut fields from the backend (mirrors what desktop "Add to Steam" writes).
interface LaunchInfo {
  appName: string;
  exe: string;
  startDir: string;
  launchOptions: string;
}

// The subset of the live `SteamClient.Apps` API we use to create + launch a
// non-Steam shortcut without restarting Steam. AddShortcut returns the new
// appid; the setters are defensive (some Steam builds ignore AddShortcut's
// extra args).
interface SteamApps {
  // Order is (appName, exe, startDir, launchOptions) — matches the
  // NonSteamLaunchers plugin's working createShortcut. The exe and startDir
  // must be passed *quoted* (literal surrounding double-quotes), which is also
  // how Spool's server computes `shortcut_app_id` (steam.rs compute_shortcut_app_id
  // CRCs the quoted exe) — so Steam's returned appid matches the server's.
  AddShortcut(appName: string, exe: string, startDir: string, launchOptions: string): Promise<number>;
  SetShortcutName?(appId: number, name: string): void;
  SetShortcutExe?(appId: number, path: string): void;
  SetShortcutStartDir?(appId: number, dir: string): void;
  // NSL uses SetAppLaunchOptions; SetShortcutLaunchOptions appears to be a
  // no-op on current Steam builds (left launch options empty → spool-launcher.sh
  // ran with no `--run` args → nothing launched).
  SetAppLaunchOptions?(appId: number, opts: string): void;
  SetShortcutLaunchOptions?(appId: number, opts: string): void;
  RemoveShortcut?(appId: number): Promise<void> | void;
  // Programmatic launch. gameId is the string gameid (the big number);
  // signature used across Decky plugins: (gameId, "", -1, 100).
  RunGame?(gameId: string, launchSource: string, a: number, b: number): void;
  // SetCustomArtworkForApp(appId, base64, 'png'|'jpg', assetType): the live,
  // no-restart way to set Steam library art. assetType is ELibraryAssetType.
  SetCustomArtworkForApp?(
    appId: number,
    base64: string,
    imageType: string,
    assetType: number,
  ): Promise<void>;
}
function steamApps(): SteamApps | undefined {
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
async function applyArtwork(base: string, gameId: string, appid: number, apps: SteamApps) {
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

// Persist game_id -> Steam appid so a game added to Steam once isn't re-added
// (which would duplicate the shortcut) on later launches. Lives in the CEF
// web-context localStorage (steamloopback.host origin).
const APPID_MAP_KEY = "spool:steamAppids";
function loadAppidMap(): Record<string, number> {
  try {
    return JSON.parse(localStorage.getItem(APPID_MAP_KEY) || "{}");
  } catch {
    return {};
  }
}
function rememberAppid(gameId: string, appid: number) {
  const map = loadAppidMap();
  map[gameId] = appid;
  localStorage.setItem(APPID_MAP_KEY, JSON.stringify(map));
}

// Reverse of loadAppidMap: maps steam_appid (non-Steam shortcut CRC id) -> spool game_id.
function buildInverseAppidMap(): Record<number, string> {
  const map = loadAppidMap();
  return Object.fromEntries(
    Object.entries(map).map(([gameId, appid]) => [appid, gameId])
  );
}

function formatPlaytime(minutes: number): string {
  if (minutes < 60) return `${minutes}m`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m > 0 ? `${h}h ${m}m` : `${h}h`;
}

function formatRelativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 60) return "just now";
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d ago`;
  const weeks = Math.floor(days / 7);
  if (weeks < 5) return `${weeks}w ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}

// Returns the Spool game matching a Steam appid, or null if not found.
// Checks two sources:
//   1. game.steam_id matches — native Steam game Spool also tracks
//   2. localStorage inverse map — non-Steam shortcut created via Spool
function findSpoolGame(games: LibraryGame[], appid: number): LibraryGame | null {
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

// Hook: fetch the library once and return the Spool playtime for the given Steam appid.
function useSpoolPlaytime(
  appid: number,
  base: string | null,
): { game: LibraryGame | null; loading: boolean } {
  const [game, setGame] = useState<LibraryGame | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!base || !appid) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        // Fetch current library immediately so the badge appears fast.
        const first = await fetch(`${base}/library`);
        if (!first.ok) throw new Error(`bad status ${first.status}`);
        const initial = (await first.json()) as LibraryGame[];
        if (!cancelled) setGame(findSpoolGame(initial, appid));

        // Trigger a cross-device fold in the background, then refresh.
        await fetch(`${base}/fold`, { method: "POST" }).catch(() => undefined);
        if (cancelled) return;

        const second = await fetch(`${base}/library`);
        if (!second.ok) throw new Error(`bad status ${second.status}`);
        const fresh = (await second.json()) as LibraryGame[];
        if (!cancelled) {
          setGame(findSpoolGame(fresh, appid));
          setLoading(false);
        }
      } catch {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [appid, base]);

  return { game, loading };
}

// Badge injected on the Steam /library/app/:appid page when Spool has a match.
function SpoolPlaytimeBadge({
  appid,
  base,
}: {
  appid: number;
  base: string | null;
}) {
  const { game, loading } = useSpoolPlaytime(appid, base);

  if (loading || !game || game.playtime_minutes <= 0) return null;
  const lastPlayed = game.last_played_at ? formatRelativeTime(game.last_played_at) : null;

  const sep = <span style={{ opacity: 0.3, margin: "0 0.3rem" }}>·</span>;

  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        padding: "0.3rem 0.75rem",
        borderRadius: "4px",
        background: "rgba(255,255,255,0.08)",
        fontSize: "0.8rem",
        fontWeight: 600,
        marginBottom: "0.5rem",
      }}
    >
      <span style={{ opacity: 0.6, marginRight: "0.4rem" }}>💾</span>
      {formatPlaytime(game.playtime_minutes)} played
      {lastPlayed && <>{sep}Last played {lastPlayed}</>}
    </div>
  );
}

// Best-effort: does Steam still know this appid? (The user may have removed the
// shortcut.) If we can't tell, assume yes and let the launch attempt proceed.
function appStillExists(appid: number): boolean {
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

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

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
async function resolveSteamGameId(appid: number): Promise<string> {
  const store = appStore();
  for (let i = 0; i < 25; i++) {
    const details = store?.m_mapApps?.get?.(appid);
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
function runSteamGame(gameid: string): string {
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

// Launch a local-library game in Game Mode: ensure it's a non-Steam shortcut
// (created live via SteamClient.Apps — no Steam restart needed) then ask Steam
// to run it. Steam runs `spool --run "Name" "Exe"`, which triggers the existing
// attached-launch workflow (restore -> play -> backup).
async function launchLibraryGame(base: string, gameId: string, shortcutAppId: number | null = null) {
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

// Mirror of the Rust `LanPeer` (lan/discovery.rs).
interface LanPeer {
  device_id: string;
  device_name: string;
  addr: string;
  game_count: number;
  file_server_port: number;
  last_seen_ago_secs: number;
}

// Mirror of the Rust `PeerGame` (lan/server.rs) — only the fields the grid uses.
interface PeerGame {
  id: string;
  game_name: string;
}

// Mirror of the Rust `DownloadProgress` (lan/install.rs).
interface DownloadProgress {
  install_token: string;
  source_device_name: string;
  game_name: string;
  bytes_done: number;
  bytes_total: number;
  current_file: string;
  status: "starting" | "transferring" | "done" | "error" | "canceled";
  message?: string;
  new_game_id?: string;
  bytes_per_second: number;
}

// Resolve the headless server base URL once. The whole full-screen UI talks
// to the server over loopback HTTP directly (not the Decky callable bridge):
// `http://127.0.0.1` is a secure origin, so `<img>` covers aren't blocked as
// mixed content from the https://steamloopback.host page.
function useServerBase(): { base: string | null; error: string | null } {
  const [base, setBase] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const { baseUrl } = await getServerBase();
      if (cancelled) return;
      if (baseUrl) setBase(baseUrl);
      else setError("Spool isn’t running. Launch Spool, then try again.");
    })();
    return () => {
      cancelled = true;
    };
  }, []);
  return { base, error };
}

// Shared cover-tile grid used by both the local library and a peer's games.
// Tiles are focusable for controller nav; `onActivate` is optional (inert in
// the LAN browse view for now — installing lands in a later phase).
interface Tile {
  key: string;
  name: string;
  coverUrl: string | null;
  accentColor?: string | null;
}
function CoverGrid({
  tiles,
  onActivate,
}: {
  tiles: Tile[];
  onActivate?: (key: string) => void;
}) {
  return (
    <Focusable
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))",
        gap: "1.25rem",
      }}
    >
      {tiles.map((t) => (
        <Focusable
          key={t.key}
          onActivate={() => onActivate?.(t.key)}
          style={{
            aspectRatio: "2 / 3",
            borderRadius: "8px",
            overflow: "hidden",
            position: "relative",
            display: "flex",
            alignItems: "flex-end",
            background: t.accentColor ?? "#1a2330",
          }}
        >
          {t.coverUrl ? (
            <img
              src={t.coverUrl}
              alt={t.name}
              style={{
                position: "absolute",
                inset: 0,
                width: "100%",
                height: "100%",
                objectFit: "cover",
              }}
            />
          ) : (
            <span
              style={{
                padding: "0.5rem",
                fontSize: "0.85rem",
                fontWeight: 600,
                textShadow: "0 1px 3px rgba(0,0,0,0.85)",
              }}
            >
              {t.name}
            </span>
          )}
        </Focusable>
      ))}
    </Focusable>
  );
}

// ── Local library grid ─────────────────────────────────────────────────────
function LibraryGrid({ base }: { base: string }) {
  const [games, setGames] = useState<LibraryGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${base}/library`);
        const data = (await res.json()) as LibraryGame[];
        if (!cancelled) setGames(data);
      } catch {
        if (!cancelled) setError("Couldn’t load your library.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [base]);

  const coverUrl = (g: LibraryGame): string | null => {
    if (!g.cover_image_path) return null;
    const file = g.cover_image_path.split(/[/\\]/).pop();
    return file ? `${base}/covers/${encodeURIComponent(file)}` : null;
  };

  if (error) return <div style={{ opacity: 0.8 }}>{error}</div>;
  if (!games) return <div style={{ opacity: 0.7 }}>Loading…</div>;
  if (games.length === 0)
    return <div style={{ opacity: 0.7 }}>No games in your library yet.</div>;

  return (
    <CoverGrid
      onActivate={(id) => {
        const g = games.find((g) => g.id === id);
        void launchLibraryGame(base, id, g?.shortcut_app_id ?? null);
      }}
      tiles={games.map((g) => ({
        key: g.id,
        name: g.game_name,
        coverUrl: coverUrl(g),
        accentColor: g.accent_color,
      }))}
    />
  );
}

// ── LAN browse ─────────────────────────────────────────────────────────────
//
// Peers and their games are fetched server-side: the UI can't hit a peer's
// non-loopback http directly (mixed content), so the headless server discovers
// peers and proxies their /games + covers. We poll /lan/peers (there's no push
// bus over plain HTTP). Selecting a peer shows its shared games as a grid;
// tiles are inert until the install phase.
function LanView({ base }: { base: string }) {
  const [peers, setPeers] = useState<LanPeer[] | null>(null);
  const [selected, setSelected] = useState<LanPeer | null>(null);

  useEffect(() => {
    let cancelled = false;
    const poll = async () => {
      try {
        const res = await fetch(`${base}/lan/peers`);
        const data = (await res.json()) as LanPeer[];
        if (!cancelled) setPeers(data);
      } catch {
        if (!cancelled) setPeers([]);
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 3000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [base]);

  if (selected)
    return <PeerGames base={base} peer={selected} onBack={() => setSelected(null)} />;

  if (!peers) return <div style={{ opacity: 0.7 }}>Scanning…</div>;
  if (peers.length === 0)
    return (
      <div style={{ opacity: 0.7 }}>No Spool devices found on your network.</div>
    );

  return (
    <Focusable style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
      {peers.map((p) => {
        const browsable = p.file_server_port !== 0;
        return (
          <Focusable
            key={p.device_id}
            onActivate={() => browsable && setSelected(p)}
            style={{
              padding: "1rem 1.25rem",
              borderRadius: "8px",
              background: "#1a2330",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              opacity: browsable ? 1 : 0.5,
            }}
          >
            <span style={{ fontWeight: 600 }}>{p.device_name || p.addr}</span>
            <span style={{ fontSize: "0.85rem", opacity: 0.8 }}>
              {browsable
                ? `${p.game_count} game${p.game_count === 1 ? "" : "s"}`
                : "sharing off"}
            </span>
          </Focusable>
        );
      })}
    </Focusable>
  );
}

// Formats bytes to a human-readable string (e.g. "1.2 GB", "450 MB").
function fmtBytes(n: number): string {
  if (n >= 1_073_741_824) return `${(n / 1_073_741_824).toFixed(1)} GB`;
  if (n >= 1_048_576) return `${(n / 1_048_576).toFixed(0)} MB`;
  if (n >= 1_024) return `${(n / 1_024).toFixed(0)} KB`;
  return `${n} B`;
}

// A selected peer’s shared games, fetched through the server-side proxy.
// Activating a tile kicks off a download; a progress row appears above the
// grid while the install is in flight.
function PeerGames({
  base,
  peer,
  onBack,
}: {
  base: string;
  peer: LanPeer;
  onBack: () => void;
}) {
  const [games, setGames] = useState<PeerGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const peerBase = `${base}/lan/peers/${peer.addr}/${peer.file_server_port}`;

  // Fetch the game list once on mount.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${peerBase}/games`);
        if (!res.ok) throw new Error();
        const data = (await res.json()) as PeerGame[];
        if (!cancelled) setGames(data);
      } catch {
        if (!cancelled) setError("Couldn’t reach this device.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [peerBase]);

  // On mount, pick up any in-flight download (e.g. navigated away and back).
  useEffect(() => {
    void fetch(`${base}/lan/download`)
      .then((r) => r.json() as Promise<DownloadProgress | null>)
      .then((p) => {
        if (p && p.status !== "done" && p.status !== "error" && p.status !== "canceled") {
          setDownload(p);
          startPolling();
        }
      })
      .catch(() => undefined);
    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function startPolling() {
    if (pollRef.current) return;
    pollRef.current = setInterval(() => {
      void fetch(`${base}/lan/download`)
        .then((r) => r.json() as Promise<DownloadProgress | null>)
        .then((p) => {
          setDownload(p);
          if (!p || p.status === "done" || p.status === "error" || p.status === "canceled") {
            if (pollRef.current) clearInterval(pollRef.current);
            pollRef.current = null;
            if (p?.status === "done") {
              toaster.toast({ title: "Install complete", body: p.game_name });
            } else if (p?.status === "error") {
              toaster.toast({ title: "Install failed", body: p.message ?? p.game_name });
            }
            // Clear the terminal state row after 3 s.
            setTimeout(() => setDownload(null), 3000);
          }
        })
        .catch(() => undefined);
    }, 500);
  }

  async function startDownload(gameId: string) {
    if (download && (download.status === "starting" || download.status === "transferring")) {
      return; // already in flight
    }
    try {
      const res = await fetch(`${base}/lan/install`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          peer_addr: peer.addr,
          peer_port: peer.file_server_port,
          game_id: gameId,
        }),
      });
      if (!res.ok) throw new Error("Server error");
      startPolling();
    } catch {
      toaster.toast({ title: "Install failed", body: "Couldn’t start download." });
    }
  }

  async function cancelDownload() {
    if (!download) return;
    await fetch(`${base}/lan/download`, {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ install_token: download.install_token }),
    }).catch(() => undefined);
  }

  const isActive =
    download !== null &&
    (download.status === "starting" || download.status === "transferring");
  const pct =
    download && download.bytes_total > 0
      ? Math.round((download.bytes_done / download.bytes_total) * 100)
      : 0;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
      <Focusable>
        <ButtonItem layout="below" onClick={onBack}>
          ← Back to devices
        </ButtonItem>
      </Focusable>
      <h2 style={{ margin: 0 }}>{peer.device_name || peer.addr}</h2>

      {/* Download progress row */}
      {download !== null && (
        <div
          style={{
            background: "#1a2330",
            borderRadius: "8px",
            padding: "0.75rem 1rem",
            display: "flex",
            flexDirection: "column",
            gap: "0.5rem",
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
            }}
          >
            <span style={{ fontWeight: 600 }}>
              {download.game_name || "Fetching manifest…"}
            </span>
            {isActive && (
              <Focusable>
                <ButtonItem layout="below" onClick={() => void cancelDownload()}>
                  Cancel
                </ButtonItem>
              </Focusable>
            )}
            {download.status === "done" && (
              <span style={{ color: "#4caf50" }}>Installed</span>
            )}
            {download.status === "canceled" && (
              <span style={{ opacity: 0.7 }}>Cancelled</span>
            )}
            {download.status === "error" && (
              <span style={{ color: "#f44336" }}>Failed</span>
            )}
          </div>

          {download.status === "error" && download.message && (
            <div style={{ fontSize: "0.8rem", opacity: 0.8 }}>{download.message}</div>
          )}

          {(download.status === "starting" || download.status === "transferring") && (
            <>
              <div
                style={{
                  height: "4px",
                  borderRadius: "2px",
                  background: "#2a3a52",
                  overflow: "hidden",
                }}
              >
                <div
                  style={{
                    height: "100%",
                    width: `${pct}%`,
                    background: "#4a90d9",
                    transition: "width 0.3s",
                  }}
                />
              </div>
              <div style={{ fontSize: "0.8rem", opacity: 0.7 }}>
                {download.bytes_total > 0
                  ? `${fmtBytes(download.bytes_done)} / ${fmtBytes(download.bytes_total)}  (${fmtBytes(Math.round(download.bytes_per_second))}/s)`
                  : download.current_file || "Starting…"}
              </div>
            </>
          )}
        </div>
      )}

      {error && <div style={{ opacity: 0.8 }}>{error}</div>}
      {!error && !games && <div style={{ opacity: 0.7 }}>Loading…</div>}
      {games && games.length === 0 && (
        <div style={{ opacity: 0.7 }}>This device isn’t sharing any games.</div>
      )}
      {games && games.length > 0 && (
        <CoverGrid
          onActivate={(id) => void startDownload(id)}
          tiles={games.map((g) => ({
            key: g.id,
            name: g.game_name,
            coverUrl: `${peerBase}/games/${g.id}/cover`,
          }))}
        />
      )}
    </div>
  );
}

// ── Full-screen page: Library | LAN toggle ─────────────────────────────────
function SpoolPage() {
  const { base, error } = useServerBase();
  const [view, setView] = useState<"library" | "lan">("library");

  const TabButton = ({ id, label }: { id: "library" | "lan"; label: string }) => (
    <Focusable
      onActivate={() => setView(id)}
      style={{
        padding: "0.5rem 1.25rem",
        borderRadius: "6px",
        fontWeight: 600,
        background: view === id ? "#2a3a52" : "transparent",
        opacity: view === id ? 1 : 0.7,
      }}
    >
      {label}
    </Focusable>
  );

  return (
    <div
      style={{
        height: "100%",
        overflowY: "scroll",
        padding: "2rem",
        boxSizing: "border-box",
      }}
    >
      <Focusable
        style={{
          display: "flex",
          gap: "0.5rem",
          alignItems: "center",
          marginBottom: "1.5rem",
        }}
      >
        <h1 style={{ margin: "0 1rem 0 0" }}>Spool</h1>
        <TabButton id="library" label="Library" />
        <TabButton id="lan" label="LAN" />
      </Focusable>

      {error && <div style={{ opacity: 0.8 }}>{error}</div>}
      {base && view === "library" && <LibraryGrid base={base} />}
      {base && view === "lan" && <LanView base={base} />}
    </div>
  );
}

function Content() {
  const [status, setStatus] = useState<Awaited<ReturnType<typeof getStatus>> | null>(
    null,
  );
  const [settings, setLocalSettings] = useState<Settings | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = async () => setStatus(await getStatus());
  useEffect(() => {
    void refresh();
    void getSettings().then(setLocalSettings);
  }, []);

  const save = async (patch: Partial<Settings>) => {
    const next = { ...(settings ?? { spool_command: "", notify: true }), ...patch };
    setLocalSettings(next);
    setLocalSettings(await setSettings(next.spool_command, next.notify));
  };

  return (
    <>
      <PanelSection title="Library">
        <PanelSectionRow>
          <ButtonItem
            layout="below"
            onClick={() => {
              Navigation.Navigate(SPOOL_ROUTE);
              Navigation.CloseSideMenus();
            }}
          >
            Browse Library
          </ButtonItem>
        </PanelSectionRow>
      </PanelSection>

      <PanelSection title="Spool">
        <PanelSectionRow>
          {status?.hasSession ? (
            <div style={{ fontSize: "0.8rem", opacity: 0.85 }}>
              Last session: <strong>{status.game}</strong>
              <br />
              {status.backedUp ? "Backed up ✓" : "Not yet backed up"}
            </div>
          ) : (
            <div style={{ fontSize: "0.8rem", opacity: 0.7 }}>
              No active Spool session recorded.
            </div>
          )}
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem
            layout="below"
            disabled={busy || !status?.hasSession}
            onClick={async () => {
              setBusy(true);
              if (status?.game) {
                toaster.toast({
                  title: "Spool",
                  body: `Backing up ${status.game}…`,
                });
              }
              const r = await backupNow();
              toaster.toast({
                title: "Spool",
                body: !r.acted
                  ? "Nothing to back up"
                  : r.ok
                    ? `Backed up ${r.game} ✓`
                    : `Backup failed: ${r.reason ?? "unknown error"}`,
              });
              setBusy(false);
              void refresh();
            }}
          >
            Back up now
          </ButtonItem>
        </PanelSectionRow>
      </PanelSection>

      <PanelSection title="Settings">
        <PanelSectionRow>
          <ToggleField
            label="Notify on backup"
            description="Show a toast when a backup finishes."
            checked={settings?.notify ?? true}
            onChange={(value) => void save({ notify: value })}
          />
        </PanelSectionRow>
        <PanelSectionRow>
          <TextField
            label="Spool command"
            description="Override the auto-detected spool / spool-launcher.sh path."
            value={settings?.spool_command ?? ""}
            onChange={(e) => void save({ spool_command: e.target.value })}
          />
        </PanelSectionRow>
      </PanelSection>
    </>
  );
}

// Badge wrapper injected into the game detail page's InnerContainer via
// afterPatch. Uses useParams to read appid from Steam's internal router —
// window.location.pathname is always '/index.html' in Steam's CEF context.
function PlaytimePatchWrapper() {
  const { base } = useServerBase();
  const { appid: appidStr } = useParams<{ appid: string }>();
  const appid = parseInt(appidStr ?? "0", 10);

  if (!appid) return null;

  return (
    <div style={{ padding: "0.5rem 0" }}>
      <SpoolPlaytimeBadge appid={appid} base={base} />
    </div>
  );
}

export default definePlugin(() => {
  // Register the full-screen route (Library | LAN). The QAM "Browse Library"
  // button navigates to it; we remove it on dismount to avoid duplicate
  // patches across hot-reloads.
  routerHook.addRoute(SPOOL_ROUTE, SpoolPage);

  // Patch the Steam game-detail page to inject Spool's cross-device playtime
  // badge. Uses afterPatch + findInReactTree to splice into the InnerContainer
  // of the rendered tree — same approach as OMGDuke/protondb-decky. Wrapping
  // props.children doesn't work because the game detail component ignores it.
  const playtimePatch = routerHook.addPatch(
    "/library/app/:appid",
    (tree: any) => {
      const routeProps = findInReactTree(tree, (x: any) => x?.renderFunc);
      if (!routeProps) return tree;
      const patchHandler = createReactTreePatcher(
        [
          (t: any) =>
            findInReactTree(t, (x: any) => x?.props?.children?.props?.overview)
              ?.props?.children,
        ],
        (_: Array<Record<string, unknown>>, ret?: ReactElement) => {
          const container = findInReactTree(
            ret,
            (x: any) =>
              Array.isArray(x?.props?.children) &&
              x?.props?.className?.includes(appDetailsClasses.InnerContainer),
          );
          if (typeof container !== "object") return ret;
          container.props.children.splice(1, 0, createElement(PlaytimePatchWrapper, null));
          return ret;
        },
      );
      afterPatch(routeProps, "renderFunc", patchHandler);
      return tree;
    },
  );

  // Register the game-stop listener ONCE at plugin load (not inside the panel,
  // which unmounts when the QAM closes). On a stop, let the backend decide
  // whether a forced-close fallback backup is needed.
  const sub = SteamClient.GameSessions.RegisterForAppLifetimeNotifications(
    (n) => {
      if (!n.bRunning) {
        // Spool's non-Steam shortcut appids are `crc32(...) | 0x80000000`, so
        // the high bit is set. Steam surfaces those through `unAppID` as a
        // *signed* int32 (e.g. -105595925 instead of 4189371371), which would
        // never match the unsigned `steam_appid` in active-session.json. `>>> 0`
        // coerces it back to the unsigned 32-bit value the backend compares.
        void onAppStop(n.unAppID >>> 0);
      }
    },
  );

  const onBackupFinished = (game: string, ok: boolean, reason: string) => {
    toaster.toast({
      title: "Spool",
      body: ok ? `Backed up ${game} ✓` : `Backup failed: ${reason || "unknown error"}`,
    });
  };
  addEventListener("spool_backup_finished", onBackupFinished);

  return {
    name: "Spool",
    titleView: <div className={staticClasses.Title}>Spool</div>,
    content: <Content />,
    icon: <FaFloppyDisk />,
    onDismount() {
      sub.unregister();
      removeEventListener("spool_backup_finished", onBackupFinished);
      routerHook.removeRoute(SPOOL_ROUTE);
      routerHook.removePatch("/library/app/:appid", playtimePatch);
    },
  };
});
