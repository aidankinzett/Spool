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

import { getVersion } from '@tauri-apps/api/app';
import { platform, version as osVersion, arch } from '@tauri-apps/plugin-os';

export type ToastKind = 'ok' | 'info' | 'warn' | 'bad';

/** Prefilled content for a "Report issue" action that opens a GitHub new-issue
 *  page. `body` is the human-written part; environment details (app version,
 *  user agent) are appended at click time by {@link buildIssueUrl}. */
export type ReportInfo = { title: string; body: string };

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
  /** "Report issue" action that opens a prefilled GitHub issue. For `kind:
   *  'bad'` toasts this is auto-derived from `title`/`sub` unless set here —
   *  pass `false` to suppress it, or an object to override the prefill.
   *  Resolved to a `ReportInfo | undefined` by `show()` before storage. */
  report?: ReportInfo | false;
  /** Milliseconds before auto-dismiss. 0 (default for warn/bad) = sticky. */
  duration?: number;
  /** Optional determinate progress bar, 0–1. Undefined = no bar. A value
   *  outside that range is clamped when rendered. Set/updated via `update()`. */
  progress?: number;
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
    const report = this.#resolveReport(input);
    this.items.push({ ...input, id, createdAt: Date.now(), report });
    if (duration > 0) {
      setTimeout(() => this.dismiss(id), duration);
    }
    return id;
  }

  /**
   * Patches fields of an existing toast in place (e.g. to advance a
   * progress bar or swap the title/sub). No-op if the id is gone —
   * the user may have dismissed it. Mutates the array element so
   * Svelte's deep reactivity picks up the change.
   */
  update(id: string, patch: Partial<Omit<Toast, 'id' | 'createdAt'>>) {
    const t = this.items.find((t) => t.id === id);
    if (t) Object.assign(t, patch);
  }

  dismiss(id: string) {
    const idx = this.items.findIndex((t) => t.id === id);
    if (idx >= 0) this.items.splice(idx, 1);
  }

  /**
   * Decides the resolved "Report issue" action for a toast. Explicit `false`
   * opts out; an explicit object is used verbatim; otherwise error toasts
   * (`kind: 'bad'`) get an auto-derived report from their title + sub so every
   * failure surfaces a one-click way to file it. Non-error toasts get none.
   */
  #resolveReport(input: Omit<Toast, 'id' | 'createdAt'>): ReportInfo | undefined {
    if (input.report === false) return undefined;
    if (input.report) return input.report;
    if (input.kind !== 'bad') return undefined;
    return { title: input.title, body: input.sub };
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

/** GitHub new-issue endpoint for this repo. GitHub prefills the form from the
 *  `title` / `body` query params. */
const ISSUE_URL = 'https://github.com/aidankinzett/Spool/issues/new';

/**
 * Builds a GitHub new-issue URL from a {@link ReportInfo}, appending the app
 * version and OS details (platform / version / arch) so reports arrive with
 * environment context. Both are best-effort — if the lookups fail (e.g. outside
 * Tauri, in Storybook) they're omitted rather than blocking the report.
 */
export async function buildIssueUrl(report: ReportInfo): Promise<string> {
  let appVersion = '';
  try {
    appVersion = await getVersion();
  } catch {
    // best-effort — a missing version shouldn't block filing the issue
  }
  let os = '';
  try {
    os = `${platform()} ${osVersion()} (${arch()})`;
  } catch {
    // best-effort — OS info is unavailable outside a Tauri webview
  }
  const body = [
    report.body,
    '',
    '---',
    appVersion ? `Spool version: ${appVersion}` : null,
    os ? `OS: ${os}` : null,
  ]
    .filter((line) => line !== null)
    .join('\n');
  const query = `title=${encodeURIComponent(report.title)}&body=${encodeURIComponent(body)}`;
  return `${ISSUE_URL}?${query}`;
}

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
