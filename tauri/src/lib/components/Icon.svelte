<script lang="ts">
  /**
   * Line icon component — looks up `name` in the icons dictionary and renders
   * a 16×16 SVG using `currentColor`. Caller controls colour via Tailwind
   * `text-*` classes on the parent element.
   *
   * Pass `class` to extend styling (e.g. size overrides via Tailwind).
   */
  import { icons, type IconName } from './icons';

  let {
    name,
    size = 14,
    stroke = 1.5,
    class: className = '',
  }: {
    name: IconName;
    size?: number;
    stroke?: number;
    class?: string;
  } = $props();

  const def = $derived(icons[name]);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  class="block shrink-0 {className}"
>
  {#if def.fill}
    <path d={def.d} fill="currentColor" stroke="none" />
  {:else}
    <path d={def.d} />
  {/if}
</svg>
