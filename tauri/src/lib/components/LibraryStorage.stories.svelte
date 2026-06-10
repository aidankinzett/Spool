<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { mockIPC } from '@tauri-apps/api/mocks';
  import { makeGame } from '../../../.storybook/fixtures';
  import type { FolderCapacity, GameEntry, LibraryFolder } from '$lib/types';
  import LibraryStorage from './LibraryStorage.svelte';
  import ConfirmHost from './ConfirmHost.svelte';
  import ToastStack from './ToastStack.svelte';

  // The component fetches its own game list (list_games) and, from its actions,
  // lists drives, moves installs, and uninstalls games. Mock those so the drive
  // panels render and the actions are interactive. shouldMockEvents keeps the
  // library:changed listener from throwing.
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  mockIPC(
    (cmd, args) => {
      switch (cmd) {
        case 'list_games':
          return SAMPLE_GAMES;
        case 'list_drives':
          return [
            { mount_point: 'C:\\', name: 'OS', total_bytes: 500 * 1024 * 1048576, available_bytes: 120 * 1024 * 1048576, is_removable: false },
            { mount_point: 'D:\\', name: 'Games', total_bytes: 1024 * 1024 * 1048576, available_bytes: 760 * 1024 * 1048576, is_removable: false },
          ];
        case 'uninstall_game':
          return sleep(400); // resolves void after a beat
        case 'move_game_install':
          return makeGame({ id: String((args as Record<string, unknown>)?.id ?? '') });
        case 'cancel_move':
          return true;
        default:
          return undefined;
      }
    },
    { shouldMockEvents: true },
  );

  const FOLDERS: LibraryFolder[] = [
    { path: 'C:/Games', label: 'System drive' },
    { path: 'D:/Games', label: 'Games SSD' },
  ];

  // Per-folder drive capacity (total + available bytes). C:/Games is nearly
  // full — its free segment flags amber.
  const CAPACITY: Record<string, FolderCapacity> = {
    'C:/Games': { total_bytes: 500 * 1024 * 1048576, available_bytes: 40 * 1024 * 1048576 },
    'D:/Games': { total_bytes: 1024 * 1024 * 1048576, available_bytes: 760 * 1024 * 1048576 },
  };

  // Two games on C:/Games, one on D:/Games, one in a stray folder (E:/Misc) that
  // matches no library folder → the "Outside library folders" group. Plus an
  // uninstalled entry that must NOT appear (no files on disk).
  const SAMPLE_GAMES: GameEntry[] = [
    makeGame({ id: 'g1', catalog_number: 1, game_name: 'The Witcher 3: Wild Hunt', game_folder_path: 'C:/Games/The Witcher 3', install_size_mb: 51200 }),
    makeGame({ id: 'g2', catalog_number: 4, game_name: 'Cuphead', game_folder_path: 'C:/Games/Cuphead', install_size_mb: 4096, accent_color: '#e0703a' }),
    makeGame({ id: 'g3', catalog_number: 2, game_name: 'Stardew Valley', game_folder_path: 'D:/Games/Stardew Valley', install_size_mb: 520, accent_color: '#8bbf5a' }),
    makeGame({ id: 'g4', catalog_number: 7, game_name: 'Hades', game_folder_path: 'E:/Misc/Hades', install_size_mb: 15360, accent_color: '#d24b3a' }),
    makeGame({ id: 'g5', catalog_number: 9, game_name: 'Hollow Knight', installed: false, game_folder_path: null, install_size_mb: 0 }),
  ];

  const { Story } = defineMeta({
    title: 'Settings/LibraryStorage',
    component: LibraryStorage,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    args: {
      folders: FOLDERS,
      capacity: CAPACITY,
      onAddFolder: async () => true,
      onRemoveFolder: async () => {},
    },
    render: template,
  });
</script>

<!-- Match the settings content column so the panels read at the right width. -->
{#snippet template(args: ComponentProps<typeof LibraryStorage>)}
  <div class="bg-bg-0 text-ink-0 min-h-screen px-8 py-7">
    <div style="max-width: 700px;">
      <LibraryStorage {...args} />
    </div>
  </div>
  <ConfirmHost />
  <ToastStack />
{/snippet}

<!-- Two drive panels with capacity meters (C:/Games nearly full → amber free)
     plus an "Outside library folders" panel for the stray E:/Misc install.
     Tick games (or a drive's header checkbox) to enable Move / Delete. -->
<Story name="Drive meters" args={{}} />

<!-- Only one library folder configured: the games on D:/Games and E:/Misc now
     match no folder, so they fall under "Outside library folders". -->
<Story
  name="One folder, rest stray"
  args={{
    folders: [{ path: 'C:/Games', label: 'System drive' }],
    capacity: { 'C:/Games': { total_bytes: 500 * 1024 * 1048576, available_bytes: 40 * 1024 * 1048576 } },
  }}
/>
