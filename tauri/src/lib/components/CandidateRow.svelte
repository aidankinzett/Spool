<script lang="ts">
  /**
   * One row in the Add Game candidate list.
   *
   *   ○ Elden Ring: Nightreign                              [98%]
   *     📁 %APPDATA%/EldenRingNightreign/save
   *
   *   ● = picked (oxide radio + name bold + tinted background)
   *   Match % shows only when confidence is below 95%.
   */
  import { Folder } from '@lucide/svelte';
  import type { SearchCandidate } from '$lib/types';

  let {
    cand,
    picked,
    onpick,
  }: {
    cand: SearchCandidate;
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
  class="grid w-full cursor-pointer items-center gap-x-2.5 gap-y-0.5 border-l-2 px-3 py-1.5 text-left transition-colors"
  style:grid-template-columns="20px 1fr auto"
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

  <!-- Match score only -->
  <span class="inline-flex shrink-0 items-center">
    {#if showScore}
      <span
        title={`Match confidence ${scorePct}%`}
        class="font-mono px-0.5 text-[10px] tracking-[0.04em]"
        style:color={scoreColor}
      >
        {scorePct}%
      </span>
    {/if}
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
