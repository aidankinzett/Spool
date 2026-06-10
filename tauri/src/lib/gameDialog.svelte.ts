/**
 * Shared store class for the global per-game dialogs (remove game,
 * move install). Each dialog module instantiates one store and exports it;
 * a matching host component mounted by `+layout.svelte` renders the modal in
 * every window, so any view can open it without wiring its own modal.
 */
import type { GameEntry } from '$lib/types';

export interface GameDialogRequest {
  /** The game the dialog acts on. */
  game: GameEntry;
  /** Run after the dialog's action succeeds (e.g. close the edit window). */
  onDone?: () => void;
}

export class GameDialogStore {
  /** The request currently on screen, or null when the modal is closed. */
  current = $state<GameDialogRequest | null>(null);

  /** Open the dialog for `game`. A second call replaces the first. */
  request(game: GameEntry, opts?: { onDone?: () => void }): void {
    this.current = { game, onDone: opts?.onDone };
  }

  /** Dismiss the modal (Cancel / Escape / scrim / after a successful action). */
  close(): void {
    this.current = null;
  }
}
