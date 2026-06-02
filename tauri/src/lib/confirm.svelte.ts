/**
 * Confirmation-dialog store — module-level reactive state.
 *
 * A replacement for the browser `window.confirm()`, which Tauri's webview
 * doesn't reliably surface (the call returns immediately, so destructive
 * actions ran without ever asking). Instead, `confirmDialog(...)` returns a
 * promise that resolves to the user's choice once they answer a custom modal.
 *
 * Usage:
 *   import { confirmDialog } from '$lib/confirm.svelte';
 *
 *   if (!(await confirmDialog({
 *     title: 'Remove from library?',
 *     body: `"${game.game_name}" will be forgotten.`,
 *     confirmLabel: 'Remove',
 *   })) return;
 *
 * Mounted globally by `+layout.svelte` via `<ConfirmHost />`, so it works in
 * every window (main, add, edit, settings, splash) without per-route wiring.
 */

export interface ConfirmOptions {
  /** Headline question. */
  title: string;
  /** Optional supporting line(s). Newlines render as separate paragraphs. */
  body?: string;
  /** Primary button text. Defaults to "Confirm". */
  confirmLabel?: string;
  /** Secondary button text. Defaults to "Cancel". */
  cancelLabel?: string;
  /** Render the confirm button in the destructive (red) style. */
  danger?: boolean;
  /** Mono eyebrow shown in the chrome (uppercase, tracked). E.g. "LUDUSAVI". */
  label?: string;
  /** Optional catalog id badge (e.g. "SPL-0031"). */
  catalog?: string;
  /** Per-game accent hex for the non-danger confirm button + chrome tape. */
  accent?: string | null;
}

interface ActiveConfirm extends ConfirmOptions {
  id: string;
  resolve: (ok: boolean) => void;
}

class ConfirmStore {
  /** The dialog currently on screen, or null when none is open. */
  current = $state<ActiveConfirm | null>(null);

  #nextId = 0;

  /**
   * Open a confirmation dialog and resolve once the user answers.
   * Resolves `true` for confirm, `false` for cancel / dismiss / Escape.
   *
   * Only one dialog shows at a time; opening a second auto-cancels the first
   * (its promise resolves `false`) so a stray second prompt can't strand a
   * caller awaiting the first.
   */
  ask(opts: ConfirmOptions): Promise<boolean> {
    if (this.current) this.#settle(false);
    return new Promise<boolean>((resolve) => {
      this.current = { ...opts, id: `c${++this.#nextId}`, resolve };
    });
  }

  #settle(ok: boolean): void {
    const c = this.current;
    if (!c) return;
    this.current = null;
    c.resolve(ok);
  }

  /** Answer the active dialog affirmatively. */
  confirm(): void {
    this.#settle(true);
  }

  /** Dismiss the active dialog (Cancel / scrim / Escape). */
  cancel(): void {
    this.#settle(false);
  }
}

export const confirms = new ConfirmStore();

/** Shorthand: open a confirm dialog, await the user's choice. */
export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  return confirms.ask(opts);
}
