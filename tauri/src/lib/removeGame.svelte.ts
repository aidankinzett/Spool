/**
 * Remove-game dialog store — module-level reactive state.
 *
 * Opens the three-option {@link RemoveGameModal} (Remove from disk / Remove from
 * library / Remove from disk and library) from anywhere — the game detail view,
 * the right-click context menu, or the edit window — without each call site
 * wiring up its own modal. Mounted globally by `+layout.svelte` via
 * `<RemoveGameHost />`, so it works in every window. The host owns the
 * IPC + toast logic; callers just hand over the game (and an optional `onDone`,
 * e.g. to close the edit window after a successful removal).
 *
 * Usage:
 *   import { removeGameDialog } from '$lib/removeGame.svelte';
 *   removeGameDialog.request(game);
 *   removeGameDialog.request(entry, { onDone: () => getCurrentWindow().close() });
 */
import type { GameEntry } from '$lib/types';

export interface RemoveRequest {
  /** The game to remove. */
  game: GameEntry;
  /** Run after a successful removal (e.g. close the edit window). */
  onDone?: () => void;
}

class RemoveGameStore {
  /** The request currently on screen, or null when the modal is closed. */
  current = $state<RemoveRequest | null>(null);

  /** Open the remove chooser for `game`. A second call replaces the first. */
  request(game: GameEntry, opts?: { onDone?: () => void }): void {
    this.current = { game, onDone: opts?.onDone };
  }

  /** Dismiss the modal (Cancel / Escape / scrim / after a successful removal). */
  close(): void {
    this.current = null;
  }
}

export const removeGameDialog = new RemoveGameStore();
