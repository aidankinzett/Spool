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
};

class ToastStore {
  /** Reactive array of currently-visible toasts. */
  items = $state<Toast[]>([]);

  #nextId = 0;

  /**
   * Default auto-dismiss: success/info fade after 5s; warn/bad stick until
   * the user dismisses them (you don't want a cloud conflict to vanish).
   */
  #defaultDuration(kind: ToastKind): number {
    return kind === 'ok' || kind === 'info' ? 5000 : 0;
  }

  show(input: Omit<Toast, 'id'>): string {
    const id = `t${++this.#nextId}`;
    const duration = input.duration ?? this.#defaultDuration(input.kind);
    this.items.push({ ...input, id });
    if (duration > 0) {
      setTimeout(() => this.dismiss(id), duration);
    }
    return id;
  }

  dismiss(id: string) {
    const idx = this.items.findIndex((t) => t.id === id);
    if (idx >= 0) this.items.splice(idx, 1);
  }
}

export const toasts = new ToastStore();
