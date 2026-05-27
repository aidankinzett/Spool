<script lang="ts">
  /**
   * One row inside a SettingsCard.
   *
   *   ┌───────────────────────────────────────────────────────────────┐
   *   │ Title                                       [right-aligned    │
   *   │ Subtitle text                                control]         │
   *   │ ─ optional below-control extras ────────────────────────────  │
   *   └───────────────────────────────────────────────────────────────┘
   *
   * The control snippet is right-aligned for the row's hero state (e.g.
   * a toggle or short input). The `extras` snippet renders full-width
   * below, used when a row needs a wide field (e.g. a long path) plus
   * inline buttons (Auto-detect, Browse…).
   */
  let {
    title,
    subtitle,
    control,
    extras,
  }: {
    title: string;
    subtitle?: string;
    control?: import('svelte').Snippet;
    extras?: import('svelte').Snippet;
  } = $props();
</script>

<div class="flex flex-col gap-2.5 px-4 py-3.5">
  <div class="flex items-start justify-between gap-4">
    <div class="min-w-0 flex-1">
      <div class="text-[13px] font-medium text-ink-0">{title}</div>
      {#if subtitle}
        <div class="mt-0.5 text-[12px] text-ink-2">{subtitle}</div>
      {/if}
    </div>
    {#if control}
      <div class="flex shrink-0 items-center gap-2">{@render control()}</div>
    {/if}
  </div>
  {#if extras}
    <div class="flex items-center gap-2">{@render extras()}</div>
  {/if}
</div>
