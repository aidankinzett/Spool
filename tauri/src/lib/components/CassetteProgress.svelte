<script lang="ts">
  /**
   * Cassette-tape progress bar.
   *
   * Solid fill with a subtle glow shadow on top of a dark track, with
   * reel-tick marks underneath. The footer row shows a label on the
   * left and (optionally) a direction arrow + peer string on the
   * right. Used inside the Transfers panel and the in-detail
   * downloading block.
   */
  import { ArrowDown, ArrowUp, Cloud, Wifi } from '@lucide/svelte';

  interface Props {
    /** 0–100. Clamped. */
    percent: number;
    /** CSS colour for the fill bar and its glow halo. */
    accent: string;
    /** Left-hand status label — bytes / speed / ETA / verb. */
    label: string;
    /** Optional peer string on the right — `Workshop · Desktop`, `TorBox · debrid`, … */
    source?: string;
    /** Categorises the peer for icon selection. */
    sourceKind?: 'lan' | 'torbox' | 'cloud' | null;
    /** Up = uploading from us, down = downloading to us. */
    dir?: 'up' | 'down';
    /** Bar height in pixels. */
    height?: number;
  }

  let {
    percent,
    accent,
    label,
    source,
    sourceKind = null,
    dir = 'down',
    height = 6,
  }: Props = $props();

  const clamped = $derived(Math.max(0, Math.min(100, percent)));
  const dirColor = $derived(dir === 'up' ? 'var(--color-ok)' : 'var(--color-spool)');
  const sourceColor = $derived(
    sourceKind === 'lan'
      ? 'var(--color-ok)'
      : sourceKind === 'torbox' || sourceKind === 'cloud'
        ? 'var(--color-info)'
        : 'var(--color-ink-3)',
  );
</script>

<div>
  <!-- Bar -->
  <div class="relative mb-1.5" style:height="{height}px">
    <!-- Track -->
    <div class="absolute inset-0 rounded-[1px] bg-bg-0"></div>
    <!-- Fill -->
    <div
      class="absolute left-0 top-0 bottom-0 rounded-[1px] transition-[width] duration-150 ease-out"
      style:width="{clamped}%"
      style:background={accent}
      style:box-shadow="0 0 8px {accent}66"
    ></div>
    <!-- Reel tick marks -->
    <div
      class="pointer-events-none absolute left-0 right-0 h-[2px]"
      style:bottom="-3px"
      style:background-image="repeating-linear-gradient(to right, var(--color-line-2) 0 1px, transparent 1px 12.5%)"
    ></div>
  </div>

  <!-- Label + source row -->
  <div
    class="font-mono flex items-center justify-between gap-2 text-[10px] text-ink-2 tracking-[0.04em]"
  >
    <span class="min-w-0 truncate">{label}</span>
    {#if source}
      <span class="flex shrink-0 items-center gap-1.5 text-ink-3">
        {#if dir === 'up'}
          <ArrowUp size={9} color={dirColor} />
        {:else}
          <ArrowDown size={9} color={dirColor} />
        {/if}
        {#if sourceKind === 'lan'}
          <Wifi size={9} color={sourceColor} />
        {:else if sourceKind === 'torbox' || sourceKind === 'cloud'}
          <Cloud size={9} color={sourceColor} />
        {/if}
        {source}
      </span>
    {/if}
  </div>
</div>
