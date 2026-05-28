<script lang="ts">
  /**
   * Title-bar transfer pill — two arms.
   *
   *   ┌──────────────┬──────────────┐
   *   │ ↓ N  ▭▭▭▭▭▭ │ ↑ N  ▭▭▭▭▭▭ │
   *   └──────────────┴──────────────┘
   *
   * Downloads (incoming) on the left in `--color-spool`, uploads
   * (outgoing) on the right in `--color-ok`. An idle arm dims to ~45%
   * so the eye reads the active direction instantly. Click toggles
   * the unified Transfers panel.
   */
  import { ArrowDown, ArrowUp } from '@lucide/svelte';

  interface Props {
    /** Number of in-flight downloads. */
    downloadCount: number;
    /** 0–100 aggregate progress across all downloads. */
    downloadPercent: number;
    /** Number of in-flight uploads. */
    uploadCount: number;
    /** 0–100 aggregate progress across all uploads. */
    uploadPercent: number;
    open?: boolean;
    onclick?: () => void;
  }

  let {
    downloadCount,
    downloadPercent,
    uploadCount,
    uploadPercent,
    open = false,
    onclick,
  }: Props = $props();

  const total = $derived(downloadCount + uploadCount);
</script>

<button
  type="button"
  {onclick}
  title={total > 0 ? `${total} transfer${total === 1 ? '' : 's'} in flight` : 'Transfers'}
  aria-label="Transfers"
  data-tauri-drag-region="false"
  class="inline-flex h-[22px] cursor-pointer items-stretch overflow-hidden rounded-sm border transition-colors"
  style:border-color={open ? 'var(--color-line-3)' : 'var(--color-line-2)'}
  style:background={open ? 'var(--color-bg-3)' : 'var(--color-bg-2)'}
>
  <!-- ↓ Downloads -->
  <span
    class="inline-flex items-center gap-1.5 px-[9px]"
    style:opacity={downloadCount === 0 ? '0.45' : '1'}
  >
    <ArrowDown
      size={11}
      color={downloadCount === 0 ? 'var(--color-ink-3)' : 'var(--color-spool)'}
    />
    <span
      class="font-mono min-w-[8px] text-center text-[10px] tracking-[0.08em] text-ink-1"
    >
      {downloadCount}
    </span>
    <span
      class="inline-block h-[3px] w-[22px] overflow-hidden rounded-[2px] bg-bg-0"
    >
      <span
        class="block h-full"
        style:width="{downloadCount === 0 ? 0 : downloadPercent}%"
        style:background="var(--color-spool)"
      ></span>
    </span>
  </span>

  <!-- Divider -->
  <span
    class="w-[1px] self-stretch"
    style:background={open ? 'var(--color-line-3)' : 'var(--color-line-2)'}
  ></span>

  <!-- ↑ Uploads -->
  <span
    class="inline-flex items-center gap-1.5 px-[9px]"
    style:opacity={uploadCount === 0 ? '0.45' : '1'}
  >
    <ArrowUp
      size={11}
      color={uploadCount === 0 ? 'var(--color-ink-3)' : 'var(--color-ok)'}
    />
    <span
      class="font-mono min-w-[8px] text-center text-[10px] tracking-[0.08em] text-ink-1"
    >
      {uploadCount}
    </span>
    <span
      class="inline-block h-[3px] w-[22px] overflow-hidden rounded-[2px] bg-bg-0"
    >
      <span
        class="block h-full"
        style:width="{uploadCount === 0 ? 0 : uploadPercent}%"
        style:background="var(--color-ok)"
      ></span>
    </span>
  </span>
</button>
