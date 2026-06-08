/**
 * Auto-update orchestration.
 *
 * Polls the Tauri updater on app launch and then on a recurring
 * interval; if a newer signed build is available, surfaces a sticky
 * toast with Install/Later CTAs. The actual download + install flow is
 * handled by the plugin — we just provide the UI surface around it.
 *
 * Spool hides to the system tray instead of quitting, so the main
 * webview mounts once and can stay alive for days. A single startup
 * check would then go stale, so `startUpdateChecks` keeps polling on an
 * interval while the process lives. The Tauri updater has no built-in
 * scheduler — `check()` is just a function — so the cadence is driven
 * from this long-lived webview's timer.
 *
 * The manifest URL + signing public key are configured in
 * `tauri-rewrite/tauri/src-tauri/tauri.conf.json` (the `plugins.updater`
 * block). The release pipeline publishes `latest.json` to the GitHub
 * release page at that URL.
 */

import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { toasts } from './toasts.svelte';

/** How often to re-check for updates while the process stays tray-resident. */
const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000; // 6 hours

/**
 * Version of the update we've already surfaced a toast for. Prevents the
 * recurring check from stacking a fresh sticky toast every interval while
 * the same update sits available and the user hasn't acted on it yet.
 */
let surfacedVersion: string | null = null;

/**
 * Begin polling for updates: one check shortly after mount, then every
 * `UPDATE_CHECK_INTERVAL_MS`. Returns a teardown that cancels both the
 * pending startup check and the interval.
 *
 * Each check is best-effort — any failure (no network, manifest 404,
 * signature mismatch) just logs and silently moves on so it never blocks
 * launch or wedges the loop.
 */
export function startUpdateChecks(): () => void {
  const startupTimer = setTimeout(() => {
    void checkForUpdate();
  }, 2000);

  const interval = setInterval(() => {
    void checkForUpdate();
  }, UPDATE_CHECK_INTERVAL_MS);

  return () => {
    clearTimeout(startupTimer);
    clearInterval(interval);
  };
}

/**
 * Single update check. Surfaces a sticky toast the first time it sees a
 * given version available; quietly no-ops on repeat checks for that same
 * version so the tray-resident interval doesn't pile up toasts.
 */
export async function checkForUpdate(): Promise<void> {
  let update: Update | null;
  try {
    update = await check();
  } catch (e) {
    console.error('[updater] check failed:', e);
    return;
  }
  if (!update || !update.available) return;
  // `force: false` — skip the toast if we already surfaced this version on an
  // earlier check, so the tray-resident interval doesn't pile up toasts.
  surfaceUpdate(update, false);
}

/** Outcome of an interactive ("Check for updates" button) check. */
export type UpdateCheckResult =
  | { status: 'available'; version: string }
  | { status: 'up-to-date' }
  | { status: 'error'; error: string };

/**
 * User-initiated update check (the Settings button). Unlike the silent
 * background check, it reports every outcome back to the caller so the UI can
 * say "you're up to date" / "couldn't check", and it always (re)surfaces the
 * install toast when an update exists — the user explicitly asked, so a
 * previously-dismissed toast shouldn't make the click do nothing.
 */
export async function checkForUpdateInteractive(): Promise<UpdateCheckResult> {
  let update: Update | null;
  try {
    update = await check();
  } catch (e) {
    console.error('[updater] manual check failed:', e);
    return { status: 'error', error: String(e) };
  }
  if (!update || !update.available) return { status: 'up-to-date' };
  surfaceUpdate(update, true);
  return { status: 'available', version: update.version };
}

/**
 * Show the sticky "update available" toast with an Install CTA. `force`
 * bypasses the once-per-version guard for user-initiated checks.
 */
function surfaceUpdate(update: Update, force: boolean): void {
  if (!force && surfacedVersion === update.version) return;
  surfacedVersion = update.version;

  // Sticky toast — survives across navigations until the user
  // dismisses it or hits Install. The notes payload from the manifest
  // (the release's per-PR changelog) becomes the sub line as a short
  // bulleted list; long changelogs collapse to a "+N more" tail so the
  // toast can't grow without bound.
  const notes = formatNotes(update.body ?? '');
  const sub = notes || 'A new version is ready to install.';

  toasts.show({
    kind: 'info',
    label: 'UPDATE AVAILABLE',
    title: `Spool ${update.version}`,
    sub,
    duration: 0, // sticky
    cta: {
      label: 'Install now',
      onClick: () => installUpdate(update),
    },
  });
}

async function installUpdate(update: Update): Promise<void> {
  const id = toasts.show({
    kind: 'info',
    label: 'UPDATE',
    title: 'Downloading update…',
    sub: `v${update.version} — Spool will restart when done.`,
    duration: 0,
    progress: 0,
  });

  // The updater streams download events: a `Started` carrying the total
  // content length, then a `Progress` per chunk, then `Finished`. Sum the
  // chunk lengths against the total to drive the toast's progress bar.
  // `contentLength` can be missing (chunked/unknown-length responses); in
  // that case we leave the bar at 0 and just let the toast spin until done.
  let downloaded = 0;
  let total = 0;

  try {
    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          total = event.data.contentLength ?? 0;
          break;
        case 'Progress':
          downloaded += event.data.chunkLength;
          if (total > 0) toasts.update(id, { progress: downloaded / total });
          break;
        case 'Finished':
          toasts.update(id, {
            progress: 1,
            title: 'Installing update…',
            sub: `v${update.version} — Spool will restart shortly.`,
          });
          break;
      }
    });
    // On Windows the NSIS installer relaunches Spool itself after
    // running silently, so the process is usually gone by now. On the
    // Linux AppImage (and macOS) the updater just swaps the bundle in
    // place and returns — nothing restarts the app, leaving the user
    // on the old version. Relaunch explicitly so the new build takes
    // effect; harmless on Windows if the process is still alive.
    await relaunch();
  } catch (e) {
    console.error('[updater] downloadAndInstall failed:', e);
    toasts.dismiss(id);
    toasts.show({
      kind: 'bad',
      label: 'UPDATE · FAILED',
      title: "Couldn't install update",
      sub: String(e),
    });
  }
}

/** Max changelog lines to show before collapsing the rest to "+N more". */
const MAX_NOTE_LINES = 6;

/**
 * Turn the manifest's notes blob (one `- title (#123)` line per change,
 * built by the release pipeline) into a compact multi-line string for the
 * toast: trimmed non-empty lines, each clipped so a long PR title can't run
 * off, capped at `MAX_NOTE_LINES` with a "+N more" tail. Returns '' when
 * there's nothing to show so the caller can fall back to a generic line.
 */
function formatNotes(body: string): string {
  const lines = body
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => truncate(l, 80));
  if (lines.length === 0) return '';
  if (lines.length <= MAX_NOTE_LINES) return lines.join('\n');
  const rest = lines.length - MAX_NOTE_LINES;
  return lines.slice(0, MAX_NOTE_LINES).join('\n') + `\n…and ${rest} more`;
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max - 1).trimEnd() + '…';
}
