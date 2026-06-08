/**
 * Shared pre-flight for the Add-to-Steam / Remove-from-Steam actions.
 *
 * Adding or removing a non-Steam shortcut requires restarting Steam so it
 * reloads `shortcuts.vdf` (Steam reads it only at startup and rewrites it on
 * exit). The backend does that automatically — but the shutdown closes whatever
 * game Steam is currently running. When a game is running we ask the user to
 * confirm first; when Steam is idle (or closed) we proceed without interrupting
 * them.
 */

import { api } from '$lib/api';
import { confirmDialog } from '$lib/confirm.svelte';

/**
 * Returns `true` if the Steam-shortcut action should proceed. Prompts for
 * confirmation only when Steam currently has a game running. `action` describes
 * the operation for the prompt body (e.g. "Adding to Steam", "Removing from
 * Steam").
 */
export async function confirmSteamRestart(action = 'Adding to Steam'): Promise<boolean> {
  let gameRunning: boolean;
  try {
    gameRunning = await api.steamGameRunning();
  } catch {
    // Detection is best-effort; if it fails, don't block the action.
    return true;
  }
  if (!gameRunning) return true;

  return confirmDialog({
    label: 'STEAM',
    title: 'A game is running in Steam',
    body: `${action} restarts Steam so it picks up the change — this will close the game you have running. Continue?`,
    confirmLabel: 'Restart Steam',
    cancelLabel: 'Cancel',
    danger: true,
  });
}
