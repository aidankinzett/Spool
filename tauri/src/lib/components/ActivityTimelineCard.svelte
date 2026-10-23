<script lang="ts">
  /**
   * Activity timeline under the ABOUT card: play sessions and achievement
   * unlocks grouped by day, newest first.
   *
   *   ┌─[accent] ACTIVITY · TIMELINE ──────────────────────────┐
   *   │ TODAY                                                   │
   *   │ ● Played 1h 20m                                         │
   *   │ ● Unlocked 2 achievements                               │
   *   │     [icon] Winner                                       │
   *   │     [icon] Speedrunner                                  │
   *   │ 3 JUN 2026                                              │
   *   │ ● Played 45m                            [▣ Steam Deck]  │
   *   │ Show more                                               │
   *   └─────────────────────────────────────────────────────────┘
   *
   * Sessions come from `play_sessions` (every device); unlocks are each
   * achievement's first unlock from `get_game_achievements`. Grouping is in
   * `buildTimeline`.
   */
  import { Monitor, Play, Trophy } from '@lucide/svelte';
  import { achievementIconSrc, buildTimeline } from '$lib/activity';
  import { absDate, fmtPlaytime } from '$lib/format';
  import type { AchievementView, GameAchievements, PlaySession } from '$lib/types';
  import DetailCard from './DetailCard.svelte';
  import MonoLabel from './MonoLabel.svelte';

  let {
    gameId,
    sessions,
    data,
    deviceId,
    accent = 'var(--color-spool)',
  }: {
    gameId: string;
    sessions: PlaySession[];
    data: GameAchievements | null;
    deviceId: string | null;
    accent?: string;
  } = $props();

  /** Days shown at first, and added per "Show more". */
  const FIRST = 5;
  const MORE = 10;
  /** Unlocks listed by name in one entry; the rest show as an icon strip. */
  const NAMED = 3;
  const DAY_MS = 24 * 60 * 60 * 1000;

  let limit = $state(FIRST);
  $effect(() => {
    void gameId;
    limit = FIRST;
  });

  const days = $derived(buildTimeline(sessions, data?.items ?? [], deviceId));
  const shown = $derived(days.slice(0, limit));

  function dayLabel(day: number): string {
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const diff = Math.round((today - day) / DAY_MS);
    if (diff === 0) return 'TODAY';
    if (diff === 1) return 'YESTERDAY';
    return absDate(new Date(day).toISOString()).toUpperCase();
  }

  function unlockLabel(items: AchievementView[]): string {
    return `Unlocked ${items.length} achievement${items.length === 1 ? '' : 's'}`;
  }
</script>

{#snippet icon(a: AchievementView, size: string)}
  {@const src = achievementIconSrc(a)}
  <div
    class="flex shrink-0 items-center justify-center overflow-hidden rounded-sm border border-line-2 bg-bg-2 text-ink-3 {size}"
    title={a.display_name}
  >
    {#if src}
      <img {src} alt="" class="h-full w-full object-cover" />
    {:else}
      <Trophy size={12} />
    {/if}
  </div>
{/snippet}

{#snippet device(name: string)}
  <span
    class="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-sm border border-line-2 bg-bg-2 px-2 py-0.5 text-[10.5px] text-ink-1"
  >
    <Monitor size={11} style="color: {accent}" />
    {name}
  </span>
{/snippet}

{#if days.length > 0}
  <DetailCard title="ACTIVITY · TIMELINE" {accent}>
    <div class="flex flex-col gap-4">
      {#each shown as d (d.day)}
        <div>
          <MonoLabel size={9} class="text-ink-3">{dayLabel(d.day)}</MonoLabel>
          <ul class="m-0 mt-2 flex list-none flex-col gap-3 border-l border-line-2 p-0 pl-4">
            {#each d.entries as e (e.kind === 'session' ? e.session.session_id : `u:${e.deviceName ?? ''}`)}
              <li class="relative">
                <span
                  class="absolute left-[-17px] top-[5px] h-[9px] w-[9px] rounded-full border-2 border-bg-1"
                  style:background={e.kind === 'unlocks' ? accent : 'var(--color-ink-3)'}
                ></span>
                {#if e.kind === 'session'}
                  <div class="flex items-center justify-between gap-2.5">
                    <div class="flex min-w-0 items-center gap-2 text-[12.5px] text-ink-1">
                      <Play size={12} class="shrink-0 text-ink-3" />
                      <span class="truncate">Played {fmtPlaytime(e.minutes)}</span>
                    </div>
                    {#if !e.isThisDevice && e.session.device_name}
                      {@render device(e.session.device_name)}
                    {/if}
                  </div>
                {:else}
                  <div class="flex items-center justify-between gap-2.5">
                    <div class="flex min-w-0 items-center gap-2 text-[12.5px] text-ink-0">
                      <Trophy size={12} class="shrink-0" style="color: {accent}" />
                      <span class="truncate">{unlockLabel(e.items)}</span>
                    </div>
                    {#if e.deviceName}
                      {@render device(e.deviceName)}
                    {/if}
                  </div>
                  {#if e.items.length <= NAMED}
                    <div class="mt-2 flex flex-col gap-1.5">
                      {#each e.items as a (a.api_name)}
                        <div class="flex min-w-0 items-center gap-2.5">
                          {@render icon(a, 'h-7 w-7')}
                          <div class="min-w-0">
                            <div class="truncate text-[12px] text-ink-0">{a.display_name}</div>
                            {#if a.description}
                              <div class="truncate text-[11px] text-ink-3">{a.description}</div>
                            {/if}
                          </div>
                        </div>
                      {/each}
                    </div>
                  {:else}
                    <div class="mt-2 flex flex-wrap gap-1.5">
                      {#each e.items as a (a.api_name)}
                        {@render icon(a, 'h-8 w-8')}
                      {/each}
                    </div>
                  {/if}
                {/if}
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    </div>

    {#if days.length > limit}
      <button
        type="button"
        class="mt-3.5 cursor-pointer border-0 bg-transparent p-0 text-[12px] text-ink-2 hover:text-ink-0"
        onclick={() => (limit += MORE)}
      >
        Show more
      </button>
    {/if}
  </DetailCard>
{/if}
