<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { mockIPC } from '@tauri-apps/api/mocks';
  import type { GameEntry } from '$lib/types';
  import GameDetail from './GameDetail.svelte';

  // GameDetail only reaches into Tauri from its action handlers (Play,
  // Add to Steam, Armoury Crate, Remove) — nothing fires on mount. Re-`mockIPC`
  // here (overriding preview.ts's no-op default) so those buttons resolve with
  // plausible payloads instead of throwing if clicked while exploring.
  mockIPC((cmd) => {
    switch (cmd) {
      case 'add_to_steam':
        return { extras_placed: ['cover', 'hero'] };
      case 'generate_armoury_launcher':
        return 'C:/Users/you/AppData/Local/Spool/launchers/hollow-knight.exe';
      case 'remove_game':
        return true;
      default:
        // launch_game, open_path, … — fire-and-forget, void is fine.
        return undefined;
    }
  });

  // Mirrors the fixture in GameDetail.test.ts — a fully-populated GameEntry so
  // stories only override the fields they care about.
  function makeGame(over: Partial<GameEntry> = {}): GameEntry {
    return {
      id: 'g1',
      catalog_number: 1,
      game_name: 'Hollow Knight',
      exe_path: 'C:/Games/HollowKnight/hollow_knight.exe',
      safe_name: 'hollow-knight',
      cover_image_path: null,
      hero_image_path: null,
      added_at: '2026-01-12T09:00:00Z',
      last_played_at: null,
      launcher_exe_path: null,
      game_folder_path: 'C:/Games/HollowKnight',
      run_as_admin: false,
      use_proton: false,
      proton_version_path: null,
      wine_prefix_path: null,
      launch_args: null,
      description:
        'A 2D action-adventure through a vast, ruined kingdom of insects and heroes. Explore twisting caverns, battle tainted creatures and escape ancient labyrinths.',
      developer: 'Team Cherry',
      publisher: 'Team Cherry',
      genres: ['Metroidvania', 'Action', 'Adventure'],
      release_date: '2017-02-24',
      install_size_mb: 9216,
      playtime_minutes: 0,
      lan_shared: false,
      lan_share_folder: null,
      save_backup_count: 0,
      save_last_backed_up_at: null,
      save_backup_size_mb: 0,
      install_source: 'Manual',
      lan_install_source_device_name: null,
      lan_install_source_device_id: null,
      steam_id: null,
      gog_id: null,
      lutris_slug: null,
      manifest_install_dir: null,
      save_paths: [],
      accent_color: null,
      sync_badge: null,
      cloud_sync_baseline: null,
      ...over,
    };
  }

  const { Story } = defineMeta({
    title: 'Detail/GameDetail',
    component: GameDetail,
    render: template,
    // The component is a full right-hand pane — give it the whole canvas.
    parameters: { layout: 'fullscreen' },
    argTypes: {
      runPhase: {
        control: 'select',
        options: [null, 'restoring', 'launching', 'playing', 'backing-up'],
      },
    },
  });
</script>

<!-- GameDetail expects a flex parent with a real height to fill. -->
{#snippet template(args: ComponentProps<typeof GameDetail>)}
  <div style="height: 100vh; display: flex;">
    <GameDetail {...args} />
  </div>
{/snippet}

<!-- Fresh entry: no playtime, no backups, brand accent fallback. -->
<Story name="Idle" args={{ game: makeGame() }} />

<!-- Played-and-backed-up: the "lived-in" state most library entries reach. -->
<Story
  name="Played & backed up"
  args={{
    game: makeGame({
      last_played_at: '2026-05-28T21:14:00Z',
      playtime_minutes: 1873,
      save_backup_count: 12,
      save_backup_size_mb: 34,
      save_last_backed_up_at: '2026-05-28T23:02:00Z',
      save_paths: ['C:/Users/you/AppData/LocalLow/Team Cherry/Hollow Knight'],
      accent_color: '#6fb7c9',
      steam_id: 367520,
      install_source: 'Steam',
    }),
  }}
/>

<!-- Mid-run: Play button is disabled and reflects the live phase label. -->
<Story name="Playing" args={{ game: makeGame({ playtime_minutes: 1873 }), runPhase: 'playing' }} />

<!-- Restoring saves — first phase of the run workflow. -->
<Story name="Restoring saves" args={{ game: makeGame(), runPhase: 'restoring' }} />

<!-- No executable on file: Play and the launcher actions are disabled. -->
<Story name="No executable" args={{ game: makeGame({ exe_path: '' }) }} />

<!-- Sparse metadata: empty About + Saves cards fall back to their hints. -->
<Story
  name="Sparse metadata"
  args={{
    game: makeGame({
      description: '',
      genres: [],
      developer: '',
      publisher: '',
      release_date: null,
    }),
  }}
/>
