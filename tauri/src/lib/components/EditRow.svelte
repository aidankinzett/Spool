<script lang="ts">
  /**
   * One labelled row in the game editor: a 160px label + optional helper text on
   * the left, the control (passed as children) on the right. Shared by the
   * editor tabs and the Saves panel so the field layout lives in one place.
   */
  import type { Snippet } from 'svelte';

  let {
    label,
    helper = '',
    children,
  }: { label: string; helper?: string; children: Snippet } = $props();
</script>

<div
  class="grid items-start gap-4 border-b border-dashed border-line-1 py-2.5"
  style:grid-template-columns="160px 1fr"
>
  <div class="pt-1.5">
    <div class="text-[12.5px] font-medium text-ink-0">{label}</div>
    {#if helper}
      <div class="mt-0.5 text-[11px] leading-snug text-ink-2">{helper}</div>
    {/if}
  </div>
  <div>{@render children()}</div>
</div>
