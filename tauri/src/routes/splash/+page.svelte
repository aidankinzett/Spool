<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { api, assetUrl } from '$lib/api';
  import type { RunPhaseEvent, GameEntry } from '$lib/types';

  let phase = $state<string>('restoring');
  let message = $state<string | null>(null);
  let cloudUsed = $state(false);
  let game = $state<GameEntry | null>(null);
  let progress = $state(0);
  let progressRaf = $state<number | null>(null);

  // Determine flow from phase. Error can occur in either flow; we track
  // which flow was active when the error hit via a separate flag.
  const EXIT_PHASES = new Set(['backing-up', 'done']);
  let exitFlowReached = $state(false);
  let flow = $derived(EXIT_PHASES.has(phase) || (phase === 'error' && exitFlowReached) ? 'exit' : 'launch');

  // Accent color from game or brand default.
  let accent = $derived(game?.accent_color ?? '#d7c9a0');

  // Active ramp phases drive an animated progress bar.
  const RAMP_PHASES = new Set(['restoring', 'backing-up']);

  function startRamp() {
    progress = 0;
    const start = performance.now();
    function tick(now: number) {
      const p = Math.min(1, (now - start) / 3000);
      progress = 1 - Math.pow(1 - p, 2);
      if (p < 1) progressRaf = requestAnimationFrame(tick);
    }
    progressRaf = requestAnimationFrame(tick);
  }

  function stopRamp() {
    if (progressRaf != null) cancelAnimationFrame(progressRaf);
    progressRaf = null;
    progress = 1;
  }

  // Phase copy
  const COPY: Record<string, { kicker: string; sub: string; tone: string }> = {
    restoring:     { kicker: 'RESTORING',       sub: 'Pulling the latest revision before launch',             tone: 'accent' },
    launching:     { kicker: 'SAVES RESTORED',  sub: 'Starting game…',                                       tone: 'ok'     },
    playing:       { kicker: 'STARTING',         sub: 'Handing off to your game',                             tone: 'ok'     },
    'backing-up':  { kicker: 'BACKING UP',       sub: 'Saving your progress before you go',                   tone: 'accent' },
    done:          { kicker: 'ALL SAVED',         sub: 'Returning you to Steam…',                              tone: 'ok'     },
    error:         { kicker: 'FAILED',            sub: '',   tone: 'bad' },
  };
  let copy = $derived.by(() => {
    const base = COPY[phase] ?? { kicker: phase.toUpperCase(), sub: '', tone: 'accent' as string };
    if (phase === 'error') {
      return {
        ...base,
        kicker: flow === 'exit' ? 'BACKUP FAILED' : 'RESTORE FAILED',
        sub: message ?? (flow === 'exit'
          ? 'Your last good revision is kept — try again next launch'
          : 'Restore failed — you can launch anyway or retry'),
      };
    }
    return { ...base, sub: message ?? base.sub };
  });

  function toneColor(tone: string) {
    if (tone === 'ok')     return '#7ee2a4';
    if (tone === 'bad')    return '#ff7a7a';
    if (tone === 'warn')   return '#f4b66c';
    if (tone === 'info')   return '#7ec6ff';
    if (tone === 'accent') return accent;
    return '#f4f4f5';
  }

  // ── Steps ──────────────────────────────────────────────────────────

  type Step = { id: string; label: string; detail: string; state: string; badge?: string };

  function launchSteps(): Step[] {
    const LAUNCH_ORDER = ['restoring', 'launching', 'playing'];
    const isErr = phase === 'error';
    const idx = isErr ? 0 : LAUNCH_ORDER.indexOf(phase);
    const state = (n: number) => {
      if (isErr && n === 0) return 'error';
      if (n < idx) return 'done';
      if (n === idx) return 'active';
      return 'pending';
    };
    return [
      {
        id: 'restore', label: 'Restore saves',
        detail: game ? `Latest revision · pulled from your cloud remote` : 'Checking latest revision…',
        state: state(0),
      },
      {
        id: 'launch', label: 'Launch game',
        detail: game ? game.exe_path.split(/[\\/]/).pop() ?? game.game_name : '…',
        state: state(1),
      },
      {
        id: 'handoff', label: 'Hand off display',
        detail: 'Spool steps aside while you play',
        state: state(2),
      },
      {
        id: 'exitbackup', label: 'Back up on exit',
        detail: 'Automatic when you quit — even from the game\'s own menu',
        state: 'pending',
      },
    ];
  }

  function exitSteps(): Step[] {
    const isErr = phase === 'error';
    const backupState = isErr ? 'error' : phase === 'backing-up' ? 'active' : 'done';
    const syncState = !cloudUsed ? 'skipped'
      : isErr ? 'pending'
      : phase === 'backing-up' ? 'pending'
      : phase === 'done' ? 'done'
      : 'active';

    const backupDetail = isErr
      ? 'ludusavi error · last good revision kept, nothing lost'
      : 'New revision written to this device';
    const syncDetail = !cloudUsed
      ? 'No cloud remote configured'
      : phase === 'done' ? 'Mirrors to your cloud remote · all devices in step'
      : 'Mirrors to your cloud remote after local backup completes';

    return [
      { id: 'ended',  label: 'Session ended', detail: `Closed by game`, state: 'done' },
      { id: 'backup', label: 'Back up saves · this device', detail: backupDetail, state: backupState },
      { id: 'sync',   label: 'Sync to cloud', detail: syncDetail, state: syncState },
    ];
  }

  let steps = $derived(flow === 'exit' ? exitSteps() : launchSteps());

  // ── Metadata footer ────────────────────────────────────────────────
  function fmtSize(mb: number): string {
    if (mb < 1) return `${Math.round(mb * 1024)} KB`;
    return `${mb.toFixed(1)} MB`;
  }
  function fmtPlaytime(min: number): string {
    if (min < 60) return `${min}m`;
    const h = Math.floor(min / 60), m = min % 60;
    return m > 0 ? `${h}h ${m}m` : `${h}h`;
  }
  function fmtWhen(iso: string | null): string {
    if (!iso) return 'never';
    const d = new Date(iso), now = Date.now();
    const diff = Math.floor((now - d.getTime()) / 60000);
    if (diff < 2)  return 'just now';
    if (diff < 60) return `${diff}m ago`;
    const h = Math.floor(diff / 60);
    if (h < 24)  return `${h}h ago`;
    const days = Math.floor(h / 24);
    if (days < 7) return `${days}d ago`;
    return d.toLocaleDateString();
  }
  function catalogId(n: number): string {
    return `SPL-${String(n).padStart(4, '0')}`;
  }

  let footMeta = $derived(
    flow === 'exit'
      ? [
          { label: 'New backup', value: game ? fmtSize(game.save_backup_size_mb) : '…' },
          { label: 'Revisions kept', value: game ? String(game.save_backup_count) : '…' },
        ]
      : [
          { label: 'Last backup', value: game ? `${fmtSize(game.save_backup_size_mb)} · ${fmtWhen(game.save_last_backed_up_at)}` : '…' },
          { label: 'Playtime', value: game ? fmtPlaytime(game.playtime_minutes) : '…' },
        ]
  );

  // ── Phase queue (same pacing logic as before) ─────────────────────
  const MIN_VISIBLE_MS = 700;
  const queue: RunPhaseEvent[] = [];
  let draining = false;

  function applyPhase(ev: RunPhaseEvent) {
    // Stop any ongoing ramp when phase changes.
    stopRamp();
    // Track whether an exit-flow phase was ever reached (so an error after
    // backing-up is displayed in exit-flow context, not as a launch error).
    if (EXIT_PHASES.has(ev.phase)) exitFlowReached = true;
    phase = ev.phase;
    message = ev.message ?? null;
    cloudUsed = ev.cloud_used;

    // Hydrate game entry on first event if not yet loaded.
    if (!game && ev.game_id) {
      api.listGames().then((games) => {
        game = games.find((g) => g.id === ev.game_id) ?? null;
      }).catch(() => {});
    }

    if (RAMP_PHASES.has(phase)) {
      startRamp();
    }
  }

  async function pump() {
    if (draining) return;
    draining = true;
    while (queue.length > 0) {
      const ev = queue.shift()!;
      applyPhase(ev);
      if (ev.phase === 'done' || ev.phase === 'error') break;
      if (queue.length > 0) {
        await new Promise((r) => setTimeout(r, MIN_VISIBLE_MS));
      }
    }
    draining = false;
  }

  onMount(() => {
    // Pre-fetch won't block notifySplashReady — we just populate game
    // data as soon as the list resolves.
    api.listGames().catch(() => {});

    let unlistenFn: (() => void) | undefined;
    listen<RunPhaseEvent>('run:phase', (event) => {
      queue.push(event.payload);
      void pump();
    })
      .then((fn) => {
        unlistenFn = fn;
        return api.notifySplashReady();
      })
      .catch((e) => console.error('[splash] run-phase listener failed:', e));

    // Start with an animated ramp for the initial restoring phase.
    startRamp();

    return () => {
      unlistenFn?.();
      if (progressRaf != null) cancelAnimationFrame(progressRaf);
    };
  });
</script>

<div
  class="splash"
  style="
    --accent: {accent};
    --tone: {toneColor(copy.tone)};
    --bloom-col: {phase === 'error' ? '#ff7a7a' : accent};
  "
>
  <!-- Background -->
  <div class="bg" aria-hidden="true">
    <div class="bloom"></div>
    <div class="grain"></div>
  </div>

  <div class="layout">
    <!-- Top row -->
    <div class="top-row">
      <div class="lockup">
        <!-- Spool cassette mark -->
        <svg width="22" height="16" viewBox="0 0 22 16" fill="none" class="mark">
          <rect x="0.75" y="0.75" width="20.5" height="14.5" rx="1.4" stroke="rgba(244,244,245,0.78)" stroke-width="1.5"/>
          <circle cx="6.5" cy="8" r="2.4" stroke="rgba(244,244,245,0.78)" stroke-width="1.4"/>
          <circle cx="6.5" cy="8" r="0.7" fill="rgba(244,244,245,0.78)"/>
          <circle cx="15.5" cy="8" r="2.4" stroke="rgba(244,244,245,0.78)" stroke-width="1.4"/>
          <circle cx="15.5" cy="8" r="0.7" fill="rgba(244,244,245,0.78)"/>
          <rect x="3" y="12.5" width="16" height="1.4" rx="0.4" fill={accent} opacity="0.85"/>
        </svg>
        <span class="lockup-spool">SPOOL</span>
        <span class="lockup-sep"></span>
        <span class="lockup-mode">GAME MODE</span>
      </div>

      <!-- Game identity (right) -->
      <div class="game-id-area">
        <div class="game-id-meta">
          {#if game?.catalog_number}
            <span class="catalog-chip" style="color:{accent};border-color:{accent}55">{catalogId(game.catalog_number)}</span>
          {/if}
          {#if game?.developer}
            <span class="dev-label">{game.developer}</span>
          {/if}
        </div>
        <!-- Small cover thumbnail -->
        {#if game?.cover_image_path}
          <div class="cover-thumb">
            <img src={assetUrl(game.cover_image_path)} alt={game.game_name} />
          </div>
        {/if}
      </div>
    </div>

    <!-- Headline -->
    <div class="headline-area">
      <div class="kicker">
        <span class="kicker-dot" style="background:{toneColor(copy.tone)}"></span>
        <span class="kicker-text" style="color:{toneColor(copy.tone)}">{copy.kicker}</span>
      </div>
      <h1 class="game-title" class:error-title={phase === 'error'}>
        {game?.game_name ?? '…'}
      </h1>
      <p class="sub">{copy.sub}</p>
    </div>

    <!-- Pipeline -->
    <div class="pipeline">
      {#each steps as step, k (step.id)}
        {@const isLast = k === steps.length - 1}
        {@const isActive = step.state === 'active'}
        {@const isDone = step.state === 'done'}
        {@const isErr = step.state === 'error'}
        {@const isWarn = step.state === 'warn'}
        {@const isSkipped = step.state === 'skipped'}
        {@const isPending = step.state === 'pending'}
        {@const tint = isErr ? '#ff7a7a' : isWarn ? '#f4b66c' : isDone ? '#7ee2a4' : isActive ? accent : 'rgba(244,244,245,0.36)'}
        {@const badge = step.badge ?? (isDone ? 'DONE' : isActive ? ({'restore':'RESTORING','launch':'RUNNING','handoff':'RUNNING','backup':'SAVING','sync':'UPLOADING','exitbackup':'QUEUED'}[step.id] ?? 'RUNNING') : isErr ? 'FAILED' : isWarn ? 'OFFLINE' : isSkipped ? 'OFF' : 'QUEUED')}
        <div class="step" class:step-dim={isPending || isSkipped}>
          <!-- Rail left -->
          <div class="step-rail">
            <!-- Glyph -->
            {#if isDone}
              <svg width="22" height="22" viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="#7ee2a4" stroke-width="1.6"/>
                <path d="M6.5 11.2l3 3 6-6.4" fill="none" stroke="#7ee2a4" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            {:else if isActive}
              <!-- Spinning reel -->
              <svg width="22" height="22" viewBox="0 0 22 22" class="glyph glyph-spin" style="color:{accent}">
                <circle cx="11" cy="11" r="9.68" fill="none" stroke="currentColor" stroke-width="0.99"/>
                <line x1="11" y1="7.48" x2="11" y2="4.84" stroke="currentColor" stroke-width="0.99" stroke-linecap="round"/>
                <line x1="13.59" y1="8.29" x2="15.87" y2="6.32" stroke="currentColor" stroke-width="0.99" stroke-linecap="round"/>
                <line x1="14.96" y1="11.16" x2="17.60" y2="11.88" stroke="currentColor" stroke-width="0.99" stroke-linecap="round"/>
                <line x1="13.37" y1="13.94" x2="15.32" y2="16.23" stroke="currentColor" stroke-width="0.99" stroke-linecap="round"/>
                <line x1="8.63" y1="13.94" x2="6.68" y2="16.23" stroke="currentColor" stroke-width="0.99" stroke-linecap="round"/>
                <line x1="7.04" y1="11.16" x2="4.40" y2="11.88" stroke="currentColor" stroke-width="0.99" stroke-linecap="round"/>
                <circle cx="11" cy="11" r="2.86" fill="currentColor"/>
              </svg>
            {:else if isErr}
              <svg width="22" height="22" viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="#ff7a7a" stroke-width="1.6"/>
                <path d="M11 6v6M11 15.4v.2" fill="none" stroke="#ff7a7a" stroke-width="1.8" stroke-linecap="round"/>
              </svg>
            {:else if isWarn}
              <svg width="22" height="22" viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="#f4b66c" stroke-width="1.6"/>
                <path d="M11 6v6M11 15.4v.2" fill="none" stroke="#f4b66c" stroke-width="1.8" stroke-linecap="round"/>
              </svg>
            {:else if isSkipped}
              <svg width="22" height="22" viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="rgba(255,255,255,0.16)" stroke-width="1.5"/>
                <path d="M7 11h8" fill="none" stroke="rgba(244,244,245,0.36)" stroke-width="1.6" stroke-linecap="round"/>
              </svg>
            {:else}
              <svg width="22" height="22" viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="rgba(255,255,255,0.16)" stroke-width="1.5" stroke-dasharray="2.5 3.5"/>
              </svg>
            {/if}
            <!-- Connector rail -->
            {#if !isLast}
              <div class="rail-line" style="background:{isDone ? '#7ee2a4' : 'rgba(255,255,255,0.10)'}; opacity:{isDone ? 0.5 : 1}"></div>
            {/if}
          </div>

          <!-- Body -->
          <div class="step-body" class:step-body-last={isLast}>
            <div class="step-header">
              <span class="step-label" class:step-label-active={isActive || isErr || isWarn} class:step-label-strike={isSkipped}>
                {step.label}
              </span>
              <span class="step-badge" style="color:{tint}; border-color:{tint}55">
                {badge}
              </span>
            </div>
            <div class="step-detail" style="color:{isWarn ? '#f4b66c' : 'rgba(244,244,245,0.56)'}">
              {step.detail}
            </div>
          </div>
        </div>
      {/each}
    </div>

    <!-- Cloud sync row -->
    {#if cloudUsed}
      {@const cloudTone = phase === 'error' ? '#ff7a7a'
        : phase === 'done' ? '#7ee2a4'
        : phase === 'backing-up' || phase === 'restoring' ? '#7ec6ff'
        : 'rgba(244,244,245,0.36)'}
      {@const cloudLabel = phase === 'done' ? 'CLOUD SYNC · UP TO DATE'
        : phase === 'backing-up' ? 'CLOUD SYNC · WAITING'
        : phase === 'restoring' ? 'CLOUD SYNC · CHECKING'
        : phase === 'error' ? 'CLOUD SYNC · ON HOLD'
        : 'CLOUD SYNC'}
      {@const cloudNote = phase === 'done' ? 'Every device now has this revision.'
        : phase === 'backing-up' ? 'Will mirror to your cloud remote once the local backup is written.'
        : phase === 'restoring' ? 'Checking your cloud remote for newer saves from your other devices…'
        : phase === 'error' && flow === 'exit' ? 'Sync paused until the local backup succeeds.'
        : phase === 'error' ? 'Remote check paused while the restore is retried.'
        : ''}
      <div class="cloud-row" style="border-color:{cloudTone}; background:linear-gradient(90deg,{cloudTone}1f,{cloudTone}08 40%,transparent)">
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke={cloudTone} stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0">
          <path d="M4.5 11.5a3 3 0 0 1-.3-6 3.5 3.5 0 0 1 6.8-.6 2.8 2.8 0 0 1 .5 5.6Z"/>
        </svg>
        <div>
          <div class="cloud-label" style="color:{cloudTone}">{cloudLabel}</div>
          {#if cloudNote}<div class="cloud-note">{cloudNote}</div>{/if}
        </div>
      </div>
    {/if}

    <!-- Progress bar + metadata footer -->
    <div class="footer">
      <div class="tape-wrap">
        <div class="tape-track">
          <div
            class="tape-fill"
            style="
              width:{Math.round(progress * 100)}%;
              background:{phase === 'error' ? '#ff7a7a' : phase === 'done' ? '#7ee2a4' : accent};
              box-shadow: 0 0 10px {phase === 'error' ? '#ff7a7a' : phase === 'done' ? '#7ee2a4' : accent}77;
            "
          ></div>
        </div>
        <div class="tape-ticks" aria-hidden="true"></div>
      </div>
      <div class="foot-meta">
        {#each footMeta as m}
          <div class="meta-cell">
            <span class="meta-label">{m.label}</span>
            <span class="meta-value">{m.value}</span>
          </div>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
    background: #060708;
    overflow: hidden;
    height: 100vh;
  }

  .splash {
    position: fixed;
    inset: 0;
    font-family: "Geist", system-ui, sans-serif;
    -webkit-font-smoothing: antialiased;
    color: #f4f4f5;
  }

  /* ── Background ── */
  .bg {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(120% 90% at -5% 30%, var(--bloom-col, #d7c9a0) 0%, transparent 45%),
      linear-gradient(180deg, #0c0e11 0%, #0b0c0e 60%, #060708 100%);
  }
  .bloom {
    position: absolute;
    inset: 0;
    /* bloom is baked into .bg above */
  }
  .grain {
    position: absolute;
    inset: 0;
    opacity: 0.5;
    mix-blend-mode: overlay;
    pointer-events: none;
    background-image: radial-gradient(rgba(255,255,255,0.045) 1px, transparent 1px);
    background-size: 3px 3px;
  }

  /* ── Layout ── */
  .layout {
    position: relative;
    z-index: 1;
    width: 100%;
    height: 100%;
    padding: clamp(40px, 8vh, 64px) clamp(40px, 8vw, 64px);
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
  }

  /* ── Top row ── */
  .top-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
  }
  .lockup {
    display: inline-flex;
    align-items: center;
    gap: 10px;
  }
  .mark { display: block; flex-shrink: 0; }
  .lockup-spool {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: 11px;
    letter-spacing: 0.22em;
    color: rgba(244,244,245,0.78);
  }
  .lockup-sep {
    width: 1px;
    height: 12px;
    background: rgba(255,255,255,0.16);
  }
  .lockup-mode {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: 11px;
    letter-spacing: 0.22em;
    color: rgba(244,244,245,0.36);
  }

  .game-id-area {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .game-id-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
  }
  .catalog-chip {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: 9.5px;
    letter-spacing: 0.1em;
    border: 1px solid;
    border-radius: 3px;
    padding: 1.5px 6px;
  }
  .dev-label {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: 10px;
    letter-spacing: 0.1em;
    color: rgba(244,244,245,0.36);
  }
  .cover-thumb {
    width: 52px;
    height: 74px;
    border-radius: 3px;
    overflow: hidden;
    background: #15181d;
    flex-shrink: 0;
  }
  .cover-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  /* ── Headline ── */
  .headline-area {
    margin-top: auto;
    margin-bottom: 30px;
  }
  .kicker {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 14px;
  }
  .kicker-dot {
    width: 7px;
    height: 7px;
    border-radius: 99px;
    animation: gm-pulse 1.3s ease-in-out infinite;
  }
  .kicker-text {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: clamp(10px, 1.4vh, 11px);
    letter-spacing: 0.2em;
  }
  .game-title {
    margin: 0;
    font-family: "Space Grotesk", system-ui, sans-serif;
    font-weight: 600;
    font-size: clamp(28px, 5vh, 40px);
    letter-spacing: -0.02em;
    color: #f4f4f5;
    line-height: 1.04;
  }
  .game-title.error-title { color: #ff7a7a; }
  .sub {
    margin: 10px 0 0;
    font-size: clamp(13px, 2vh, 16px);
    color: rgba(244,244,245,0.56);
    line-height: 1.4;
  }

  /* ── Pipeline ── */
  .pipeline {
    max-width: min(640px, 65vw);
  }
  .step {
    display: flex;
    gap: 16px;
    transition: opacity 250ms ease;
  }
  .step-dim { opacity: 0.5; }

  .step-rail {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .glyph { display: block; flex-shrink: 0; }
  .glyph-spin {
    animation: gm-spin 2.2s linear infinite;
    transform-origin: center;
  }
  .rail-line {
    width: 1.5px;
    flex: 1;
    margin: 4px 0;
    min-height: 26px;
  }

  .step-body {
    flex: 1;
    padding-bottom: 22px;
  }
  .step-body-last { padding-bottom: 0; }
  .step-header {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .step-label {
    font-family: "Space Grotesk", system-ui, sans-serif;
    font-weight: 600;
    font-size: clamp(14px, 2.2vh, 18px);
    color: rgba(244,244,245,0.78);
    letter-spacing: -0.01em;
  }
  .step-label-active { color: #f4f4f5; }
  .step-label-strike { text-decoration: line-through; }
  .step-badge {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: 9px;
    letter-spacing: 0.14em;
    border: 1px solid;
    border-radius: 3px;
    padding: 1.5px 6px;
  }
  .step-detail {
    margin-top: 5px;
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: clamp(10px, 1.4vh, 11px);
    letter-spacing: 0.03em;
  }

  /* ── Cloud row ── */
  .cloud-row {
    margin-top: 20px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-radius: 3px;
    border-left: 3px solid;
    max-width: min(640px, 65vw);
  }
  .cloud-label {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: 10px;
    letter-spacing: 0.14em;
  }
  .cloud-note {
    margin-top: 2px;
    font-size: 12px;
    color: rgba(244,244,245,0.56);
    line-height: 1.35;
  }

  /* ── Footer ── */
  .footer {
    margin-top: 28px;
    display: flex;
    align-items: center;
    gap: 22px;
  }
  .tape-wrap {
    flex: 1;
  }
  .tape-track {
    position: relative;
    height: 5px;
    border-radius: 1px;
    background: #0b0c0e;
    overflow: hidden;
    box-shadow: inset 0 0 0 1px rgba(255,255,255,0.05);
  }
  .tape-fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    border-radius: 1px;
    transition: width 90ms linear;
  }
  .tape-ticks {
    height: 2px;
    margin-top: 3px;
    background-image: repeating-linear-gradient(to right, rgba(255,255,255,0.10) 0 1px, transparent 1px 12.5%);
  }
  .foot-meta {
    display: flex;
    gap: 28px;
  }
  .meta-cell {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .meta-label {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: 9.5px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: rgba(244,244,245,0.36);
  }
  .meta-value {
    font-family: "Geist", system-ui, sans-serif;
    font-size: 14px;
    font-weight: 500;
    color: #f4f4f5;
    white-space: nowrap;
  }

  /* ── Keyframes ── */
  @keyframes gm-spin {
    to { transform: rotate(360deg); }
  }
  @keyframes gm-pulse {
    0%, 100% { opacity: 0.35; }
    50%       { opacity: 1; }
  }
</style>
