// Shared fixtures + callable/fetch mocking for the stories.
import type { DownloadProgress, LanPeer, LibraryGame, PeerGame } from "../../src/types";
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
  { id: "pg4", game_name: "Big Open World RPG", install_size_mb: 92_000, shareable: false },
];

export function makeDownload(overrides: Partial<DownloadProgress> = {}): DownloadProgress {
  return {
    install_token: "tok-1",
    source_device_name: "Desktop-PC",
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
}

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
