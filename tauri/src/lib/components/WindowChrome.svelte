<script lang="ts">
  /**
   * Custom window chrome — same on Win / Linux / macOS. No OS impersonation.
   *
   *   ┌─────────────────────────────────────────────────────────────────┐
   *   │ [Spool mark] SPOOL / SUB   …center children…       [_]  [□]  [×] │
   *   └─────────────────────────────────────────────────────────────────┘
   *
   * Renders as a 36px drag-region strip. Window controls call Tauri APIs.
   * Children render in the middle (catalog id, peer pill, search, etc.).
   *
   * Requires the Tauri window to be created with `decorations: false`.
   */
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import SpoolMark from './SpoolMark.svelte';
  import MonoLabel from './MonoLabel.svelte';
  import Icon from './Icon.svelte';

  let {
    sub,
    accent,
    children,
  }: {
    /** Sub-section label after the SPOOL wordmark, e.g. "SETTINGS". */
    sub?: string;
    /** Tape-strip colour on the Spool mark — typically the cover-art accent. */
    accent?: string;
    /** Center content (catalog id, search, peer pill, etc.). */
    children?: import('svelte').Snippet;
  } = $props();

  const win = getCurrentWindow();

  async function minimize() {
    await win.minimize();
  }
  async function toggleMaximize() {
    await win.toggleMaximize();
  }
  async function close() {
    await win.close();
  }
</script>

<div
  data-tauri-drag-region="deep"
  class="flex shrink-0 items-center gap-3 border-b border-line-1 bg-black/30 pl-3.5"
  style:height="var(--chrome-h)"
>
  <SpoolMark size={18} color="var(--color-ink-1)" tape={accent ?? 'var(--color-spool)'} />
  <MonoLabel size={10.5}>SPOOL</MonoLabel>
  {#if sub}
    <span class="text-[10px] text-ink-3">/</span>
    <MonoLabel size={10.5} class="text-ink-1">{sub}</MonoLabel>
  {/if}

  <div class="flex-1">
    {#if children}{@render children()}{/if}
  </div>

  <!-- Window controls — full-height buttons (Chrome-style hit targets).
       Drag region opts out via the inner attribute. -->
  <button
    data-tauri-drag-region="false"
    aria-label="Minimize"
    onclick={minimize}
    class="inline-flex h-full w-12 cursor-pointer items-center justify-center text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
  >
    <Icon name="winMin" size={16} stroke={1.3} />
  </button>
  <button
    data-tauri-drag-region="false"
    aria-label="Maximize"
    onclick={toggleMaximize}
    class="inline-flex h-full w-12 cursor-pointer items-center justify-center text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
  >
    <Icon name="winMax" size={14} stroke={1.3} />
  </button>
  <button
    data-tauri-drag-region="false"
    aria-label="Close"
    onclick={close}
    class="inline-flex h-full w-12 cursor-pointer items-center justify-center text-ink-2 transition-colors hover:bg-bad/80 hover:text-white"
  >
    <Icon name="winClose" size={14} stroke={1.3} />
  </button>
</div>
