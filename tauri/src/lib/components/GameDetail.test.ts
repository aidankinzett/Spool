import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import type { GameEntry } from '$lib/types';
import { api } from '$lib/api';
import { confirmDialog } from '$lib/confirm.svelte';
import GameDetail from './GameDetail.svelte';

// GameDetail reaches into Tauri (webview windows, IPC) from its action
// handlers. None fire on mount, but stub them so importing the component never
// touches a real Tauri runtime. ("Open folder" now goes through `api`, mocked
// below, rather than the opener plugin — see system_open.rs / issue #95.)
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: class {
    static getByLabel = vi.fn(async () => null);
  },
}));
vi.mock('$lib/api', () => {
  // One mock per method, so a test can assert on the same function the
  // component called.
  const fns = new Map<PropertyKey, ReturnType<typeof vi.fn>>();
  return {
    api: new Proxy(
      {},
      {
        get: (_, key) => {
          if (!fns.has(key)) fns.set(key, vi.fn(() => Promise.resolve()));
          return fns.get(key);
        },
      },
    ),
    assetUrl: () => '',
  };
});
vi.mock('$lib/confirm.svelte', () => ({
  confirmDialog: vi.fn(async () => false),
}));
vi.mock('$lib/toasts.svelte', () => ({
  toasts: { show: vi.fn(), push: vi.fn(), success: vi.fn(), error: vi.fn(), info: vi.fn() },
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
    installed: true,
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
    manifest_install_dir: null,
    save_paths: [],
    custom_save: null,
    manifest_override: null,
    accent_color: null,
    sync_badge: null,
    cloud_sync_baseline: null,
    save_last_backer_device: null,
    save_cloud_revision_at: null,
    steam_app_id: null,
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
      props: { game: makeGame(), runPhase: 'backing-up' },
    });

    const play = screen.getByTestId('play-button') as HTMLButtonElement;
    expect(play.disabled).toBe(true);
    expect(play.textContent).toContain('Backing up');
  });

  it('turns Play into Stop while playing, and stops after confirming', async () => {
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    render(GameDetail, {
      props: { game: makeGame(), runPhase: 'playing', stoppable: true },
    });

    const button = screen.getByTestId('play-button') as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    expect(button.textContent).toContain('Stop');

    await fireEvent.click(button);
    await vi.waitFor(() => expect(api.stopGame).toHaveBeenCalledWith('g1'));
    expect(button.textContent).toContain('Stopping');
    expect(button.disabled).toBe(true);
  });

  it('does not stop when the confirmation is cancelled', async () => {
    vi.mocked(confirmDialog).mockResolvedValueOnce(false);
    vi.mocked(api.stopGame).mockClear();
    render(GameDetail, {
      props: { game: makeGame(), runPhase: 'playing', stoppable: true },
    });

    await fireEvent.click(screen.getByTestId('play-button'));
    await vi.waitFor(() => expect(confirmDialog).toHaveBeenCalled());
    expect(api.stopGame).not.toHaveBeenCalled();
  });

  it('keeps a disabled Playing label when the game cannot be stopped', () => {
    render(GameDetail, {
      props: { game: makeGame(), runPhase: 'playing', stoppable: false },
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

  it('offers "restore an earlier save" only when more than one revision exists', () => {
    // 2+ revisions → rollback affordance shown.
    const { unmount } = render(GameDetail, {
      props: { game: makeGame({ save_backup_count: 3 }) },
    });
    expect(screen.queryByText('Restore an earlier save')).not.toBeNull();
    unmount();

    // A single revision → nothing to roll back to, so no affordance.
    render(GameDetail, { props: { game: makeGame({ save_backup_count: 1 }) } });
    expect(screen.queryByText('Restore an earlier save')).toBeNull();
  });
});
