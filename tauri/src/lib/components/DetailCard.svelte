<script lang="ts">
  /**
   * Generic bordered card used inside the game detail pane.
   *
   *   ┌─[ACCENT] TITLE ──────────────────────────── [action]┐
   *   │ children …                                          │
   *   └─────────────────────────────────────────────────────┘
   *
   * The header has a vertical accent strip (per-game cover-art tint
   * eventually; brand spool for now) and a mono-eyebrow title. The
   * optional `action` snippet renders right-aligned in the header for
   * inline buttons (e.g. "Back up" / "Restore…" on the Saves card).
   */
  import MonoLabel from './MonoLabel.svelte';

  let {
    title,
    accent = 'var(--color-spool)',
    action,
    children,
  }: {
    title: string;
    /** Hex / CSS color for the title accent strip. Defaults to brand spool. */
    accent?: string;
    action?: import('svelte').Snippet;
    children: import('svelte').Snippet;
  } = $props();
</script>

<section class="min-w-0 overflow-hidden rounded-md border border-line-1 bg-bg-1">
  <header
    class="flex items-center justify-between gap-2.5 border-b border-dashed border-line-1 bg-bg-2 px-3.5 py-2.5"
  >
    <div class="flex items-center gap-2">
      <span class="h-3.5 w-1 rounded-[1px]" style:background={accent}></span>
      <MonoLabel size={10}>{title}</MonoLabel>
    </div>
    {#if action}{@render action()}{/if}
  </header>
  <div class="p-3.5">
    {@render children()}
  </div>
</section>
