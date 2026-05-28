<script lang="ts">
  /**
   * Catalog ID — the "SPL-NNNN" badge that appears on every game (the
   * cassette-shelf metaphor). Mono digits, hairline border, optional
   * cover-art accent tint.
   */
  let {
    id,
    accent,
  }: {
    id: string;
    /** Hex color from the game's cover art; tints the border + text. */
    accent?: string;
  } = $props();
</script>

<span
  class="font-mono inline-block whitespace-nowrap rounded-sm border px-1.5 py-px text-[10px] tracking-[0.1em]"
  style:color={accent ?? 'var(--color-ink-2)'}
  style:border-color={accent ? `${accent}55` : 'var(--color-line-2)'}
>
  {id}
</span>
