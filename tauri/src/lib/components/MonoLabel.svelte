<script lang="ts">
  /**
   * Mono eyebrow label — the wide-tracked, uppercase, monospaced caption
   * used throughout the design (toolbars, section headers, metadata rows).
   *
   * Defaults match the "10/0.12em/uppercase" recipe from the design system.
   * Override colour by wrapping in or composing with a `text-*` class.
   */
  let {
    size = 10,
    tracking = '0.12em',
    class: className = '',
    children,
  }: {
    size?: number;
    tracking?: string;
    class?: string;
    children: import('svelte').Snippet;
  } = $props();
</script>

<span
  class="font-mono uppercase text-ink-2 {className}"
  style:font-size="{size}px"
  style:letter-spacing={tracking}
>
  {@render children()}
</span>
