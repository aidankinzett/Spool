<script lang="ts">
  /**
   * Unified transfers panel — opens from `TransferPill`. Two sticky-
   * header sections: Downloading (peer → us) and Uploading (us →
   * peer). Each row carries a `CassetteProgress` strip. Footer rolls
   * combined progress + throughput.
   *
   * Wire data:
   *   - `download` is our single in-flight LAN install (Tauri backend
   *     serialises to one slot today; the panel renders 0 or 1 rows).
   *   - `uploads` is the list of peers currently downloading from us
   *     (multiple sessions OK).
   *
   * Per the Spool Transfers redesign — central hub mirroring Steam.
   */
  import { ArrowDown, ArrowUp, X } from '@lucide/svelte';
  import type { DownloadProgress, UploadSnapshot } from '$lib/types';
  import { assetUrl } from '$lib/api';
  import MonoLabel from './MonoLabel.svelte';
  import CassetteProgress from './CassetteProgress.svelte';

  interface Props {
    download: DownloadProgress | null;
    uploads: UploadSnapshot[];
    /** Called when the user clicks the X on the active download. */
    onCancelDownload?: () => void;
    /** Called when the user clicks the X on an upload row. */
    onCancelUpload?: (session: UploadSnapshot) => void;
    /**
     * Resolves a local library game id to its cover URL (already
     * wrapped through `assetUrl` so the webview can load it). Returns
     * `null` when the game isn't in the local library or has no
     * cover yet — the row falls back to a sleeve gradient. Used for
     * uploads where we ARE the source so the entry exists locally;
     * downloads stay as gradient placeholders until install completes
     * and a library entry is created.
     */
    coverFor?: (gameId: string) => string | null;
  }

  let {
    download,
    uploads,
    onCancelDownload,
    onCancelUpload,
    coverFor,
  }: Props = $props();

  // ── Derived totals ──────────────────────────────────────────────────────
  const downloadActive = $derived(
    download != null && (download.status === 'starting' || download.status === 'transferring'),
  );
  const dlCount = $derived(downloadActive ? 1 : 0);
  const dlPercent = $derived(
    download && download.bytes_total > 0
      ? Math.round((download.bytes_done / download.bytes_total) * 100)
      : 0,
  );

  const ulCount = $derived(uploads.length);

  const totalSpeed = $derived(download?.bytes_per_second ?? 0);

  function fmtRate(bps: number): string {
    if (!bps || bps <= 0) return '—';
    if (bps < 1024) return `${bps.toFixed(0)} B/s`;
    if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(1)} KB/s`;
    if (bps < 1024 * 1024 * 1024) return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`;
    return `${(bps / (1024 * 1024 * 1024)).toFixed(2)} GB/s`;
  }

  function fmtBytes(bytes: number): string {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function dlLabel(d: DownloadProgress): string {
    if (d.status === 'starting') {
      // Show the backend's current_file — that's where phase labels
      // like "Fetching manifest…" live. Falls back to the generic
      // string only if the backend hasn't published a phase yet.
      return d.current_file || 'Preparing transfer…';
    }
    const left =
      d.bytes_total > 0
        ? `${fmtBytes(d.bytes_done)} / ${fmtBytes(d.bytes_total)}`
        : fmtBytes(d.bytes_done);
    const pct =
      d.bytes_total > 0
        ? ` · ${Math.min(100, Math.round((d.bytes_done / d.bytes_total) * 100))}%`
        : '';
    const rate = d.bytes_per_second > 0 ? ` · ${fmtRate(d.bytes_per_second)}` : '';
    return `${left}${pct}${rate}`;
  }
</script>

<div
  class="w-[460px] overflow-hidden rounded-md border border-line-2 bg-bg-1 font-sans text-ink-0"
  style:box-shadow="0 18px 48px rgb(0 0 0 / 0.5)"
  role="dialog"
  aria-label="Transfers"
>
  <!-- Header -->
  <header
    class="flex items-center justify-between border-b border-dashed border-line-1 bg-bg-2 px-3.5 py-3"
  >
    <div class="flex items-center gap-2">
      <span
        class="h-[14px] w-1 rounded-[1px]"
        style:background="var(--color-spool)"
      ></span>
      <MonoLabel size={10}>Transfers</MonoLabel>
    </div>
  </header>

  <!-- Scrollable body -->
  <div class="max-h-[520px] overflow-y-auto">
    <!-- ── Downloading section ────────────────────────────────────────── -->
    <div
      class="sticky top-0 z-[1] flex items-center gap-2 border-b border-dashed border-line-1 bg-bg-1 px-3.5 pb-1 pt-2.5"
    >
      <ArrowDown size={11} color="var(--color-spool)" />
      <MonoLabel size={9.5}>DOWNLOADING</MonoLabel>
      <span
        class="font-mono text-[9.5px] tracking-[0.08em] text-ink-3"
      >· {dlCount}</span>
    </div>

    {#if download && (download.status === 'starting' || download.status === 'transferring')}
      {@const dlCover = assetUrl(download.cover_image_path)}
      <div
        class="grid items-center gap-3 border-b border-dashed border-line-1 px-3.5 py-3"
        style:grid-template-columns="40px 1fr auto"
      >
        <!-- Peer-prefetched cover if it landed, else sleeve gradient. -->
        {#if dlCover}
          <img
            src={dlCover}
            alt={download.game_name}
            class="h-[56px] w-[40px] shrink-0 rounded-sm border border-line-1 object-cover"
          />
        {:else}
          <div
            class="h-[56px] w-[40px] shrink-0 overflow-hidden rounded-sm border border-line-1"
            style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
          ></div>
        {/if}

        <div class="min-w-0">
          <div class="mb-1 flex items-center gap-2">
            <span class="truncate text-[13px] font-medium" title={download.game_name}>
              {download.game_name}
            </span>
          </div>
          <CassetteProgress
            percent={dlPercent}
            accent="var(--color-spool)"
            label={dlLabel(download)}
            source={download.source_device_name}
            sourceKind="lan"
            dir="down"
          />
        </div>

        <div class="flex items-center">
          <button
            type="button"
            onclick={onCancelDownload}
            aria-label="Cancel download"
            title="Cancel"
            class="inline-flex h-6 w-6 items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-white/5 hover:text-ink-0"
          >
            <X size={13} />
          </button>
        </div>
      </div>
    {:else}
      <div
        class="font-mono border-b border-dashed border-line-1 px-3.5 py-4 text-[11.5px] tracking-[0.04em] text-ink-3"
      >
        No incoming transfers
      </div>
    {/if}

    <!-- ── Uploading section ─────────────────────────────────────────── -->
    <div
      class="sticky top-0 z-[1] flex items-center gap-2 border-b border-dashed border-line-1 bg-bg-1 px-3.5 pb-1 pt-2.5"
    >
      <ArrowUp size={11} color="var(--color-ok)" />
      <MonoLabel size={9.5}>UPLOADING</MonoLabel>
      <span
        class="font-mono text-[9.5px] tracking-[0.08em] text-ink-3"
      >· {ulCount}</span>
    </div>

    {#if uploads.length === 0}
      <div
        class="font-mono px-3.5 py-4 text-[11.5px] tracking-[0.04em] text-ink-3"
      >
        No outgoing transfers
      </div>
    {:else}
      {#each uploads as upload, i (upload.session_id)}
        {@const fresh = upload.last_seen_ago_secs < 2}
        {@const cover = coverFor?.(upload.game_id) ?? null}
        <div
          class="grid items-center gap-3 px-3.5 py-3"
          class:border-b={i !== uploads.length - 1}
          class:border-dashed={i !== uploads.length - 1}
          class:border-line-1={i !== uploads.length - 1}
          style:grid-template-columns="40px 1fr auto"
          style:opacity={upload.cancelled ? '0.6' : '1'}
        >
          <!-- Local game's cover if available, else sleeve gradient. -->
          {#if cover}
            <img
              src={cover}
              alt={upload.game_name}
              class="h-[56px] w-[40px] shrink-0 rounded-sm border border-line-1 object-cover"
            />
          {:else}
            <div
              class="h-[56px] w-[40px] shrink-0 overflow-hidden rounded-sm border border-line-1"
              style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
            ></div>
          {/if}

          <div class="min-w-0">
            <div class="mb-1 flex items-center gap-2">
              <span class="truncate text-[13px] font-medium" title={upload.game_name}>
                {upload.game_name}
              </span>
              {#if upload.cancelled}
                <span
                  class="font-mono rounded-[3px] px-1.5 py-0.5 text-[9.5px] tracking-[0.06em]"
                  style:background="rgba(244,182,108,0.10)"
                  style:color="var(--color-warn)"
                >CANCELLED</span>
              {/if}
            </div>
            <CassetteProgress
              percent={upload.cancelled ? 100 : fresh ? 100 : 50}
              accent={upload.cancelled
                ? 'var(--color-line-3)'
                : fresh
                  ? 'var(--color-ok)'
                  : 'var(--color-ink-3)'}
              label={upload.cancelled
                ? 'Cancelled — waiting for peer to notice'
                : fresh
                  ? `Streaming · seen ${upload.last_seen_ago_secs}s ago`
                  : `Idle · ${upload.last_seen_ago_secs}s ago`}
              source={upload.peer_addr}
              sourceKind="lan"
              dir="up"
            />
          </div>

          <div class="flex items-center">
            {#if !upload.cancelled}
              <button
                type="button"
                onclick={() => onCancelUpload?.(upload)}
                aria-label="Cancel this upload"
                title="Cancel upload"
                class="inline-flex h-6 w-6 items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-white/5 hover:text-ink-0"
              >
                <X size={13} />
              </button>
            {/if}
          </div>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Footer summary -->
  <div
    class="font-mono flex items-center justify-between border-t border-line-1 bg-bg-0 px-3.5 py-2.5 text-[10.5px] tracking-[0.04em] text-ink-2"
  >
    <span>
      {download && download.bytes_total > 0
        ? `${fmtBytes(download.bytes_done)} / ${fmtBytes(download.bytes_total)}`
        : `${dlCount + ulCount} active`}
    </span>
    <span class="text-ink-2">{fmtRate(totalSpeed)}</span>
  </div>
</div>
