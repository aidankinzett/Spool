import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import type { GameEntry } from '$lib/types';
import GameDetail from './GameDetail.svelte';

// GameDetail reaches into Tauri (opener, webview windows, IPC) from its action
// handlers. None fire on mount, but stub them so importing the component never
// touches a real Tauri runtime.
vi.mock('@tauri-apps/plugin-opener', () => ({ openPath: vi.fn() }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: class {
    static getByLabel = vi.fn(async () => null);
  },
}));
vi.mock('$lib/api', () => ({
  api: new Proxy({}, { get: () => vi.fn() }),
  assetUrl: () => '',
}));
vi.mock('$lib/toasts.svelte', () => ({
  toasts: { push: vi.fn(), success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

function makeGame(over: Partial<GameEntry> = {}): GameEntry {
  return {
    id: 'g1',
    catalog_number: 1,
    game_name: 'Hollow Knight',
    exe_path: 'C:/Games/HollowKnight/hk.exe',
    safe_name: 'hollow-knight',
    cover_image_path: null,
    hero_image_path: null,
    added_at: null,
    last_played_at: null,
    launcher_exe_path: null,
    game_folder_path: null,
    run_as_admin: false,
    use_proton: false,
    proton_version_path: null,
    wine_prefix_path: null,
    launch_args: null,
    description: 'A 2D action-adventure.',
    developer: 'Team Cherry',
    publisher: 'Team Cherry',
    genres: ['Metroidvania'],
    release_date: null,
    install_size_mb: 0,
    playtime_minutes: 0,
    lan_shared: false,
    lan_share_folder: null,
    save_backup_count: 0,
    save_last_backed_up_at: null,
    save_backup_size_mb: 0,
    install_source: '',
    lan_install_source_device_name: null,
    lan_install_source_device_id: null,
    steam_id: null,
    gog_id: null,
    lutris_slug: null,
    has_cloud_save: false,
    manifest_install_dir: null,
    save_paths: [],
    accent_color: null,
    sync_badge: null,
    ...over,
  };
}

describe('GameDetail', () => {
  it('shows the game title and an enabled Play button when idle', () => {
    render(GameDetail, { props: { game: makeGame() } });

    expect(screen.getByTestId('game-title').textContent).toContain(
      'Hollow Knight',
    );

    const play = screen.getByTestId('play-button') as HTMLButtonElement;
    expect(play.textContent).toContain('Play');
    expect(play.disabled).toBe(false);
  });

  it('disables Play and reflects the running phase', () => {
    render(GameDetail, {
      props: { game: makeGame(), runPhase: 'playing' },
    });

    const play = screen.getByTestId('play-button') as HTMLButtonElement;
    expect(play.disabled).toBe(true);
    expect(play.textContent).toContain('Playing');
  });

  it('disables Play when the game has no executable', () => {
    render(GameDetail, { props: { game: makeGame({ exe_path: '' }) } });
    expect((screen.getByTestId('play-button') as HTMLButtonElement).disabled).toBe(
      true,
    );
  });
});
