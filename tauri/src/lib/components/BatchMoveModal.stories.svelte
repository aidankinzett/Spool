<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { fn } from 'storybook/test';
  import { mockIPC } from '@tauri-apps/api/mocks';
  import { emit } from '@tauri-apps/api/event';
  import { makeGame } from '../../../.storybook/fixtures';
  import type { GameEntry, LibraryFolder } from '$lib/types';
  import BatchMoveModal from './BatchMoveModal.svelte';

  // The batch chooser reaches into Tauri while choosing (free space per folder)
  // and while moving (one move_game_install at a time, streaming move:progress).
  // Mock those so the story is interactive: pick a folder, click Move, and watch
  // each queued game copy in turn. shouldMockEvents lets the emitted progress
  // reach the modal's listen('move:progress').
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  mockIPC(
    (cmd, args) => {
      switch (cmd) {
        case 'folder_free_space':
          // Plenty of room everywhere, so no folder is disabled as too small.
          return 800 * 1024 * 1048576;
        case 'move_game_install': {
          const id = String((args as Record<string, unknown>)?.id ?? '');
          const dest = String((args as Record<string, unknown>)?.destFolder ?? '');
          // Stream a few progress ticks for the active game, then resolve.
          return (async () => {
            for (const frac of [0.25, 0.6, 0.9, 1]) {
              await emit('move:progress', {
                game_id: id,
                game_name: '',
                copied_bytes: Math.round(frac * 4 * 1024 * 1048576),
                total_bytes: 4 * 1024 * 1048576,
                status: frac < 1 ? 'copying' : 'finalizing',
                message: null,
                dest_folder: dest,
              });
              await sleep(180);
            }
            return makeGame({ id });
          })();
        }
        case 'cancel_move':
          return true;
        default:
          return undefined;
      }
    },
    { shouldMockEvents: true },
  );

  const FOLDERS: LibraryFolder[] = [
    { path: 'C:/Games', label: null, default_install: false },
    { path: 'D:/Games', label: 'Fast SSD', default_install: false },
    { path: 'E:/Library', label: 'Bulk HDD', default_install: false },
  ];

  // A selection of three installed games, all currently on C:/Games — so every
  // folder except C:/Games is a valid destination for the whole batch.
  const SELECTION: GameEntry[] = [
    makeGame({ id: 'g1', game_name: 'The Witcher 3: Wild Hunt', game_folder_path: 'C:/Games/The Witcher 3', install_size_mb: 51200 }),
    makeGame({ id: 'g2', game_name: 'Cuphead', game_folder_path: 'C:/Games/Cuphead', install_size_mb: 4096 }),
    makeGame({ id: 'g3', game_name: 'Disco Elysium', game_folder_path: 'C:/Games/Disco Elysium', install_size_mb: 20480 }),
  ];

  const { Story } = defineMeta({
    title: 'Modals/BatchMoveModal',
    component: BatchMoveModal,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    args: {
      games: SELECTION,
      folders: FOLDERS,
      onClose: fn(),
      onDone: fn(),
    },
    render: template,
  });
</script>

{#snippet template(args: ComponentProps<typeof BatchMoveModal>)}
  <div class="bg-bg-0 text-ink-0 min-h-screen">
    <BatchMoveModal {...args} />
  </div>
{/snippet}

<!-- Default: pick a destination for three games. Click Move to watch the queue
     copy one at a time, with a combined progress bar. -->
<Story name="Choose destination" args={{}} />

<!-- A single game in the batch (the count copy and queue degrade to "1 game"). -->
<Story name="Single game" args={{ games: [SELECTION[0]] }} />

<!-- Some of the selection already lives in a destination: those are skipped, not
     moved. Here g2 already sits in D:/Games, so picking D:/Games skips it. -->
<Story
  name="Some already in target"
  args={{
    games: [
      SELECTION[0],
      makeGame({ id: 'g2', game_name: 'Cuphead', game_folder_path: 'D:/Games/Cuphead', install_size_mb: 4096 }),
      SELECTION[2],
    ],
  }}
/>

<!-- No library folders configured yet: the empty-state hint points at Settings. -->
<Story name="No library folders" args={{ folders: [] }} />
