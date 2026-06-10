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
import { GameDialogStore, type GameDialogRequest } from '$lib/gameDialog.svelte';

export type RemoveRequest = GameDialogRequest;

export const removeGameDialog = new GameDialogStore();
