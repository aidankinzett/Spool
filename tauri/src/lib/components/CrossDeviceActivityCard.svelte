<script lang="ts">
  /**
   * Cross-device playtime + activity card — sits above the ENTRY · DETAILS
   * card in the game detail pane. Recreates the "ACTIVITY · CROSS-DEVICE"
   * design (spool/project/allgames detail_all.jsx → CrossDeviceCard +
   * pieces.jsx → ActivityTimeline).
   *
   *   ┌─[accent] ACTIVITY · CROSS-DEVICE ───── SYNCED · N DEVICES┐
   *   │ TOTAL PLAYTIME            LAST PLAYED                     │
   *   │ 149h 12m                  [▣ Studio PC · 2d ago]         │
   *   │ 122 sessions                                             │
   *   │ ▁▂▅▃▆▇▄▆██▇█▆▄  (weekly bars, last active week glows)     │
   *   │ 14 WK AGO                                          NOW    │
   *   └──────────────────────────────────────────────────────────┘
   *
   * Totals (playtime, last-played time) come from the authoritative GameEntry
   * aggregate; the session *count*, device breakdown, last-played *device*, and
   * the weekly bars are derived from the per-session history (`play_sessions`,
   * via api.listPlaySessions). The bars are a single-accent weekly *total* — not
   * a per-device stack — matching the design.
   */
  import { Monitor } from '@lucide/svelte';
  import { api } from '$lib/api';
  import type { GameEntry, PlaySession } from '$lib/types';
  import { fmtPlaytime, relDate } from '$lib/format';
  import DetailCard from './DetailCard.svelte';
  import MonoLabel from './MonoLabel.svelte';

  let {
    game,
    accent = 'var(--color-spool)',
  }: {
    game: GameEntry;
    /** Per-game accent colour (cover-art tint). Tints the bars + chip glyph. */
    accent?: string;
  } = $props();

  /** Weeks shown on the X axis (matches the design's 14-bar timeline). */
  const WEEKS = 14;
  const WEEK_MS = 7 * 24 * 60 * 60 * 1000;

  let sessions = $state<PlaySession[]>([]);

  // Reload the per-session history when the selected game changes, AND when this
  // game's aggregate moves — a session just ended (or a peer's session was folded
  // in) bumps playtime_minutes / last_played_at and re-emits library:changed, but
  // not game_name. Depending on those fields keeps the session count, device
  // chip, and weekly bars in step with the total playtime instead of going stale
  // until the game is reselected. Best-effort: a failure leaves the timeline
  // empty (totals still render). (#270)
  $effect(() => {
    const name = game.game_name;
    void game.last_played_at;
    void game.playtime_minutes;
    let stale = false;
    api
      .listPlaySessions(name)
      .then((s) => {
        if (!stale) sessions = s ?? [];
      })
      .catch(() => {
        if (!stale) sessions = [];
      });
    return () => {
      stale = true;
    };
  });

  const totalSessions = $derived(sessions.length);

  // Distinct devices that have a session on record. Drives "SYNCED · N DEVICES".
  const deviceCount = $derived(new Set(sessions.map((s) => s.device_id)).size);

  // Most-recent session's device — the "LAST PLAYED on <device>" chip. The
  // last-played *time* uses the authoritative entry aggregate (which may predate
  // session tracking); the device name only comes from sessions.
  const lastDeviceName = $derived.by(() => {
    let latest: PlaySession | null = null;
    for (const s of sessions) {
      if (!latest || s.started_at > latest.started_at) latest = s;
    }
    return latest?.device_name || null;
  });

  // Weekly play totals (minutes), oldest bucket first, newest = current week.
  // Sessions older than WEEKS weeks fall off the left edge of the chart.
  const activity = $derived.by(() => {
    const buckets = new Array<number>(WEEKS).fill(0);
    const now = Date.now();
    for (const s of sessions) {
      const t = new Date(s.started_at).getTime();
      if (Number.isNaN(t)) continue;
      const idx = WEEKS - 1 - Math.floor((now - t) / WEEK_MS);
      if (idx >= 0 && idx < WEEKS) buckets[idx] += s.duration_secs / 60;
    }
    return buckets;
  });

  const maxBucket = $derived(Math.max(1, ...activity));
  // Index of the most-recent week with any play — gets the full-accent glow bar.
  const lastActive = $derived(
    activity.reduce((acc, v, i) => (v > 0 ? i : acc), -1),
  );

  // Only worth showing once a game has recorded playtime. Gated on the
  // authoritative entry aggregate (not the session rows) so a never-played game
  // shows no card, and so the card appears immediately rather than after the
  // session fetch resolves.
  const hasActivity = $derived(game.playtime_minutes > 0);
</script>

{#if hasActivity}
  <DetailCard title="ACTIVITY · CROSS-DEVICE" {accent}>
    {#snippet action()}
      {#if deviceCount > 0}
        <MonoLabel size={9} class="text-ink-3">
          SYNCED · {deviceCount} DEVICE{deviceCount === 1 ? '' : 'S'}
        </MonoLabel>
      {/if}
    {/snippet}

    <div class="flex items-end justify-between gap-3">
      <div>
        <MonoLabel size={9}>TOTAL PLAYTIME</MonoLabel>
        <div
          class="font-display mt-1 text-[30px] font-bold leading-none tracking-[-0.02em] text-ink-0 tabular-nums"
        >
          {fmtPlaytime(game.playtime_minutes)}
        </div>
        <div
          class="font-mono mt-1 text-[10.5px] tracking-[0.04em] text-ink-3"
        >
          {totalSessions} session{totalSessions === 1 ? '' : 's'}
        </div>
      </div>

      <div class="flex flex-col items-end gap-1.5">
        <MonoLabel size={9}>LAST PLAYED</MonoLabel>
        {#if lastDeviceName}
          <span
            class="inline-flex items-center gap-1.5 whitespace-nowrap rounded-sm border border-line-2 bg-bg-2 px-2 py-1 text-[10.5px] text-ink-1"
          >
            <Monitor size={11} style="color: {accent}" />
            {lastDeviceName}
            {#if game.last_played_at}
              <span class="font-mono text-[9px] tracking-[0.04em] text-ink-3">
                · {relDate(game.last_played_at)}
              </span>
            {/if}
          </span>
        {:else}
          <span class="text-[11.5px] text-ink-2">{relDate(game.last_played_at)}</span>
        {/if}
      </div>
    </div>

    <!-- Weekly activity bars (single-accent totals; most-recent week glows). -->
    <div class="mt-4">
      <div
        class="flex items-end gap-[3px] border-b border-line-2 px-px"
        style:height="52px"
      >
        {#each activity as v, i (i)}
          {@const isLast = i === lastActive}
          {@const hPct = v === 0 ? 0 : Math.max(7, (v / maxBucket) * 100)}
          <div class="flex h-full flex-1 items-end">
            <div
              class="w-full rounded-[1px]"
              style:height="{hPct}%"
              style:min-height={v === 0 ? '2px' : undefined}
              style:background={v === 0
                ? 'rgba(255,255,255,0.05)'
                : isLast
                  ? accent
                  : `color-mix(in srgb, ${accent} 36%, transparent)`}
              style:box-shadow={isLast
                ? `0 0 8px color-mix(in srgb, ${accent} 40%, transparent)`
                : 'none'}
            ></div>
          </div>
        {/each}
      </div>
      <div
        class="font-mono mt-1.5 flex justify-between text-[9px] tracking-[0.1em] text-ink-3"
      >
        <span>{WEEKS} WK AGO</span>
        <span>NOW</span>
      </div>
    </div>
  </DetailCard>
{/if}
