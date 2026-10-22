<script lang="ts">
  /**
   * Desktop achievement overlay — the page inside one `achievement-toast-<n>`
   * window (see `achievements/overlay.rs`). The window is transparent and
   * exactly the card's size, so the page background is cleared and the card
   * fills it. It slides in, holds, and slides out a little before the backend
   * closes the window.
   *
   *   ┌──────────────────────────────────────────────┐
   *   │ [icon]  ACHIEVEMENT UNLOCKED                  │
   *   │         Winner                                │
   *   │         Win one game. · Game name             │
   *   └──────────────────────────────────────────────┘
   */
  import { onMount } from 'svelte';
  import { Trophy } from '@lucide/svelte';
  import { api, assetUrl } from '$lib/api';
  import type { AchievementToast } from '$lib/types';

  /** Slide-in/out length; matches the CSS transition. */
  const ANIM_MS = 260;

  let toast = $state<AchievementToast | null>(null);
  let shown = $state(false);

  const iconSrc = $derived.by(() => {
    const icon = toast?.icon;
    if (!icon) return null;
    return icon.startsWith('http') ? icon : assetUrl(icon);
  });

  onMount(() => {
    // app.css paints html/body; clear it so only the card is drawn.
    document.documentElement.style.background = 'transparent';
    document.body.style.background = 'transparent';
    let outTimer: ReturnType<typeof setTimeout> | undefined;
    api
      .achievementToastPayload()
      .then((t) => {
        toast = t;
        if (!t) return;
        requestAnimationFrame(() => (shown = true));
        outTimer = setTimeout(() => (shown = false), Math.max(0, t.duration_ms - ANIM_MS - 100));
      })
      .catch((e) => console.error('[achievement-toast] payload failed:', e));
    return () => clearTimeout(outTimer);
  });
</script>

{#if toast}
  <div
    class="card flex h-screen w-screen items-center gap-3.5 overflow-hidden rounded-md border border-line-2 bg-bg-1 px-3.5"
    class:shown
    style:--anim="{ANIM_MS}ms"
  >
    <div
      class="flex h-[60px] w-[60px] shrink-0 items-center justify-center overflow-hidden rounded-sm border border-line-2 bg-bg-2 text-spool"
    >
      {#if iconSrc}
        <img src={iconSrc} alt="" class="h-full w-full object-cover" />
      {:else}
        <Trophy size={26} />
      {/if}
    </div>
    <div class="min-w-0 flex-1">
      <div class="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-2">
        Achievement unlocked
      </div>
      <div class="mt-0.5 truncate text-[15px] font-semibold text-ink-0">{toast.display_name}</div>
      <div class="truncate text-[11.5px] text-ink-2">
        {toast.description ? `${toast.description} · ${toast.game_name}` : toast.game_name}
      </div>
    </div>
  </div>
{/if}

<style>
  .card {
    transform: translateX(110%);
    opacity: 0;
    transition:
      transform var(--anim) ease-out,
      opacity var(--anim) ease-out;
  }
  .card.shown {
    transform: translateX(0);
    opacity: 1;
  }
  @media (prefers-reduced-motion: reduce) {
    .card {
      transform: none;
      transition: opacity var(--anim) linear;
    }
  }
</style>
