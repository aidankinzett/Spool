/**
 * Toast notification store — module-level reactive state.
 *
 * Usage:
 *   import { toasts } from '$lib/toasts.svelte';
 *
 *   toasts.show({
 *     kind: 'warn',
 *     label: 'LUDUSAVI',
 *     title: 'Cloud sync conflict',
 *     sub: 'Open Ludusavi to resolve before launching.',
 *     cta: { label: 'Open Ludusavi', onClick: () => api.openLudusaviGui() },
 *     duration: 0,   // 0 = sticky, otherwise ms before auto-dismiss
 *   });
 *
 * Mounted globally by `+layout.svelte` via `<ToastStack />`.
 */

export type ToastKind = 'ok' | 'info' | 'warn' | 'bad';

export type Toast = {
  id: string;
  kind: ToastKind;
  /** Mono eyebrow (uppercase, tracked). E.g. "LUDUSAVI" or "SYNC · CONFLICT". */
  label: string;
  title: string;
  sub: string;
  /** Optional catalog id badge (e.g. "SPL-0031"). */
  catalog?: string;
  /** Optional inline action button. */
  cta?: { label: string; onClick: () => void };
  /** Milliseconds before auto-dismiss. 0 (default for warn/bad) = sticky. */
  duration?: number;
  /** Wall-clock ms set internally by `show()`. Used by the relative-time
   *  chip on the right of the toast header ("now" / "12s" / "4m"). */
  createdAt?: number;
};

class ToastStore {
  /** Reactive array of currently-visible toasts. */
  items = $state<Toast[]>([]);

  /**
   * Bumps every second so toasts can derive a live relative-time
   * chip via `$derived`. The ticker only starts when the first toast
   * shows (lazy) so SSR isn't paying for a setInterval that nobody
   * reads.
   */
  tick = $state(0);

  #nextId = 0;
  #tickerStarted = false;

  /**
   * Default auto-dismiss: success/info fade after 5s; warn/bad stick until
   * the user dismisses them (you don't want a cloud conflict to vanish).
   */
  #defaultDuration(kind: ToastKind): number {
    return kind === 'ok' || kind === 'info' ? 5000 : 0;
  }

  show(input: Omit<Toast, 'id' | 'createdAt'>): string {
    if (!this.#tickerStarted) this.#startTicker();
    const id = `t${++this.#nextId}`;
    const duration = input.duration ?? this.#defaultDuration(input.kind);
    this.items.push({ ...input, id, createdAt: Date.now() });
    if (duration > 0) {
      setTimeout(() => this.dismiss(id), duration);
    }
    return id;
  }

  dismiss(id: string) {
    const idx = this.items.findIndex((t) => t.id === id);
    if (idx >= 0) this.items.splice(idx, 1);
  }

  #startTicker(): void {
    if (typeof window === 'undefined') return; // SSR-safe — no ticker on the server
    this.#tickerStarted = true;
    setInterval(() => {
      this.tick++;
    }, 1000);
  }
}

export const toasts = new ToastStore();

/**
 * Renders a wall-clock `createdAt` as the "now" / "12s" / "4m" / "2h"
 * chip the design uses on the right of each toast header. Returns
 * `"now"` for missing input so freshly-pushed toasts read clean.
 */
export function fmtToastTime(createdAt: number | undefined): string {
  if (!createdAt) return 'now';
  const diffSec = Math.max(0, (Date.now() - createdAt) / 1000);
  if (diffSec < 5) return 'now';
  if (diffSec < 60) return `${Math.round(diffSec)}s`;
  if (diffSec < 3600) return `${Math.round(diffSec / 60)}m`;
  return `${Math.round(diffSec / 3600)}h`;
}
