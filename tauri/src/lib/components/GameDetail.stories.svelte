<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { mockIPC } from '@tauri-apps/api/mocks';
  import type { GameEntry } from '$lib/types';
  import { makeGame as makePlayedGame } from '../../../.storybook/fixtures';
  import GameDetail from './GameDetail.svelte';

  // GameDetail reaches into Tauri on mount (app_platform gates the Armoury
  // Crate button) and from its action handlers (Play, Add to Steam, Armoury
  // Crate, Remove). Re-`mockIPC` here (overriding preview.ts's no-op default)
  // so the platform probe reports a Windows host and handlers resolve with
  // plausible payloads instead of throwing if clicked while exploring.
  mockIPC((cmd) => {
    switch (cmd) {
      // Pretend we're on Windows so the "Armoury Crate" button renders.
      case 'app_platform':
        return 'windows';
      case 'add_to_steam':
        return { extras_placed: ['cover', 'hero'] };
      case 'generate_armoury_launcher':
        return 'C:/Users/you/AppData/Local/Spool/launchers/hollow-knight.exe';
      case 'remove_game':
        return true;
      default:
        // launch_game, open_path, refresh_save_metadata, … — fire-and-forget,
        // void is fine.
        return undefined;
    }
  });

  // GameDetail's stories explore the *fresh* baseline (no playtime, no backups,
  // brand-accent fallback), whereas the shared fixture defaults to a populated
  // "played" entry. Derive from the shared fixture (single source of truth for
  // the GameEntry shape) with the played-only fields reset, so individual
  // stories still only override what they care about.
  const FRESH: Partial<GameEntry> = {
    last_played_at: null,
    playtime_minutes: 0,
    save_backup_count: 0,
    save_last_backed_up_at: null,
    save_backup_size_mb: 0,
    save_paths: [],
    accent_color: null,
    sync_badge: null,
    steam_id: null,
    install_source: 'Manual',
  };

  function makeGame(over: Partial<GameEntry> = {}): GameEntry {
    return makePlayedGame({ ...FRESH, ...over });
  }

  const { Story } = defineMeta({
    title: 'Detail/GameDetail',
    component: GameDetail,
    render: template,
    parameters: {
      // The component is a full right-hand pane — give it the whole canvas
      // when viewed as a standalone story.
      layout: 'fullscreen',
      // In the Docs page, render each preview in a bounded iframe instead of
      // inline, so the 100vh-tall pane doesn't blow out the page height.
      docs: { story: { inline: false, height: '640px' } },
    },
    argTypes: {
      runPhase: {
        control: 'select',
        options: [null, 'restoring', 'launching', 'playing', 'backing-up', 'uploading'],
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
