<script lang="ts">
  /**
   * Post-game summary at the top of the game detail pane: this device's most
   * recent session and the achievements it unlocked.
   *
   *   ┌─[accent] POST-GAME SUMMARY ─────────────────── 2h ago  ×┐
   *   │ SESSION        ENDED                 ACHIEVEMENTS       │
   *   │ 1h 20m         3 Jun 2026, 21:14     +3                 │
   *   │ ███████████▓▓▓░░░░░░░░░░░░░░░░░░░░░░   12 → 15 / 40     │
   *   │ [icon] Winner                         Rare · 4.2%       │
   *   │        Win one game.                                    │
   *   └─────────────────────────────────────────────────────────┘
   *
   * The card stays up until dismissed, or until the session is a week old.
   * Dismissal is stored per game as the dismissed `session_id`, so the next
   * session shows the card again. An unlock belongs to the session when the
   * session's achievement watcher caught it (`sessionUnlocks`).
   */
  import { Lock, Trophy, X } from '@lucide/svelte';
  import { sessionUnlocks } from '$lib/activity';
  import { achievementIconSrc } from '$lib/achievementIcon';
  import { absDateTime, fmtPlaytime, relDate } from '$lib/format';
  import type { GameAchievements, PlaySession } from '$lib/types';
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
    /** This game's play sessions, every device. */
    sessions: PlaySession[];
    data: GameAchievements | null;
    /** This device's id; the card only summarises sessions played here. */
    deviceId: string | null;
    accent?: string;
  } = $props();

  /** Unlock rows shown before "+N more". */
  const SHOWN = 4;
  /** A session older than this gets no summary, so games last played long ago
   *  don't each open with one. */
  const MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;

  const storageKey = $derived(`spool:post-game-dismissed:${gameId}`);

  function readDismissed(key: string): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  }

  /** The dismissed session id for the selected game. Re-read when the game
   *  changes; `dismiss` overwrites it locally. */
  let dismissed = $derived(readDismissed(storageKey));

  const latest = $derived.by(() => {
    if (!deviceId) return null;
    let best: PlaySession | null = null;
    for (const s of sessions) {
      if (s.device_id !== deviceId) continue;
      if (!best || Date.parse(s.ended_at) > Date.parse(best.ended_at)) best = s;
    }
    return best;
  });

  const earned = $derived(latest && data ? sessionUnlocks(latest, data.items) : []);
  const shown = $derived(earned.slice(0, SHOWN));
  const hasTotal = $derived(data?.schema_source != null && data.total > 0);
  const before = $derived(data ? data.unlocked - earned.length : 0);
  const visible = $derived(
    latest !== null &&
      dismissed !== latest.session_id &&
      Date.now() - Date.parse(latest.ended_at) < MAX_AGE_MS,
  );
  const stats = $derived(
    latest
      ? [
          { label: 'SESSION', value: fmtPlaytime(Math.round(latest.duration_secs / 60)) },
          { label: 'ENDED', value: absDateTime(latest.ended_at) },
          { label: 'ACHIEVEMENTS', value: earned.length > 0 ? `+${earned.length}` : '—' },
        ]
      : [],
  );

  function dismiss() {
    if (!latest) return;
    dismissed = latest.session_id;
    try {
      localStorage.setItem(storageKey, latest.session_id);
    } catch {
      // Storage unavailable: the card hides until the pane re-renders.
    }
  }

  function pct(n: number, total: number): number {
    return total > 0 ? (n / total) * 100 : 0;
  }

  function fmtRarity(p: number): string {
    const v = p < 10 ? p.toFixed(1) : String(Math.round(p));
    return p < 10 ? `Rare · ${v}%` : `${v}% of players`;
  }
</script>

{#if visible && latest}
  <DetailCard title="POST-GAME SUMMARY" {accent}>
    {#snippet action()}
      <div class="flex items-center gap-2">
        <MonoLabel size={9} class="text-ink-3">{relDate(latest.ended_at).toUpperCase()}</MonoLabel>
        <button
          type="button"
          onclick={dismiss}
          class="inline-flex cursor-pointer border-0 bg-transparent p-0.5 text-ink-3 transition-colors hover:text-ink-0"
          title="Dismiss"
          aria-label="Dismiss post-game summary"
        >
          <X size={12} />
        </button>
      </div>
    {/snippet}

    <div class="grid grid-cols-3 gap-[18px]">
      {#each stats as stat (stat.label)}
        <div class="min-w-0">
          <MonoLabel size={9}>{stat.label}</MonoLabel>
          <div class="font-display mt-0.5 truncate text-[18px] font-semibold text-ink-0 tabular-nums">
            {stat.value}
          </div>
        </div>
      {/each}
    </div>

    {#if hasTotal && data}
      <div class="mt-3 flex items-center gap-2.5">
        <div class="flex h-1.5 flex-1 overflow-hidden rounded-full bg-bg-3">
          <div
            class="h-full"
            style:width="{pct(before, data.total)}%"
            style:background="color-mix(in srgb, {accent} 36%, transparent)"
          ></div>
          <div class="h-full" style:width="{pct(earned.length, data.total)}%" style:background={accent}></div>
        </div>
        <span class="font-mono text-[11px] text-ink-2 tabular-nums">
          {earned.length > 0 ? `${before} → ` : ''}{data.unlocked} / {data.total}
        </span>
      </div>
    {/if}

    {#if earned.length > 0}
      <ul class="m-0 mt-3.5 flex list-none flex-col gap-2.5 border-t border-dashed border-line-1 p-0 pt-3">
        {#each shown as a (a.api_name)}
          {@const src = achievementIconSrc(a)}
          <li class="flex items-center gap-3">
            <div
              class="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-sm border border-line-2 bg-bg-2 text-ink-3"
              style:box-shadow="0 0 10px color-mix(in srgb, {accent} 30%, transparent)"
            >
              {#if src}
                <img {src} alt="" class="h-full w-full object-cover" />
              {:else}
                <Trophy size={14} />
              {/if}
            </div>
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13px] font-medium text-ink-0">{a.display_name}</div>
              {#if a.description}
                <div class="truncate text-[11.5px] text-ink-2">{a.description}</div>
              {/if}
            </div>
            {#if a.global_percent !== null}
              <div
                class="shrink-0 text-right text-[11px] tabular-nums"
                class:text-ink-3={a.global_percent >= 10}
                style:color={a.global_percent < 10 ? accent : undefined}
              >
                {fmtRarity(a.global_percent)}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      {#if earned.length > SHOWN}
        <div class="mt-2.5 text-[12px] text-ink-3">+{earned.length - SHOWN} more</div>
      {/if}
    {:else if data && data.items.length > 0}
      <div class="mt-3.5 flex items-center gap-2 border-t border-dashed border-line-1 pt-3 text-[12px] text-ink-3">
        <Lock size={12} />
        No achievements unlocked this session.
      </div>
    {/if}
  </DetailCard>
{/if}
