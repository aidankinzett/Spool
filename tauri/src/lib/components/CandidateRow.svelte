<script lang="ts">
  /**
   * One row in the Add Game candidate list.
   *
   *   ○ Elden Ring: Nightreign                [98%] [steam] [gog] [☁] [1]
   *     📁 %APPDATA%/EldenRingNightreign/save
   *
   *   ● = picked (oxide radio + name bold + tinted background)
   *   numeric keyboard hint (1-9) on the right fades in on hover/pick
   */
  import { Folder } from '@lucide/svelte';
  import type { SearchCandidate } from '$lib/types';

  let {
    cand,
    index,
    picked,
    onpick,
  }: {
    cand: SearchCandidate;
    index: number;
    picked: boolean;
    onpick: () => void;
  } = $props();

  let hover = $state(false);

  // Match score: hidden when high-confidence (the design's rule).
  const showScore = $derived(cand.score < 0.95);
  // Convert 0-1 → 0-100 for display.
  const scorePct = $derived(Math.round(cand.score * 100));
  const scoreColor = $derived(
    cand.score >= 0.75
      ? 'var(--color-ink-2)'
      : cand.score >= 0.6
        ? 'var(--color-warn)'
        : 'var(--color-ink-3)',
  );
</script>

<button
  type="button"
  onclick={onpick}
  onmouseenter={() => (hover = true)}
  onmouseleave={() => (hover = false)}
  class="grid w-full cursor-pointer items-center gap-x-3 gap-y-1 border-l-2 px-3.5 py-2.5 text-left transition-colors"
  style:grid-template-columns="20px 1fr auto auto"
  style:background={picked
    ? 'rgb(215 201 160 / 0.08)'
    : hover
      ? 'var(--color-bg-2)'
      : 'transparent'}
  style:border-left-color={picked ? 'var(--color-spool)' : 'transparent'}
>
  <!-- Radio (spans both rows) -->
  <span
    class="inline-flex size-4 shrink-0 items-center justify-center rounded-full border-[1.5px]"
    style:grid-row="1 / span 2"
    style:border-color={picked ? 'var(--color-spool)' : 'var(--color-line-3)'}
  >
    {#if picked}
      <span class="size-[7px] rounded-full" style:background="var(--color-spool)"></span>
    {/if}
  </span>

  <!-- Name -->
  <span
    class="min-w-0 truncate text-[13.5px]"
    style:font-weight={picked ? 500 : 400}
    style:color={picked ? 'var(--color-ink-0)' : 'var(--color-ink-1)'}
  >
    {cand.name}
  </span>

  <!-- Right cluster: score + store badges + cloud -->
  <span class="inline-flex shrink-0 items-center gap-1.5">
    {#if showScore}
      <span
        title={`Match confidence ${scorePct}%`}
        class="font-mono px-0.5 text-[10px] tracking-[0.04em]"
        style:color={scoreColor}
      >
        {scorePct}%
      </span>
    {/if}
    {#if cand.steam_id != null}
      <span
        title={`Steam · ${cand.steam_id}`}
        class="inline-flex items-center gap-1 rounded-sm border border-line-2 bg-white/5 px-1.5 py-0.5 text-ink-2"
      >
        <!-- Custom Steam glyph — Lucide doesn't ship one. -->
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" class="block shrink-0">
          <circle cx="6" cy="6" r="5.4" stroke="currentColor" stroke-width="1.1" />
          <circle cx="8.2" cy="4.2" r="1.5" stroke="currentColor" stroke-width="1" />
          <circle cx="4" cy="8.4" r="1.1" stroke="currentColor" stroke-width="1" />
          <line x1="6.7" y1="4.7" x2="4.6" y2="7.9" stroke="currentColor" stroke-width="0.9" />
        </svg>
      </span>
    {/if}
    {#if cand.gog_id != null}
      <span
        title="GOG"
        class="font-mono inline-flex size-[18px] items-center justify-center rounded-sm border border-line-2 bg-white/5 text-[8.5px] font-bold text-ink-2"
      >
        GOG
      </span>
    {/if}
  </span>

  <!-- Keyboard hint -->
  <span
    class="font-mono w-3 text-right text-[10px] tracking-[0.04em] text-ink-3 transition-opacity"
    style:grid-row="1 / span 2"
    style:opacity={hover || picked ? 0.85 : 0}
  >
    {index < 9 ? index + 1 : ''}
  </span>

  <!-- Save path -->
  <span
    class="font-mono inline-flex min-w-0 items-center gap-1.5 truncate text-[10.5px] tracking-[0.02em] text-ink-3"
    style:grid-column="2 / 4"
  >
    <Folder size={12} class="shrink-0" />
    {cand.save_path ?? '— no save info'}
  </span>
</button>
