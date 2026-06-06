// Shared fixtures + callable/fetch mocking for the stories.
import type { DownloadProgress, LanPeer, LibraryGame, PeerGame, SaveRevision } from "../../src/types";
import { setCallable } from "./registry";

export const MOCK_BASE = "http://mock.spool";

export function makeGame(overrides: Partial<LibraryGame> = {}): LibraryGame {
  return {
    id: "game-1",
    game_name: "Hollow Knight",
    exe_path: "C:/Games/HollowKnight/hollow_knight.exe",
    cover_image_path: null,
    accent_color: "#2b3a55",
    steam_id: 367520,
    playtime_minutes: 742,
    shortcut_app_id: 3000123456,
    last_played_at: "2026-06-04T18:00:00Z",
    sync_badge: "synced",
    game_folder_path: "C:/Games/HollowKnight",
    save_backup_count: 5,
    save_last_backed_up_at: "2026-06-05T09:30:00Z",
    save_backup_size_mb: 12,
    save_last_backer_device: null,
    save_cloud_revision_at: null,
    proton_version_path: null,
    ...overrides,
  };
}

export const PEERS: LanPeer[] = [
  { device_id: "d1", device_name: "Steam-Deck", addr: "192.168.1.20", game_count: 7, file_server_port: 47632, last_seen_ago_secs: 2 },
  { device_id: "d2", device_name: "Desktop-PC", addr: "192.168.1.31", game_count: 23, file_server_port: 47632, last_seen_ago_secs: 5 },
  { device_id: "d3", device_name: "Ally-X", addr: "192.168.1.44", game_count: 0, file_server_port: 0, last_seen_ago_secs: 8 },
];

export const PEER_GAMES: PeerGame[] = [
  { id: "pg1", game_name: "Hades", install_size_mb: 14_000, shareable: true },
  { id: "pg2", game_name: "Celeste", install_size_mb: 1_300, shareable: true },
  { id: "pg3", game_name: "Stardew Valley", install_size_mb: 980, shareable: true },
  { id: "pg4", game_name: "Baldur's Gate 3", install_size_mb: 122_000, shareable: false },
];

// Cover art for the LAN stories, loaded straight from Steam's public CDN (keyed
// by Steam app id) rather than bundled into the repo — the art stays © its
// publishers and never lands in git history, only in whatever documentation
// screenshot is captured from the rendered story. Mirrors the desktop fixtures'
// `cover()`/`hero()` helpers.
const STEAM_CDN = "https://steamcdn-a.akamaihd.net/steam/apps";
export const cover = (appid: number): string => `${STEAM_CDN}/${appid}/library_600x900_2x.jpg`;
export const hero = (appid: number): string => `${STEAM_CDN}/${appid}/library_hero.jpg`;

// Steam app ids for the sample peer games, so the LAN game list shows real
// portrait cover art in stories. Every PEER_GAMES id is mapped — each row builds
// a cover URL unconditionally, so an unmapped id would render a broken-image
// icon.
export const PEER_COVER_APPIDS: Record<string, number> = {
  pg1: 1145360, // Hades
  pg2: 504230, // Celeste
  pg3: 413150, // Stardew Valley
  pg4: 1086940, // Baldur's Gate 3
};

export function makeDownload(overrides: Partial<DownloadProgress> = {}): DownloadProgress {
  return {
    install_token: "tok-1",
    source_device_name: "Desktop-PC",
    source_game_id: "pg1", // Hades, in PEER_GAMES
    game_name: "Hades",
    bytes_done: 6_300_000_000,
    bytes_total: 14_000_000_000,
    current_file: "Hades/Content/Audio.pck",
    status: "transferring",
    bytes_per_second: 78_000_000,
    ...overrides,
  };
}

// Register benign handlers for every callable the stories' components fire.
export function registerDeckyCallables(opts: { serverRunning?: boolean } = {}): void {
  const { serverRunning = true } = opts;
  setCallable("get_server_base", async () => ({ baseUrl: serverRunning ? MOCK_BASE : null }));
  setCallable("get_status", async () => ({ hasSession: true, game: "Hollow Knight", backedUp: true }));
  setCallable("get_settings", async () => ({ spool_command: "", notify: true }));
  setCallable("set_settings", async (cmd: string, notify: boolean) => ({ spool_command: cmd, notify }));
  setCallable("backup_now", async () => ({ acted: true, ok: true, game: "Hollow Knight" }));
  setCallable("pull_cloud_saves", async () => ({ ok: true, outcome: "up_to_date" }));
  setCallable("delete_game", async () => ({ ok: true }));
  setCallable("install_deps", async () => ({ ok: true, message: "done" }));
  setCallable("list_proton_versions", async () => [
    { name: "GE-Proton9-20", path: "/p/ge", source: "GE-Proton" },
    { name: "UMU-Proton-9.0", path: "/p/umu", source: "UMU-Proton" },
  ]);
  setCallable("set_proton_version", async () => ({ ok: true }));
  setCallable("list_save_revisions", async () => ({ ok: true, revisions: SAMPLE_REVISIONS }));
  setCallable("restore_save_revision", async () => ({ ok: true, game_count: 1 }));
}

// A few retained save revisions (newest-first, tip flagged) for the restore
// picker. Timestamps are relative to the real clock so the "ago" labels read
// naturally whenever a story is opened.
export const SAMPLE_REVISIONS: SaveRevision[] = [
  { name: ".", when: new Date(Date.now() - 2 * 3_600_000).toISOString(), is_current: true },
  { name: "20260605T093000", when: new Date(Date.now() - 26 * 3_600_000).toISOString(), is_current: false },
  { name: "20260601T193000", when: new Date(Date.now() - 5 * 86_400_000).toISOString(), is_current: false },
];

// Install a window.fetch stub for stories whose components hit the loopback
// server. `routes` maps a URL substring to a JSON body (or a function of the
// URL); unmatched URLs resolve to an empty 200 so cover <img> loads don't error.
type RouteValue = unknown | ((url: string) => unknown);
export function installFetchMock(routes: Record<string, RouteValue>): void {
  const json = (body: unknown) =>
    Promise.resolve({
      ok: true,
      status: 200,
      json: async () => body,
    } as Response);

  (window as any).fetch = async (input: RequestInfo | URL): Promise<Response> => {
    const url = typeof input === "string" ? input : input.toString();
    for (const [needle, value] of Object.entries(routes)) {
      if (url.includes(needle)) {
        const body = typeof value === "function" ? (value as (u: string) => unknown)(url) : value;
        return json(body);
      }
    }
    return json(null);
  };
}

// The LAN components build cover URLs themselves as `<base>/games/<id>/cover`
// and load them either through a plain <img> or a CSS `background-image: url(…)`
// — neither of which hits installFetchMock's window.fetch stub. To show real
// art in stories the way the desktop fixtures do (absolute Steam CDN URLs
// straight in `cover_image_path`), a MutationObserver watches the rendered DOM
// and rewrites any cover-endpoint URL it sees to a Steam CDN portrait keyed by
// game id — on `<img src>` and on inline `background-image` alike. Rewriting
// reads the final DOM, so it doesn't matter how React set the value. URLs whose
// id isn't mapped pass through untouched. The CDN URL no longer matches the
// pattern, so a rewrite never re-triggers itself.
let coverAppids: Record<string, number> = { ...PEER_COVER_APPIDS };
let coverObserver: MutationObserver | null = null;
// Match a whole cover-endpoint URL — scheme through `/cover` — capturing the
// game id. The path between host and `/games/` is arbitrary (the live peer base
// is `<server-base>/lan/peers/<addr>/<port>`), so the wildcard spans it lazily
// and the entire URL is swapped, not just the suffix. In the DOM both <img src>
// and
// `background-image: url(…)` resolve to absolute URLs, so requiring a scheme is
// safe and keeps the match from straying past a `url(…)` wrapper.
const COVER_RE = /https?:\/\/[^\s)"']*?\/games\/([^/?#)"'\s]+)\/cover/g;

function rewriteCoverUrl(value: string): string {
  if (!value.includes("/cover")) return value;
  return value.replace(COVER_RE, (match, id: string) => {
    const appid = coverAppids[id];
    return appid ? cover(appid) : match;
  });
}

function rewriteCoverNode(node: Node): void {
  if (!(node instanceof HTMLElement)) return;
  const els: HTMLElement[] = [node, ...Array.from(node.querySelectorAll<HTMLElement>("*"))];
  for (const el of els) {
    if (el instanceof HTMLImageElement && el.src) {
      const next = rewriteCoverUrl(el.src);
      if (next !== el.src) el.src = next;
    }
    const bg = el.style.backgroundImage;
    if (bg) {
      const next = rewriteCoverUrl(bg);
      if (next !== bg) el.style.backgroundImage = next;
    }
  }
}

export function installCoverArtMock(appids: Record<string, number> = PEER_COVER_APPIDS): void {
  coverAppids = { ...appids };
  if (coverObserver) {
    rewriteCoverNode(document.body); // re-sweep with the (possibly new) mapping
    return;
  }
  coverObserver = new MutationObserver((records) => {
    for (const rec of records) {
      if (rec.type === "attributes" && rec.target) {
        rewriteCoverNode(rec.target);
      }
      rec.addedNodes.forEach(rewriteCoverNode);
    }
  });
  coverObserver.observe(document.body, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["src", "style"],
  });
  rewriteCoverNode(document.body); // initial sweep for already-mounted nodes
}
