<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { waitFor, expect } from 'storybook/test';
  import { tauriDecorator } from '../../../.storybook/tauri-mock';
  import { makeGame } from '../../../.storybook/fixtures';
  import SplashHarness from '../../../.storybook/SplashHarness.svelte';

  // Assert the splash settled on the expected phase. The harness emits the
  // `run:phase` event on a fresh task and the splash paces phase changes, so
  // wait for the headline kicker to read the expected label. Scoped to the
  // `.kicker-text` element since some phase labels also appear in a step badge.
  function expectKicker(text: string) {
    return async ({ canvasElement }: { canvasElement: HTMLElement }) => {
      await waitFor(() => {
        const el = canvasElement.querySelector('.kicker-text');
        expect(el?.textContent?.trim()).toBe(text);
      });
    };
  }

  // The splash is driven entirely by `run:phase` events. SplashHarness renders
  // the real splash page and emits an event for the current controls — use the
  // `phase` control on the "Step through" story to walk the pipeline. The mock
  // provides the game (id `g1`, matching the harness's emitted `game_id`) and a
  // reachable cloud unless a story overrides `current_sync_status`.
  const ONLINE = { reachability: 'online', server_version: null, error: null, last_ok_ago_secs: 3 };
  const OFFLINE = { reachability: 'offline', server_version: null, error: 'unreachable', last_ok_ago_secs: 240 };

  const { Story } = defineMeta({
    title: 'Screens/Splash',
    component: SplashHarness,
    tags: ['!autodocs'],
    parameters: {
      layout: 'fullscreen',
      tauri: { list_games: [makeGame({ id: 'g1' })], current_sync_status: ONLINE },
    },
    decorators: [tauriDecorator()],
    argTypes: {
      phase: {
        control: 'select',
        options: ['restoring', 'launching', 'playing', 'backing-up', 'done', 'error'],
      },
      message: { control: 'text' },
      cloudUsed: { control: 'boolean' },
      cloudUploadFailed: { control: 'boolean' },
      sessionMinutes: { control: 'number' },
      errorDuringExit: { control: 'boolean' },
    },
    args: {
      phase: 'restoring',
      cloudUsed: true,
      cloudUploadFailed: false,
      sessionMinutes: 1873,
      errorDuringExit: false,
    },
  });
</script>

<!-- Use the `phase` control to step the splash through the whole pipeline. -->
<Story name="Step through" args={{ phase: 'restoring' }} />

<!-- Launch flow ------------------------------------------------------------ -->
<Story name="Restoring saves" args={{ phase: 'restoring' }} play={expectKicker('RESTORING')} />
<Story name="Launching" args={{ phase: 'launching' }} play={expectKicker('SAVES RESTORED')} />
<Story name="Playing" args={{ phase: 'playing' }} play={expectKicker('STARTING')} />

<!-- Exit flow -------------------------------------------------------------- -->
<Story name="Backing up" args={{ phase: 'backing-up' }} play={expectKicker('BACKING UP')} />
<Story name="Done" args={{ phase: 'done' }} play={expectKicker('ALL SAVED')} />

<!-- Cloud unreachable: backup is local-only, sync queued. -->
<Story
  name="Cloud offline"
  args={{ phase: 'done', cloudUploadFailed: true }}
  parameters={{ tauri: { list_games: [makeGame({ id: 'g1' })], current_sync_status: OFFLINE } }}
/>

<!-- Errors ----------------------------------------------------------------- -->
<Story
  name="Restore failed"
  args={{ phase: 'error', message: 'ludusavi restore failed: backup not found' }}
  play={expectKicker('RESTORE FAILED')}
/>
<Story
  name="Backup failed"
  args={{ phase: 'error', errorDuringExit: true, message: 'ludusavi backup failed: target unavailable' }}
  play={expectKicker('BACKUP FAILED')}
/>

<!-- Cloud save conflict: the local-vs-cloud picker overlays the splash. -->
<Story
  name="Cloud conflict"
  args={{ phase: 'error', message: 'cloud sync conflict detected' }}
  parameters={{
    tauri: {
      list_games: [makeGame({ id: 'g1' })],
      current_sync_status: ONLINE,
      get_cloud_conflict_details: {
        local: { modified: '2026-05-28T22:40:00Z', size_bytes: 35 * 1024 * 1024 },
        cloud: { modified: '2026-05-29T08:15:00Z', size_bytes: 36 * 1024 * 1024 },
      },
    },
  }}
/>
