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
import { GameDialogStore, type GameDialogRequest } from '$lib/gameDialog.svelte';

export type MoveRequest = GameDialogRequest;

export const moveInstallDialog = new GameDialogStore();
