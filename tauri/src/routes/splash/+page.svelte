<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { api } from '$lib/api';
  import type { RunPhaseEvent } from '$lib/types';

  let phase = $state<string>('restoring');
  let message = $state<string>('Preparing…');

  const LABELS: Record<string, string> = {
    restoring: 'Restoring saves…',
    launching: 'Launching game…',
    playing: 'Game running',
    'backing-up': 'Backing up saves…',
    done: 'Done',
    error: 'Launch failed',
  };

  // Keep each phase on screen for at least this long, so transitions the
  // backend fires almost simultaneously (e.g. `launching` → `playing`) are
  // shown in sequence instead of one instantly replacing the other.
  const MIN_VISIBLE_MS = 700;
  const queue: RunPhaseEvent[] = [];
  let draining = false;

  function apply(ev: RunPhaseEvent) {
    phase = ev.phase;
    message = ev.message ?? LABELS[ev.phase] ?? ev.phase;
  }

  async function pump() {
    if (draining) return;
    draining = true;
    while (queue.length > 0) {
      const ev = queue.shift()!;
      apply(ev);
      // Terminal states shouldn't hold the window open.
      if (ev.phase === 'done' || ev.phase === 'error') break;
      // Only pace when another phase is already waiting behind this one.
      if (queue.length > 0) {
        await new Promise((r) => setTimeout(r, MIN_VISIBLE_MS));
      }
    }
    draining = false;
  }

  onMount(() => {
    let unlistenFn: (() => void) | undefined;
    listen<RunPhaseEvent>('run:phase', (event) => {
      queue.push(event.payload);
      void pump();
    })
      .then((fn) => {
        unlistenFn = fn;
        // Listener is wired — tell the backend it's safe to start the
        // workflow. Without this the early phases race the webview load
        // and the splash stays stuck on its default "Restoring saves…".
        return api.notifySplashReady();
      })
      .catch((e) => console.error('[splash] run-phase listener failed:', e));
    return () => unlistenFn?.();
  });
</script>

<div class="splash">
  <div class="logo">SPOOL</div>
  <div class="spinner" class:error={phase === 'error'}></div>
  <!-- On error, show the specific reason (e.g. a restore timeout) rather than
       the generic "Launch failed" label so the user knows why the launch stopped. -->
  <div class="label">{phase === 'error' ? message : (LABELS[phase] ?? message)}</div>
</div>

<style>
  :global(body) {
    margin: 0;
    background: #0b0c0e;
    color: #e8eaed;
    overflow: hidden;
  }
  .splash {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1.25rem;
    height: 100vh;
    font-family: system-ui, sans-serif;
  }
  .logo {
    font-weight: 700;
    letter-spacing: 0.35em;
    font-size: 1.1rem;
    opacity: 0.8;
  }
  .spinner {
    width: 36px;
    height: 36px;
    border: 3px solid #2a2d31;
    border-top-color: #7aa2f7;
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  .spinner.error {
    border-top-color: #f7768e;
    animation: none;
  }
  .label {
    font-size: 0.95rem;
    opacity: 0.85;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
