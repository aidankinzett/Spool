// Typed wrappers around Tauri's `invoke` IPC bridge. All backend calls go
// through this module — gives us a single place to add caching, mocking for
// tests, or telemetry later, and keeps `invoke<T>(...)` ceremony out of
// every component.

import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import type {
  AddToSteamResult,
  ConfigData,
  GameEntry,
  LanPeer,
  NewGame,
  PeerGame,
  SearchCandidate,
} from './types';

export const api = {
  // Library
  listGames: (): Promise<GameEntry[]> => invoke('list_games'),
  addGame: (newGame: NewGame): Promise<GameEntry> => invoke('add_game', { newGame }),
  updateGame: (entry: GameEntry): Promise<GameEntry> => invoke('update_game', { entry }),
  removeGame: (id: string): Promise<boolean> => invoke('remove_game', { id }),

  // Config
  getConfig: (): Promise<ConfigData> => invoke('get_config'),
  updateConfig: (data: ConfigData): Promise<ConfigData> => invoke('update_config', { data }),
  detectLudusavi: (): Promise<string> => invoke('detect_ludusavi'),

  // Ludusavi — Add Game flow
  searchGames: (query: string): Promise<SearchCandidate[]> => invoke('search_games', { query }),
  searchByExe: (exePath: string): Promise<SearchCandidate[]> =>
    invoke('search_by_exe', { exePath }),
  openLudusaviGui: (): Promise<void> => invoke('open_ludusavi_gui'),

  // SteamGridDB
  fetchCover: (gameId: string): Promise<string | null> => invoke('fetch_cover', { gameId }),

  // Steam shortcut
  addToSteam: (gameId: string): Promise<AddToSteamResult> => invoke('add_to_steam', { gameId }),

  // LAN
  listLanPeers: (): Promise<LanPeer[]> => invoke('list_lan_peers'),
  fetchPeerGames: (addr: string, port: number): Promise<PeerGame[]> =>
    invoke('fetch_peer_games', { addr, port }),

  // Run workflow
  launchGame: (gameId: string): Promise<void> => invoke('launch_game', { gameId }),

  // Lifecycle — pulls + clears the game id queued by a startup `--run` invocation.
  takePendingRun: (): Promise<string | null> => invoke('take_pending_run'),
} as const;

/**
 * Turn an absolute filesystem path (from a `GameEntry`) into a URL that the
 * webview can load via the `asset:` protocol. Returns `null` for null/missing
 * input so callers can use it directly in template expressions.
 */
export function assetUrl(path: string | null | undefined): string | null {
  if (!path) return null;
  return convertFileSrc(path);
}
