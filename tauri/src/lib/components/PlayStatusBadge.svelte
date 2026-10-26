<script lang="ts">
  /**
   * A game's play status as a round icon chip in the status colour. `dot` fits
   * the sidebar's small cover thumbs, `sm` overlays shelf / home tiles, and `md`
   * (optionally with the label) sits in detail headers. Renders nothing when
   * the game has no status.
   */
  import { playStatusOf } from '$lib/playStatus';

  let {
    game,
    size = 'sm',
    label = false,
  }: {
    game: { play_status: string | null };
    size?: 'dot' | 'sm' | 'md';
    label?: boolean;
  } = $props();

  const def = $derived(playStatusOf(game));
  const px = $derived(size === 'dot' ? 14 : size === 'sm' ? 22 : 26);
  const iconPx = $derived(size === 'dot' ? 9 : size === 'sm' ? 13 : 15);
</script>

{#if def}
  {@const Icon = def.icon}
  {#if label}
    <span
      class="inline-flex items-center gap-1.5 rounded-full border py-0.5 pl-1 pr-2.5 font-medium"
      style:font-size="var(--text-sm)"
      style:color={def.color}
      style:border-color="color-mix(in srgb, {def.color} 40%, transparent)"
      style:background="color-mix(in srgb, {def.color} 12%, transparent)"
      title={def.label}
    >
      <Icon size={iconPx} strokeWidth={2.25} />
      {def.label}
    </span>
  {:else}
    <span
      class="inline-flex shrink-0 items-center justify-center rounded-full border"
      style:width="{px}px"
      style:height="{px}px"
      style:color={def.color}
      style:background="color-mix(in srgb, var(--color-bg-0) 78%, {def.color})"
      style:border-color="color-mix(in srgb, {def.color} 55%, transparent)"
      title={def.label}
      aria-label={def.label}
    >
      <Icon size={iconPx} strokeWidth={2.5} />
    </span>
  {/if}
{/if}
