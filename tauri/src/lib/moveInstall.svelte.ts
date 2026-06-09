/**
 * Move-install dialog store — module-level reactive state.
 *
 * Opens the {@link MoveInstallModal} (pick a library folder → relocate the
 * game's install) from anywhere — the game detail view, the right-click context
 * menu, or the edit window — without each call site wiring up its own modal.
 * Mounted globally by `+layout.svelte` via `<MoveInstallHost />`, so it works in
 * every window. The host loads the configured library folders and owns the
 * IPC + progress wiring; callers just hand over the game (and an optional
 * `onDone`, e.g. to close the edit window after a successful move).
 *
 * Usage:
 *   import { moveInstallDialog } from '$lib/moveInstall.svelte';
 *   moveInstallDialog.request(game);
 *   moveInstallDialog.request(entry, { onDone: () => getCurrentWindow().close() });
 */
import type { GameEntry } from '$lib/types';

export interface MoveRequest {
  /** The game whose install is being moved. */
  game: GameEntry;
  /** Run after a successful move (e.g. close the edit window). */
  onDone?: () => void;
}

class MoveInstallStore {
  /** The request currently on screen, or null when the modal is closed. */
  current = $state<MoveRequest | null>(null);

  /** Open the move-install chooser for `game`. A second call replaces the first. */
  request(game: GameEntry, opts?: { onDone?: () => void }): void {
    this.current = { game, onDone: opts?.onDone };
  }

  /** Dismiss the modal (Cancel / Escape / scrim / after a successful move). */
  close(): void {
    this.current = null;
  }
}

export const moveInstallDialog = new MoveInstallStore();
