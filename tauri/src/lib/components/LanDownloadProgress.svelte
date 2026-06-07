<script lang="ts">
  /**
   * Live progress for an in-flight LAN install: a bar plus a
   * `current_file · rate · percent` line. Shared by the game-detail Download
   * button and the peer-drill-down popover, which only differ cosmetically
   * (bar height, fill colour, text size) — those are props.
   */
  import { fmtRate } from '$lib/format';
  import type { DownloadProgress } from '$lib/types';

  let {
    download,
    accent = 'var(--color-spool)',
    barClass = 'h-1',
    metaClass = 'text-[9.5px]',
  }: {
    download: DownloadProgress;
    /** Fill colour of the progress bar. */
    accent?: string;
    /** Tailwind height class for the bar track. */
    barClass?: string;
    /** Tailwind text-size class for the file / rate / percent line. */
    metaClass?: string;
  } = $props();

  const pct = $derived(
    download.bytes_total > 0
      ? Math.min(100, (download.bytes_done / download.bytes_total) * 100)
      : 0,
  );
</script>

<div class="flex flex-col gap-1">
  <div class="{barClass} w-full overflow-hidden rounded-full bg-bg-2">
    <div
      class="h-full transition-[width] duration-150 ease-out"
      style:width="{pct}%"
      style:background={accent}
    ></div>
  </div>
  <div class="font-mono flex justify-between gap-2 {metaClass} tracking-[0.04em] text-ink-3">
    <span class="truncate" title={download.current_file}>
      {download.current_file || '…'}
    </span>
    <span class="shrink-0 whitespace-nowrap">
      {fmtRate(download.bytes_per_second)}
      {#if download.bytes_total > 0}
        · {Math.round(pct)}%
      {/if}
    </span>
  </div>
</div>
