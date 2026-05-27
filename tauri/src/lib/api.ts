// Typed wrappers around Tauri's `invoke` IPC bridge. All backend calls go
// through this module — gives us a single place to add caching, mocking for
// tests, or telemetry later, and keeps `invoke<T>(...)` ceremony out of
// every component.

import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import type { ConfigData, GameEntry } from './types';

export const api = {
  // Library
  listGames: (): Promise<GameEntry[]> => invoke('list_games'),

  // Config
  getConfig: (): Promise<ConfigData> => invoke('get_config'),
  updateConfig: (data: ConfigData): Promise<ConfigData> => invoke('update_config', { data }),
  detectLudusavi: (): Promise<string> => invoke('detect_ludusavi'),
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
