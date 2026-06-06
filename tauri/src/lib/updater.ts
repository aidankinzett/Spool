/**
 * Auto-update orchestration.
 *
 * Polls the Tauri updater on app launch; if a newer signed build is
 * available, surfaces a sticky toast with Install/Later CTAs. The
 * actual download + install flow is handled by the plugin — we just
 * provide the UI surface around it.
 *
 * The manifest URL + signing public key are configured in
 * `tauri-rewrite/tauri/src-tauri/tauri.conf.json` (the `plugins.updater`
 * block). The release pipeline publishes `latest.json` to the GitHub
 * release page at that URL.
 */

import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { toasts } from './toasts.svelte';

/**
 * Runs once at app startup. Best-effort — any failure (no network,
 * manifest 404, signature mismatch) just logs and silently moves on
 * so it never blocks launch.
 */
export async function checkForUpdateOnStartup(): Promise<void> {
  let update: Update | null;
  try {
    update = await check();
  } catch (e) {
    console.error('[updater] check failed:', e);
    return;
  }
  if (!update || !update.available) return;

  // Sticky toast — survives across navigations until the user
  // dismisses it or hits Install. The notes payload from the
  // manifest goes into the sub line; clipped to avoid runaway toasts
  // if release notes are long.
  const notes = (update.body ?? '').trim();
  const sub = notes
    ? `v${update.version}${notes ? ' — ' + truncate(notes, 140) : ''}`
    : `v${update.version} is ready to install.`;

  toasts.show({
    kind: 'info',
    label: 'UPDATE AVAILABLE',
    title: `Spool ${update.version}`,
    sub,
    duration: 0, // sticky
    cta: {
      label: 'Install now',
      onClick: () => installUpdate(update!),
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

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max - 1).trimEnd() + '…';
}
