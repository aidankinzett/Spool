<script lang="ts">
  /**
   * Bordered group of settings — eyebrow header on top, rows below
   * separated by hairlines. Matches the cassette-shelf "drawer" feel:
   * sharp corners, hairline borders, mono eyebrow.
   */
  import MonoLabel from './MonoLabel.svelte';

  let {
    title,
    children,
  }: {
    title: string;
    children: import('svelte').Snippet;
  } = $props();
</script>

<section class="rounded-md border border-line-1 bg-bg-1">
  <header class="border-b border-line-1 px-4 py-2.5">
    <MonoLabel size={11}>{title}</MonoLabel>
  </header>
  <div class="divide-y divide-line-1">
    {@render children()}
  </div>
</section>
