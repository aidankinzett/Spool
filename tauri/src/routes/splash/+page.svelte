<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { api, assetUrl } from '$lib/api';
  import type { RunPhaseEvent, GameEntry, SyncStatus, RawConflictDetails } from '$lib/types';
  import SpoolMark from '$lib/components/SpoolMark.svelte';
  import CloudConflictModal from '$lib/components/CloudConflictModal.svelte';
  import { exit } from '@tauri-apps/plugin-process';
  import { absDateTime, relDate, fmtSize as formatFmtSize } from '$lib/format';

  let conflictDetails = $state<RawConflictDetails | null>(null);

  let phase = $state<string>('restoring');
  let message = $state<string | null>(null);
  let cloudUsed = $state(false);
  let cloudUploadFailed = $state(false);
  let sessionMinutes = $state<number | null>(null);
  let game = $state<GameEntry | null>(null);
  let progress = $state(0);

  // Gate on a hydrated `game`: the modal's resolve/continue handlers assert
  // `game!.id`, and game hydration (in applyPhase) is async and swallows errors,
  // so without this a conflict phase arriving before the entry loads would render
  // the modal and then throw on click. Once `game` resolves the modal appears.
  // (#296)
  const isCloudConflict = $derived(!!game && phase === 'error' && !!message && /cloud sync conflict/i.test(message));

  $effect(() => {
    const id = game?.id;
    if (isCloudConflict && id) {
      api.getCloudConflictDetails(id)
        .then((res) => {
          if (game?.id === id) {
            conflictDetails = res;
          }
        })
        .catch((e) => {
          console.error('[splash] failed to fetch conflict details:', e);
        });
    } else {
      conflictDetails = null;
    }
  });

  const localMeta = $derived.by(() => {
    if (!conflictDetails?.local || !conflictDetails.local.modified) return null;
    const mb = conflictDetails.local.size_bytes / (1024 * 1024);
    return {
      abs: absDateTime(conflictDetails.local.modified),
      rel: relDate(conflictDetails.local.modified),
      size: formatFmtSize(mb),
    };
  });

  const cloudMeta = $derived.by(() => {
    if (!conflictDetails?.cloud || !conflictDetails.cloud.modified) return null;
    const mb = conflictDetails.cloud.size_bytes / (1024 * 1024);
    return {
      abs: absDateTime(conflictDetails.cloud.modified),
      rel: relDate(conflictDetails.cloud.modified),
      size: formatFmtSize(mb),
    };
  });

  const cloudNewer = $derived.by(() => {
    const localTime = conflictDetails?.local?.modified;
    const cloudTime = conflictDetails?.cloud?.modified;
    if (localTime && cloudTime) {
      return new Date(cloudTime) > new Date(localTime);
    }
    return !localTime;
  });
  let progressRaf: number | null = null;

  let windowHeight = $state(800);
  // The splash's own layout already scales to whatever resolution gamescope
  // (or a streaming host) hands it — `--s` is driven off the window height, so
  // it fills the screen on a handheld, a Deck, or a desktop monitor without any
  // extra Game-Mode zoom. (An earlier 1.5× bump here double-counted that and
  // blew the splash up, worst of all on tall displays like an ultrawide.)
  let s = $derived(windowHeight / 800);
  // The cloud-conflict modal is a shared component sized in fixed desktop px, so
  // it doesn't follow `--s` and looks small against the scaled splash. Zoom it
  // up to roughly match, but clamp it: at least readable on short windows, and
  // capped so a tall display (where `s` climbs toward 2) can't push it off the
  // edges of the screen.
  let modalZoom = $derived(Math.min(Math.max(s, 1), 1.4));
  let syncStatus = $state<'online' | 'offline' | 'unconfigured'>('online');
  // Cloud upload failure (local backup ok, remote upload failed) also renders as offline.
  let net = $derived(syncStatus === 'offline' || cloudUploadFailed ? 'offline' : 'online');

  // Determine flow from phase. Error can occur in either flow; we track
  // which flow was active when the error hit via a separate flag.
  const EXIT_PHASES = new Set(['backing-up', 'uploading', 'done']);
  let exitFlowReached = $state(false);
  let flow = $derived(EXIT_PHASES.has(phase) || (phase === 'error' && exitFlowReached) ? 'exit' : 'launch');

  // Accent color from game or brand default.
  let accent = $derived(game?.accent_color ?? '#d7c9a0');

  // True during phases where Spool is actively doing work (drives kicker dot pulse).
  let working = $derived(phase === 'restoring' || phase === 'backing-up' || phase === 'uploading');

  // Active ramp phases drive an animated progress bar.
  const RAMP_PHASES = new Set(['restoring', 'backing-up', 'uploading']);

  function startRamp() {
    // Cancel any ramp still in flight so two phases in quick succession
    // (restoring → backing-up → uploading) don't leave overlapping RAF loops
    // both writing `progress`.
    if (progressRaf != null) cancelAnimationFrame(progressRaf);
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
  const COPY: Record<string, { kicker: string; sub: (g: GameEntry | null) => string; tone: string }> = {
    restoring:     { kicker: 'RESTORING',       sub: () => 'Pulling the latest revision before launch',             tone: 'accent' },
    launching:     { kicker: 'SAVES RESTORED',  sub: (g) => g ? `Starting ${g.game_name}…` : 'Starting game…',        tone: 'ok'     },
    playing:       { kicker: 'STARTING',        sub: (g) => g ? `${g.game_name} is taking the screen` : 'Handing off to your game', tone: 'ok' },
    'backing-up':  { kicker: 'BACKING UP',      sub: (g) => g ? `You closed ${g.game_name} — saving your progress before you go` : 'Saving your progress before you go', tone: 'accent' },
    uploading:     { kicker: 'UPLOADING',       sub: () => 'Mirroring this revision to your cloud remote',          tone: 'info'   },
    done:          { kicker: 'ALL SAVED',       sub: () => 'Returning you to Steam…',                              tone: 'ok'     },
    error:         { kicker: 'FAILED',          sub: () => '',                                                     tone: 'bad'    },
  };
  let copy = $derived.by(() => {
    const baseFn = COPY[phase];
    const base = baseFn
      ? { kicker: baseFn.kicker, sub: baseFn.sub(game), tone: baseFn.tone }
      : { kicker: phase.toUpperCase(), sub: '', tone: 'accent' };
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

  type Step = { id: string; label: string; detail: string; state: string; badge?: string; warn?: boolean };

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
    
    const offline = net === 'offline';
    const restoreDetail = offline
      ? `Latest copy on this device · ${game ? fmtSize(game.save_backup_size_mb) : '…'} · cloud remote unreachable`
      : `Newest revision · ${game ? fmtSize(game.save_backup_size_mb) : '…'} · pulled from your cloud remote`;

    return [
      {
        id: 'restore', label: 'Restore saves',
        detail: game ? restoreDetail : 'Checking latest revision…',
        state: state(0),
        warn: offline && state(0) === 'done' ? true : undefined,
      },
      {
        id: 'launch', label: 'Launch game',
        detail: game ? `${game.exe_path.split(/[\\/]/).pop() ?? game.game_name} · ${game.use_proton ? 'Proton' : 'Native'}` : '…',
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
    
    const offline = net === 'offline';
    // During the dedicated upload phase the cloud step is genuinely in flight —
    // show the spinner ('active') even if a passive earlier probe read offline,
    // so the user sees the upload actually happening rather than a stale warn.
    const syncState = !cloudUsed ? 'skipped'
      : isErr ? 'pending'
      : phase === 'uploading' ? 'active'
      : offline ? 'warn'
      : phase === 'backing-up' ? 'pending'
      : phase === 'done' ? 'done'
      : 'active';

    const backupDetail = isErr
      ? 'ludusavi error · last good revision kept, nothing lost'
      : 'New revision written to this device';
    const syncDetail = !cloudUsed
      ? 'No cloud remote configured'
      : syncState === 'warn'
        ? 'Couldn\'t reach your cloud remote · 1 revision queued · retries automatically'
        : phase === 'uploading' ? 'Uploading this revision to your cloud remote…'
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
          { label: 'This session', value: sessionMinutes != null ? fmtPlaytime(sessionMinutes) : '…' },
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
    cloudUploadFailed = ev.cloud_upload_failed;
    if (ev.session_minutes != null) sessionMinutes = ev.session_minutes;

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

    api.currentSyncStatus()
      .then((s) => {
        syncStatus = s.reachability;
      })
      .catch(() => {});

    let unlistenSyncStatus: (() => void) | undefined;
    listen<SyncStatus>('sync:status-changed', (event) => {
      syncStatus = event.payload.reachability;
    }).then((fn) => {
      unlistenSyncStatus = fn;
    }).catch(() => {});

    // Re-fetch game data whenever the library changes so exit-flow footer
    // stats (revision count, backup size) reflect the just-completed backup.
    let unlistenLibrary: (() => void) | undefined;
    listen<string>('library:changed', (event) => {
      if (game && event.payload === game.id) {
        api.listGames().then((games) => {
          const updated = games.find((g) => g.id === game!.id);
          if (updated) game = updated;
        }).catch(() => {});
      }
    }).then((fn) => {
      unlistenLibrary = fn;
    }).catch(() => {});

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

    startRamp();

    return () => {
      unlistenFn?.();
      unlistenSyncStatus?.();
      unlistenLibrary?.();
      if (progressRaf != null) cancelAnimationFrame(progressRaf);
    };
  });
</script>

<svelte:window bind:innerHeight={windowHeight} />

<div
  class="splash"
  style="
    --accent: {accent};
    --tone: {toneColor(copy.tone)};
    --bloom-col: {phase === 'error' ? '#ff7a7a' : net === 'offline' ? '#f4b66c' : accent}1f;
    --s: {s};
  "
>
  <!-- Background -->
  <div class="bg" aria-hidden="true">
    <div class="grain"></div>
  </div>

  <div class="layout">
    <!-- Top row -->
    <div class="top-row">
      <div class="lockup">
        <!-- Spool cassette mark -->
        <SpoolMark size={22 * s} color="rgba(244,244,245,0.78)" tape={accent} />
        <span class="lockup-spool">SPOOL</span>
        <span class="lockup-sep"></span>
        <span class="lockup-mode">GAME MODE</span>
      </div>

      <!-- Game identity (right) -->
      <div class="game-id-area">
        <div class="game-id-meta">
          <div style="display: flex; align-items: center; gap: calc(8px * var(--s, 1)); justify-content: flex-end; margin-bottom: calc(4px * var(--s, 1));">
            {#if cloudUsed}
              {@const off = net === 'offline'}
              {@const col = off ? '#f4b66c' : '#7ec6ff'}
              <span class="cloud-chip" style="color: {col}">
                <svg width={12 * s} height={12 * s} viewBox="0 0 16 16" fill="none" stroke={col} stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="display: block; flex-shrink: 0;">
                  <path d="M4.5 11.5a3 3 0 0 1-.3-6 3.5 3.5 0 0 1 6.8-.6 2.8 2.8 0 0 1 .5 5.6Z" />
                  {#if off}
                    <line x1="2.2" y1="2.2" x2="13.8" y2="13.8" stroke={col} stroke-width="1.5" />
                  {/if}
                </svg>
                <span>{off ? 'CLOUD OFFLINE' : 'CLOUD'}</span>
              </span>
            {/if}
            {#if game?.catalog_number}
              <span class="catalog-chip" style="color:{accent};border-color:{accent}55">{catalogId(game.catalog_number)}</span>
            {/if}
          </div>
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
        <span class="kicker-dot" style="background:{toneColor(copy.tone)}; box-shadow:0 0 {8 * s}px {toneColor(copy.tone)}; animation:{working ? 'gm-pulse 1.3s ease-in-out infinite' : 'none'}"></span>
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
        {@const isWarn = step.state === 'warn' || (step.warn && isDone)}
        {@const isSkipped = step.state === 'skipped'}
        {@const isPending = step.state === 'pending'}
        {@const tint = isErr ? '#ff7a7a' : isWarn ? '#f4b66c' : isDone ? '#7ee2a4' : isActive ? accent : 'rgba(244,244,245,0.36)'}
        {@const badge = step.badge ?? (isDone ? 'DONE' : isActive ? ({'restore':'RESTORING','launch':'RUNNING','handoff':'RUNNING','backup':'SAVING','sync':'UPLOADING','exitbackup':'QUEUED'}[step.id] ?? 'RUNNING') : isErr ? 'FAILED' : isWarn ? 'OFFLINE' : isSkipped ? 'OFF' : 'QUEUED')}
        {@const badgeText = (step.warn && isDone) ? 'LOCAL ONLY' : badge}
        {@const badgeTint = (step.warn && isDone) ? '#f4b66c' : tint}
        {@const railColor = isDone ? (step.warn ? '#f4b66c' : '#7ee2a4') : 'rgba(255,255,255,0.10)'}
        <div class="step" class:step-dim={isPending || isSkipped}>
          <!-- Rail left -->
          <div class="step-rail">
            <!-- Glyph -->
            {#if isDone && !step.warn}
              <svg width={22 * s} height={22 * s} viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="#7ee2a4" stroke-width="1.6"/>
                <path d="M6.5 11.2l3 3 6-6.4" fill="none" stroke="#7ee2a4" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            {:else if isActive}
              <!-- Spinning reel -->
              <svg width={22 * s} height={22 * s} viewBox="0 0 22 22" class="glyph glyph-spin" style="color:{accent}">
                <circle cx="11" cy="11" r="9.68" fill="none" stroke="currentColor" stroke-width="1.5"/>
                <line x1="11" y1="7.48" x2="11" y2="3.08" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                <line x1="14.05" y1="9.24" x2="17.86" y2="7.04" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                <line x1="14.05" y1="12.76" x2="17.86" y2="14.96" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                <line x1="11" y1="14.52" x2="11" y2="18.92" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                <line x1="7.95" y1="12.76" x2="4.14" y2="14.96" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                <line x1="7.95" y1="9.24" x2="4.14" y2="7.04" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                <circle cx="11" cy="11" r="8.8" fill="none" stroke="currentColor" stroke-width="1.05" opacity="0.4"/>
                <circle cx="11" cy="11" r="2.86" fill="currentColor"/>
              </svg>
            {:else if isErr}
              <svg width={22 * s} height={22 * s} viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="#ff7a7a" stroke-width="1.6"/>
                <path d="M11 6v6M11 15.4v.2" fill="none" stroke="#ff7a7a" stroke-width="1.8" stroke-linecap="round"/>
              </svg>
            {:else if isWarn}
              <svg width={22 * s} height={22 * s} viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="#f4b66c" stroke-width="1.6"/>
                <path d="M11 6v6M11 15.4v.2" fill="none" stroke="#f4b66c" stroke-width="1.8" stroke-linecap="round"/>
              </svg>
            {:else if isSkipped}
              <svg width={22 * s} height={22 * s} viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="rgba(255,255,255,0.16)" stroke-width="1.5"/>
                <path d="M7 11h8" fill="none" stroke="rgba(244,244,245,0.36)" stroke-width="1.6" stroke-linecap="round"/>
              </svg>
            {:else}
              <svg width={22 * s} height={22 * s} viewBox="0 0 22 22" class="glyph">
                <circle cx="11" cy="11" r="10" fill="none" stroke="rgba(255,255,255,0.16)" stroke-width="1.5" stroke-dasharray="2.5 3.5"/>
              </svg>
            {/if}
            <!-- Connector rail -->
            {#if !isLast}
              <div class="rail-line" style="background:{railColor}; opacity:{isDone ? 0.5 : 1}"></div>
            {/if}
          </div>

          <!-- Body -->
          <div class="step-body" class:step-body-last={isLast}>
            <div class="step-header">
              <span class="step-label" class:step-label-active={isActive || isErr || isWarn} class:step-label-strike={isSkipped}>
                {step.label}
              </span>
              <span class="step-badge" style="color:{badgeTint}; border-color:{badgeTint}55">
                {badgeText}
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
      {@const offline = net === 'offline'}
      {@const uploading = phase === 'uploading'}
      {@const cloudTone = uploading ? '#7ec6ff'
        : offline ? '#f4b66c'
        : phase === 'error' ? '#ff7a7a'
        : phase === 'done' ? '#7ee2a4'
        : phase === 'backing-up' || phase === 'restoring' ? '#7ec6ff'
        : 'rgba(244,244,245,0.36)'}
      {@const cloudLabel = uploading ? 'CLOUD SYNC · UPLOADING'
        : offline ? 'CLOUD SYNC · OFFLINE'
        : phase === 'done' ? 'CLOUD SYNC · UP TO DATE'
        : phase === 'backing-up' ? 'CLOUD SYNC · WAITING'
        : phase === 'restoring' ? 'CLOUD SYNC · CHECKING'
        : phase === 'error' ? 'CLOUD SYNC · ON HOLD'
        : 'CLOUD SYNC'}
      {@const cloudNote = uploading
        ? 'Mirroring this revision to your cloud remote — your other devices pick it up next launch.'
        : offline
        ? (flow === 'exit'
            ? 'Backup saved on this device. Spool will push it to your cloud remote the moment you reconnect.'
            : 'Couldn\'t reach your cloud remote — launched with this device\'s latest save. Newer remote saves (if any) merge when you reconnect.')
        : phase === 'done' ? 'Every device now has this revision.'
        : phase === 'backing-up' ? 'Will mirror to your cloud remote once the local backup is written.'
        : phase === 'restoring' ? 'Checking your cloud remote for newer saves from your other devices…'
        : phase === 'error' && flow === 'exit' ? 'Sync paused until the local backup succeeds.'
        : phase === 'error' ? 'Remote check paused while the restore is retried.'
        : ''}
      <div class="cloud-row" style="border-color:{cloudTone}; background:linear-gradient(90deg,{cloudTone}1f,{cloudTone}08 40%,transparent)">
        <svg width={16 * s} height={16 * s} viewBox="0 0 16 16" fill="none" stroke={cloudTone} stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0">
          <path d="M4.5 11.5a3 3 0 0 1-.3-6 3.5 3.5 0 0 1 6.8-.6 2.8 2.8 0 0 1 .5 5.6Z"/>
          {#if offline}
            <line x1="2.2" y1="2.2" x2="13.8" y2="13.8" stroke={cloudTone} stroke-width="1.5" />
          {/if}
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
        {#each footMeta as m (m.label)}
          <div class="meta-cell">
            <span class="meta-label">{m.label}</span>
            <span class="meta-value">{m.value}</span>
          </div>
        {/each}
      </div>
    </div>
  </div>

  {#if isCloudConflict}
    <!-- CloudConflictModal is a shared component sized in fixed px (it renders
         at desktop density in the main window), so it doesn't read the splash's
         --s scale and looks small against the scaled splash. Wrap it in a CSS
         `zoom` (clamped) so it's comfortably readable. `zoom` scales the modal's
         fixed-position scrim too, unlike `transform`. -->
    <div style="zoom: {modalZoom};">
    <CloudConflictModal
      gameName={game?.game_name ?? 'Game'}
      accent={accent}
      coverUrl={assetUrl(game?.cover_image_path)}
      cloudNewer={cloudNewer}
      localMeta={localMeta}
      cloudMeta={cloudMeta}
      context="gamemode"
      resolve={async (side) => {
        await api.resolveCloudConflict(game!.id, side);
      }}
      onCancel={async () => {
        // The splash only ever runs in an attached (Game-Mode / streaming)
        // launch, so dismissing the conflict has to exit the whole app — not
        // just close this window. The hidden `main` window (and the exit guard
        // in lib.rs) would otherwise keep the process alive after the splash
        // closed, so the host (gamescope / Moonlight) never sees Spool stop.
        await exit(0);
      }}
      onContinue={async () => {
        const priorPhase = phase;
        try {
          // Reset state back to restoring since we're retrying launch
          phase = 'restoring';
          message = 'Syncing + restoring saves…';
          startRamp();
          await api.launchGame(game!.id);
          // Retry workflow finished (game played + backed up) — exit so the
          // attached host sees Spool stop, mirroring the normal launch path.
          await exit(0);
        } catch (e) {
          console.error('[splash] retry launch failed:', e);
          phase = priorPhase;
          message = String(e);
          if (progressRaf != null) cancelAnimationFrame(progressRaf);
          progress = 0;
        }
      }}
      onLudusavi={() => {
        api.openLudusaviGui().catch(() => {});
      }}
      onClose={async () => {
        await exit(0);
      }}
    />
    </div>
  {/if}
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
      radial-gradient(120% 90% at -5% 30%, var(--bloom-col, #d7c9a01f) 0%, transparent 45%),
      linear-gradient(180deg, #0c0e11 0%, #0b0c0e 60%, #060708 100%);
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
    padding: calc(64px * var(--s, 1));
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
    gap: calc(10px * var(--s, 1));
  }
  .lockup-spool {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(11px * var(--s, 1));
    letter-spacing: 0.22em;
    color: rgba(244,244,245,0.78);
  }
  .lockup-sep {
    width: 1px;
    height: calc(12px * var(--s, 1));
    background: rgba(255,255,255,0.16);
  }
  .lockup-mode {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(11px * var(--s, 1));
    letter-spacing: 0.22em;
    color: rgba(244,244,245,0.36);
  }

  .game-id-area {
    display: flex;
    align-items: center;
    gap: calc(14px * var(--s, 1));
  }
  .game-id-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: calc(4px * var(--s, 1));
  }
  .cloud-chip {
    display: inline-flex;
    align-items: center;
    gap: calc(5px * var(--s, 1));
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(9.5px * var(--s, 1));
    letter-spacing: 0.1em;
  }
  .catalog-chip {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(9.5px * var(--s, 1));
    letter-spacing: 0.1em;
    border: 1px solid;
    border-radius: 3px;
    padding: calc(1.5px * var(--s, 1)) calc(6px * var(--s, 1));
  }
  .dev-label {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(10px * var(--s, 1));
    letter-spacing: 0.1em;
    color: rgba(244,244,245,0.36);
  }
  .cover-thumb {
    width: calc(52px * var(--s, 1));
    height: calc(74px * var(--s, 1));
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
    margin-bottom: calc(30px * var(--s, 1));
  }
  .kicker {
    display: inline-flex;
    align-items: center;
    gap: calc(9px * var(--s, 1));
    margin-bottom: calc(14px * var(--s, 1));
  }
  .kicker-dot {
    width: calc(7px * var(--s, 1));
    height: calc(7px * var(--s, 1));
    border-radius: 99px;
  }
  .kicker-text {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(11px * var(--s, 1));
    letter-spacing: 0.2em;
  }
  .game-title {
    margin: 0;
    font-family: "Space Grotesk", system-ui, sans-serif;
    font-weight: 600;
    font-size: calc(40px * var(--s, 1));
    letter-spacing: -0.02em;
    color: #f4f4f5;
    line-height: 1.04;
  }
  .game-title.error-title { color: #ff7a7a; }
  .sub {
    margin-top: calc(10px * var(--s, 1));
    margin-bottom: 0;
    margin-left: 0;
    margin-right: 0;
    font-size: calc(16px * var(--s, 1));
    color: rgba(244,244,245,0.56);
    line-height: 1.4;
  }

  /* ── Pipeline ── */
  .pipeline {
    max-width: calc(640px * var(--s, 1));
  }
  .step {
    display: flex;
    gap: calc(16px * var(--s, 1));
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
    width: calc(1.5px * var(--s, 1));
    flex: 1;
    margin: calc(4px * var(--s, 1)) 0;
    min-height: calc(26px * var(--s, 1));
  }

  .step-body {
    flex: 1;
    padding-bottom: calc(22px * var(--s, 1));
  }
  .step-body-last { padding-bottom: 0; }
  .step-header {
    display: flex;
    align-items: center;
    gap: calc(10px * var(--s, 1));
  }
  .step-label {
    font-family: "Space Grotesk", system-ui, sans-serif;
    font-weight: 600;
    font-size: calc(18px * var(--s, 1));
    color: rgba(244,244,245,0.78);
    letter-spacing: -0.01em;
  }
  .step-label-active { color: #f4f4f5; }
  .step-label-strike { text-decoration: line-through; }
  .step-badge {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(9px * var(--s, 1));
    letter-spacing: 0.14em;
    border: 1px solid;
    border-radius: 3px;
    padding: calc(1.5px * var(--s, 1)) calc(6px * var(--s, 1));
  }
  .step-detail {
    margin-top: calc(5px * var(--s, 1));
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(11px * var(--s, 1));
    letter-spacing: 0.03em;
  }

  /* ── Cloud row ── */
  .cloud-row {
    margin-top: calc(20px * var(--s, 1));
    display: flex;
    align-items: center;
    gap: calc(12px * var(--s, 1));
    padding: calc(10px * var(--s, 1)) calc(14px * var(--s, 1));
    border-radius: 3px;
    border-left: calc(3px * var(--s, 1)) solid;
    max-width: calc(640px * var(--s, 1));
  }
  .cloud-label {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(10px * var(--s, 1));
    letter-spacing: 0.14em;
  }
  .cloud-note {
    margin-top: calc(2px * var(--s, 1));
    font-size: calc(12px * var(--s, 1));
    color: rgba(244,244,245,0.56);
    line-height: 1.35;
  }

  /* ── Footer ── */
  .footer {
    margin-top: calc(28px * var(--s, 1));
    display: flex;
    align-items: center;
    gap: calc(22px * var(--s, 1));
  }
  .tape-wrap {
    flex: 1;
  }
  .tape-track {
    position: relative;
    height: calc(5px * var(--s, 1));
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
    height: calc(2px * var(--s, 1));
    margin-top: calc(3px * var(--s, 1));
    background-image: repeating-linear-gradient(to right, rgba(255,255,255,0.10) 0 1px, transparent 1px 12.5%);
  }
  .foot-meta {
    display: flex;
    gap: calc(28px * var(--s, 1));
  }
  .meta-cell {
    display: flex;
    flex-direction: column;
    gap: calc(4px * var(--s, 1));
  }
  .meta-label {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: calc(9.5px * var(--s, 1));
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: rgba(244,244,245,0.36);
  }
  .meta-value {
    font-family: "Geist", system-ui, sans-serif;
    font-size: calc(14px * var(--s, 1));
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
