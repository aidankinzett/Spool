<script lang="ts">
  /**
   * Achievements card in the game detail pane.
   *
   *   ┌─[accent] ACHIEVEMENTS ─────────────────────── 12 / 40┐
   *   │ ████████░░░░░░░░░░░░  30%                             │
   *   │ [icon] Winner                         3 Jun 2026      │
   *   │        Win one game.                                  │
   *   │ [icon] Hidden achievement             4.2% of players │
   *   │ Show all 40                                           │
   *   └───────────────────────────────────────────────────────┘
   *
   * Data comes from `get_game_achievements`, which scans the game's
   * Steam-emulator stats file and merges it with the schema. Unlocked entries
   * come first (newest first), then locked ones (most common first). Hidden
   * locked achievements show no name or description. The card is omitted when
   * a game has neither a schema nor any unlocks.
   */
  import { listen } from '@tauri-apps/api/event';
  import { Lock, Trophy } from '@lucide/svelte';
  import { api, assetUrl } from '$lib/api';
  import type { AchievementUnlockedEvent, AchievementView, GameAchievements, GameEntry } from '$lib/types';
  import DetailCard from './DetailCard.svelte';
  import MonoLabel from './MonoLabel.svelte';

  let {
    game,
    accent = 'var(--color-spool)',
  }: {
    game: GameEntry;
    accent?: string;
  } = $props();

  /** Rows shown before "Show all". */
  const COLLAPSED = 6;

  /** The last response, tagged with the game it's for, so switching games
   *  never shows the previous game's list while the new one loads. */
  let loaded = $state<{ id: string; data: GameAchievements } | null>(null);
  let expanded = $state(false);
  /** Bumped by an `achievement:unlocked` event for this game to trigger a reload. */
  let unlockTick = $state(0);

  // The library store replaces every GameEntry object on each
  // `library:changed`, so the effects below key on these primitives, not on
  // `game`, or they'd rerun (rescan, re-subscribe, collapse the list) on every
  // unrelated refresh.
  const gameId = $derived(game.id);
  const lastPlayed = $derived(game.last_played_at);
  const data = $derived(loaded?.id === gameId ? loaded.data : null);

  // Reload when the selected game changes, after a session ends (last_played_at
  // moves), and on a live unlock for this game.
  $effect(() => {
    const id = gameId;
    void lastPlayed;
    void unlockTick;
    let stale = false;
    api
      .getGameAchievements(id)
      .then((d) => {
        if (!stale) loaded = { id, data: d };
      })
      .catch(() => {
        if (!stale) loaded = null;
      });
    return () => {
      stale = true;
    };
  });

  $effect(() => {
    const id = gameId;
    expanded = false;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<AchievementUnlockedEvent>('achievement:unlocked', (e) => {
      if (e.payload.game_id === id) unlockTick++;
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  const sorted = $derived.by(() => {
    if (!data) return [];
    const unlocked = data.items
      .filter((a) => a.unlocked_at !== null)
      .sort((a, b) => (b.unlocked_at ?? 0) - (a.unlocked_at ?? 0));
    const locked = data.items
      .filter((a) => a.unlocked_at === null)
      .sort((a, b) => (b.global_percent ?? -1) - (a.global_percent ?? -1));
    return [...unlocked, ...locked];
  });

  const shown = $derived(expanded ? sorted : sorted.slice(0, COLLAPSED));
  const pct = $derived(data && data.total > 0 ? Math.round((data.unlocked / data.total) * 100) : 0);
  // Without a schema only unlocked entries are known, so there's no total.
  const hasTotal = $derived(data?.schema_source != null);

  function iconSrc(a: AchievementView): string | null {
    const src = a.unlocked_at !== null ? a.icon : (a.icon_gray ?? a.icon);
    if (!src) return null;
    return src.startsWith('http') ? src : assetUrl(src);
  }

  function fmtDate(secs: number): string {
    return new Date(secs * 1000).toLocaleDateString(undefined, {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    });
  }

  function fmtPercent(p: number): string {
    return `${p < 10 ? p.toFixed(1) : Math.round(p)}% of players`;
  }
</script>

{#if data && data.items.length > 0}
  <DetailCard title="ACHIEVEMENTS" {accent}>
    {#snippet action()}
      <MonoLabel size={9} class="text-ink-3 tabular-nums">
        {hasTotal ? `${data!.unlocked} / ${data!.total}` : `${data!.unlocked} UNLOCKED`}
      </MonoLabel>
    {/snippet}

    {#if hasTotal}
      <div class="mb-3 flex items-center gap-2.5">
        <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-bg-3">
          <div class="h-full rounded-full" style:width="{pct}%" style:background={accent}></div>
        </div>
        <span class="font-mono text-[11px] text-ink-2 tabular-nums">{pct}%</span>
      </div>
    {/if}

    <ul class="m-0 flex list-none flex-col gap-2.5 p-0">
      {#each shown as a (a.api_name)}
        {@const locked = a.unlocked_at === null}
        {@const secret = locked && a.hidden}
        {@const src = iconSrc(a)}
        <li class="flex items-center gap-3" class:opacity-60={locked}>
          <div
            class="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-sm border border-line-2 bg-bg-2 text-ink-3"
          >
            {#if src && !secret}
              <img {src} alt="" class="h-full w-full object-cover" class:grayscale={locked} />
            {:else if locked}
              <Lock size={14} />
            {:else}
              <Trophy size={14} />
            {/if}
          </div>
          <div class="min-w-0 flex-1">
            <div class="truncate text-[13px] font-medium text-ink-0">
              {secret ? 'Hidden achievement' : a.display_name}
            </div>
            {#if !secret && a.description}
              <div class="truncate text-[11.5px] text-ink-2">{a.description}</div>
            {/if}
          </div>
          <div class="shrink-0 text-right text-[11px] text-ink-3 tabular-nums">
            {#if a.unlocked_at !== null}
              {fmtDate(a.unlocked_at)}
            {:else if a.global_percent !== null}
              {fmtPercent(a.global_percent)}
            {/if}
          </div>
        </li>
      {/each}
    </ul>

    {#if sorted.length > COLLAPSED}
      <button
        type="button"
        class="mt-3 cursor-pointer border-0 bg-transparent p-0 text-[12px] text-ink-2 hover:text-ink-0"
        onclick={() => (expanded = !expanded)}
      >
        {expanded ? 'Show fewer' : `Show all ${sorted.length}`}
      </button>
    {/if}

    {#if !hasTotal}
      <p class="mb-0 mt-3 text-[11.5px] text-ink-3">
        Add a Steam Web API key in Settings → Achievements to see names, icons, and the full list.
      </p>
    {/if}
  </DetailCard>
{/if}
