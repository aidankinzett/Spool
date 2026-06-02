<script lang="ts">
  /**
   * Story harness for the Game-Mode splash. The splash has no props — it builds
   * its entire state from `run:phase` events. This harness renders the real
   * splash page and emits a `run:phase` event reflecting the current controls,
   * re-emitting whenever they change, so the `phase` control steps the splash
   * through the launch → play → backup → done pipeline.
   *
   * The IPC mock (list_games, current_sync_status, …) is installed by the
   * `tauriDecorator` wrapping this harness, which runs before the splash mounts
   * and registers its listener — so the first emit below is never lost.
   */
  import { emitTauriEvent } from './tauri-mock';
  import Splash from '../src/routes/splash/+page.svelte';

  let {
    phase = 'restoring',
    message = null,
    cloudUsed = false,
    cloudUploadFailed = false,
    sessionMinutes = 1873,
    /** For `error`: emit a backing-up phase first so the splash frames it as a
     * failed *backup* (exit flow) rather than a failed *restore* (launch flow). */
    errorDuringExit = false,
  }: {
    phase?: string;
    message?: string | null;
    cloudUsed?: boolean;
    cloudUploadFailed?: boolean;
    sessionMinutes?: number | null;
    errorDuringExit?: boolean;
  } = $props();

  // Exit-flow phases carry a session length; launch-flow phases don't.
  const EXIT = new Set(['backing-up', 'done']);

  $effect(() => {
    // Read the controls so the effect re-runs when they change…
    const base = {
      game_id: 'g1',
      message: message ?? null,
      cloud_used: cloudUsed,
      cloud_upload_failed: cloudUploadFailed,
      session_minutes: EXIT.has(phase) || phase === 'error' ? sessionMinutes : null,
    };
    const events =
      phase === 'error' && errorDuringExit
        ? [{ ...base, phase: 'backing-up' }, { ...base, phase: 'error' }]
        : [{ ...base, phase }];
    // …but emit on a fresh task. Emitting synchronously here would run the
    // splash's listener (and its state writes) inside this effect's tick,
    // which Svelte flags as an update-depth loop.
    const t = setTimeout(() => {
      for (const ev of events) void emitTauriEvent('run:phase', ev);
    }, 0);
    return () => clearTimeout(t);
  });
</script>

<Splash />
