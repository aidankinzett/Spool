/**
 * Pure derivations over LAN transfer state, shared by the library store
 * (which feeds `TransferPill`) and `TransfersPanel`. Keeping the active /
 * count / percent logic here means the pill header and the panel rows can't
 * drift apart — e.g. a cancelled upload is excluded everywhere, not just in
 * the store.
 */
import type { DownloadProgress, UploadSnapshot } from './types';

/** True while our single in-flight install is fetching or transferring. */
export function downloadIsActive(download: DownloadProgress | null): boolean {
  return (
    download != null &&
    (download.status === 'starting' || download.status === 'transferring')
  );
}

/** Whole-percent progress of the active download (0 when unknown). */
export function downloadPercent(download: DownloadProgress | null): number {
  return download && download.bytes_total > 0
    ? Math.round((download.bytes_done / download.bytes_total) * 100)
    : 0;
}

/**
 * Uploads still serving a peer. A cancelled session lingers in the list until
 * the peer notices, so it's rendered (greyed out) but must not be counted as
 * active — otherwise the header disagrees with the rows.
 */
export function liveUploads(uploads: UploadSnapshot[]): UploadSnapshot[] {
  return uploads.filter((u) => !u.cancelled);
}
