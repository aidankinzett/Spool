import { onMount } from 'svelte';
import { listen } from '@tauri-apps/api/event';

/**
 * Subscribe a component to the cross-window `library:changed` event.
 *
 * Settings, the splash, and other secondary windows don't share the main
 * library store, so each fetches its own state and refreshes when any window
 * mutates the library. This wraps the listen + teardown dance — including the
 * unmount-before-`listen()`-resolves race — so callers don't re-implement it.
 *
 * `cb` is invoked once immediately on mount (initial load) and again on every
 * `library:changed` emit. Call during component init.
 */
export function onLibraryChanged(cb: () => void): void {
  onMount(() => {
    cb();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen('library:changed', () => cb())
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((e) => console.error('[library:changed] listener failed:', e));
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
}
