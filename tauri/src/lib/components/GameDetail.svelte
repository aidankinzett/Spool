<script lang="ts">
  /**
   * Right-pane detail view of a selected game.
   *
   * Sections (top → bottom):
   *   - Hero: gradient backdrop, catalog id + side label, big title,
   *     Play button + last-played/playtime, LAN/sync pills
   *   - StatsStrip: 4 columns of mono stats
   *   - ActionToolbar: per-entry actions (Open folder, Steam, …)
   *   - About card: description + genres
   *   - Saves card: backup count / size / last-run
   *   - Details card: developer / publisher / executable / install path
   *
   * Per-game accent colour isn't extracted yet — every game uses the
   * brand `spool` tint until the cover-art-dominant-color slice lands.
   * The structure already passes `accent` through everywhere so wiring
   * it up later is a one-liner.
   */
  import {
    ChevronDown,
    Cloud,
    CloudDownload,
    Copy,
    Download,
    Folder,
    HardDriveDownload,
    Pencil,
    Play,
    RotateCcw,
    Sparkles,
    Trash2,
    X,
  } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { openView } from '$lib/nav';
  import { api, assetUrl, peerAssetUrl } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import { confirmDialog } from '$lib/confirm.svelte';
  import type { DisplayGame, DownloadProgress, GameEntry, RunPhase, SaveRevision } from '$lib/types';
  import {
    absDate,
    absDateTime,
    fmtCatalog,
    fmtPlaytime,
    fmtRate,
    fmtSize,
    relDate,
  } from '$lib/format';
  import MonoLabel from './MonoLabel.svelte';
  import CatalogId from './CatalogId.svelte';
  import Btn from './Btn.svelte';
  import DetailCard from './DetailCard.svelte';
  import CrossDeviceActivityCard from './CrossDeviceActivityCard.svelte';
  import { removeGameDialog } from '$lib/removeGame.svelte';

  let {
    game,
    runPhase = null,
    backingUp = false,
    autofocusPlay = false,
    cloudConfigured = false,
    onPullConflict,
    download = null,
    startingGameId = null,
    onDownload,
    onCancelDownload,
  }: {
    game: DisplayGame;
    /** Current Run-workflow phase for *this* game (null if idle). */
    runPhase?: RunPhase | null;
    /** A forced post-override backup is running for this game — disables Play. */
    backingUp?: boolean;
    /** When true, the Play button is the gamepad-nav initial-focus target.
     *  Set by the touch detail overlay (its own nav scope); desktop leaves it
     *  off so focus stays on the library list. */
    autofocusPlay?: boolean;
    /** Whether a cloud remote is configured. Gates the "Sync now" pull button —
     *  there's nothing to pull without a remote. */
    cloudConfigured?: boolean;
    /** Called when a "Sync now" pull hits a true local-vs-cloud divergence, so
     *  the parent can open the `CloudConflictModal` (sets `lib.conflictGameId`).
     *  Without it the conflict falls back to an error toast. */
    onPullConflict?: (gameId: string) => void;
    /** The single in-flight LAN install, if any — used to show live progress on
     *  the Download button when it's *this* game being fetched from a peer. */
    download?: DownloadProgress | null;
    /** The peer game id whose install is mid-handshake (pre-progress). */
    startingGameId?: string | null;
    /** Start downloading this peer-sourced game (the Download button). */
    onDownload?: (g: DisplayGame) => void;
    /** Cancel the in-flight LAN install. */
    onCancelDownload?: () => void;
  } = $props();

  // When the selected game changes, refresh its save-backup stats from
  // ludusavi's real backup store. Fire-and-forget — the backend emits
  // `library:changed` (which re-feeds `game`) only if a value actually moved,
  // so this can't loop. Tracks `game.id` alone so it doesn't re-run on every
  // unrelated field update.
  $effect(() => {
    const id = game.id;
    // Synthetic peer-only rows (id `peer:…`) aren't real library entries —
    // there's nothing for the backend to refresh.
    if (id.startsWith('peer:')) return;
    void api.refreshSaveMetadata(id).catch(() => {});
  });

  const isRunning = $derived(runPhase != null);
  // Launchable only when installed, with an exe, not already running, and not
  // mid forced-backup (its saves are being rewritten + uploaded).
  const canPlay = $derived(!isRunning && !backingUp && !!game.exe_path && game.installed);
  const playLabel = $derived.by(() => {
    switch (runPhase) {
      case 'restoring':
        return 'Restoring saves…';
      case 'launching':
        return 'Launching…';
      case 'playing':
        return 'Playing';
      case 'backing-up':
        return 'Backing up…';
      case 'uploading':
        return 'Uploading saves…';
      default:
        return backingUp ? 'Backing up…' : 'Play';
    }
  });

  async function launch() {
    if (!canPlay) return;
    try {
      await api.launchGame(game.id);
    } catch (e) {
      // Error is also broadcast via run:phase → 'error', but capturing here
      // so the in-button label can flip back to "Play" immediately.
      console.error('[runner] launch failed:', e);
    }
  }

  // ── LAN download (peer-sourced rows) ────────────────────────────────────
  // A merged sidebar row backed by a peer shows Download instead of Play. This
  // covers both synthetic "available on LAN" rows and local uninstalled rows a
  // peer can supply.
  const peerSource = $derived(game.peer_source ?? null);
  // A synthetic "available on LAN" row — not a real library entry on this
  // device, so per-entry actions (Edit, Remove, Steam…) don't apply yet.
  const isSyntheticPeer = $derived(game.id.startsWith('peer:'));
  // The in-flight install, only when it's *this* peer game (id + device match).
  const peerDownload = $derived(
    peerSource &&
      download &&
      download.source_game_id === peerSource.source_game_id &&
      download.source_device_id === peerSource.device_id
      ? download
      : null,
  );
  const peerInflight = $derived(
    peerDownload != null &&
      (peerDownload.status === 'starting' || peerDownload.status === 'transferring'),
  );
  const peerStarting = $derived(peerSource != null && startingGameId === peerSource.source_game_id);
  // Disabled while any install is in progress (one slot), or the peer can't
  // actually stream it.
  const anyInstallBusy = $derived(
    startingGameId != null ||
      (download != null && (download.status === 'starting' || download.status === 'transferring')),
  );

  function startDownload() {
    if (!peerSource || peerInflight || peerStarting) return;
    onDownload?.(game);
  }

  // Hero art: local file on disk first (kept even when uninstalled), else the
  // peer's hero over HTTP for a peer-sourced row, else the gradient fallback.
  const heroUrl = $derived(
    assetUrl(game.hero_image_path) ?? (peerSource ? peerAssetUrl(peerSource, 'hero') : null),
  );

  /**
   * Per-game accent colour. Extracted from the cover image when it
   * downloaded (see steamgriddb::extract_vibrant_color); falls back to
   * the brand `spool` colour when None — keeps things consistent for
   * games without cover art and before extraction has run.
   */
  const BRAND_SPOOL = '#d7c9a0';
  const accentHex = $derived(game.accent_color ?? BRAND_SPOOL);
  // CSS-variable form for cases that need a token-style reference; same
  // value either way, just different consumers.
  const accent = $derived(accentHex);

  // Try to derive a folder path for the "Open folder" action: the entry's
  // own game_folder_path if set, else the parent of the exe path.
  function folderForGame(g: GameEntry): string | null {
    if (g.game_folder_path) return g.game_folder_path;
    if (!g.exe_path) return null;
    const sep = g.exe_path.includes('\\') ? '\\' : '/';
    const idx = g.exe_path.lastIndexOf(sep);
    return idx > 0 ? g.exe_path.slice(0, idx) : null;
  }

  async function openFolder() {
    const folder = folderForGame(game);
    if (folder) await api.openPath(folder);
  }

  async function copyToClipboard(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // best-effort; webview may not always grant clipboard
    }
  }

  // ── Remove / delete ─────────────────────────────────────────────────────
  // The Remove button opens the three-option chooser (remove from disk / from
  // library / from disk and library), hosted globally by RemoveGameHost. On
  // success `library:changed` clears the parent page's selection.

  // ── Restore an earlier save (rollback) ──────────────────────────────────
  // Lazily loaded when the user expands the picker. The backend lists
  // ludusavi's local revisions newest-first with the tip flagged.
  let revisionsOpen = $state(false);
  let revisions = $state<SaveRevision[] | null>(null);
  let revisionsLoading = $state(false);
  let rollingBack = $state<string | null>(null);

  async function toggleRevisions() {
    revisionsOpen = !revisionsOpen;
    if (revisionsOpen && revisions == null && !revisionsLoading) {
      revisionsLoading = true;
      try {
        revisions = await api.listSaveRevisions(game.id);
      } catch (e) {
        toasts.show({
          kind: 'bad',
          label: 'LUDUSAVI',
          title: "Couldn't load save revisions",
          sub: String(e),
          catalog: fmtCatalog(game.catalog_number),
        });
        revisionsOpen = false;
      } finally {
        revisionsLoading = false;
      }
    }
  }

  async function rollBackTo(rev: SaveRevision) {
    if (rev.is_current || isRunning || rollingBack) return;
    const when = absDateTime(rev.when);
    if (
      !(await confirmDialog({
        label: 'LUDUSAVI · RESTORE',
        title: 'Restore this earlier save?',
        body:
          `Restore the backup from ${when} for "${game.game_name}". ` +
          `This replaces your current save files. Spool snapshots the ` +
          `restored save as the newest revision so it sticks (this uses one ` +
          `retention slot, dropping the oldest backup).`,
        confirmLabel: 'Restore save',
        accent: accentHex,
        catalog: fmtCatalog(game.catalog_number),
      }))
    )
      return;
    rollingBack = rev.name;
    try {
      await api.restoreSaveRevision(game.id, rev.name);
      toasts.show({
        kind: 'ok',
        label: 'LUDUSAVI · RESTORE',
        title: 'Earlier save restored',
        sub: `${game.game_name} · backup from ${when}`,
        catalog: fmtCatalog(game.catalog_number),
      });
      // The revision list changed (a new tip was pinned) — reload it.
      revisions = await api.listSaveRevisions(game.id).catch(() => revisions);
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'LUDUSAVI · RESTORE',
        title: "Couldn't restore",
        sub: String(e),
        catalog: fmtCatalog(game.catalog_number),
      });
    } finally {
      rollingBack = null;
    }
  }

  // Reset the picker when switching games so we don't show a stale list.
  $effect(() => {
    void game.id;
    revisionsOpen = false;
    revisions = null;
  });

  // "Sync now" — pull the latest cloud saves down to this device and restore
  // them to disk, without launching. Pull-only: never uploads. A true
  // divergence opens the conflict modal via `onPullConflict`.
  let pulling = $state(false);
  async function pullSaves() {
    if (pulling || isRunning) return;
    pulling = true;
    try {
      const r = await api.pullCloudSaves(game.id);
      const catalog = fmtCatalog(game.catalog_number);
      switch (r.outcome) {
        case 'pulled':
          toasts.show({
            kind: 'ok',
            label: 'LUDUSAVI · SYNC',
            title: 'Pulled latest saves',
            sub: `${game.game_name} · restored from the cloud`,
            catalog,
          });
          break;
        case 'up_to_date':
          toasts.show({
            kind: 'info',
            label: 'LUDUSAVI · SYNC',
            title: 'Already up to date',
            sub: `${game.game_name} · cloud matches this device`,
            catalog,
          });
          break;
        case 'local_newer':
          toasts.show({
            kind: 'info',
            label: 'LUDUSAVI · SYNC',
            title: 'Local saves are newer',
            sub: `${game.game_name} · nothing to pull — play to upload`,
            catalog,
          });
          break;
        case 'unconfigured':
          toasts.show({
            kind: 'warn',
            label: 'LUDUSAVI · SYNC',
            title: 'No cloud remote',
            sub: 'Configure cloud saves in Settings to sync',
            catalog,
          });
          break;
      }
    } catch (e) {
      const msg = String(e);
      // A true divergence — let the parent open the in-app conflict resolver
      // rather than dead-ending on a toast.
      if (/cloud sync conflict/i.test(msg) && onPullConflict) {
        onPullConflict(game.id);
      } else {
        toasts.show({
          kind: 'bad',
          label: 'LUDUSAVI · SYNC',
          title: "Couldn't sync",
          sub: msg,
          catalog: fmtCatalog(game.catalog_number),
        });
      }
    } finally {
      pulling = false;
    }
  }

  let isWindows = $state(false);
  onMount(async () => {
    isWindows = (await api.appPlatform()) === 'windows';
  });

  let generatingArmoury = $state(false);
  async function generateArmouryLauncher() {
    generatingArmoury = true;
    try {
      const path = await api.generateArmouryLauncher(game.id);
      const sep = path.includes('\\') ? '\\' : '/';
      const idx = path.lastIndexOf(sep);
      const dir = idx > 0 ? path.slice(0, idx) : path;
      toasts.show({
        kind: 'ok',
        label: 'ARMOURY CRATE',
        title: 'Launcher generated',
        sub: `In Armoury Crate: Library → Manage Library → Add → browse to ${path}`,
        catalog: fmtCatalog(game.catalog_number),
        duration: 0,
        cta: {
          label: 'Open folder',
          onClick: () => {
            api.openPath(dir).catch((e) => console.error('[launcher] open folder failed:', e));
          },
        },
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'ARMOURY CRATE · FAILED',
        title: "Couldn't generate launcher",
        sub: String(e),
        catalog: fmtCatalog(game.catalog_number),
      });
    } finally {
      generatingArmoury = false;
    }
  }

  let addingToSteam = $state(false);
  async function addToSteam() {
    addingToSteam = true;
    try {
      const result = await api.addToSteam(game.id);
      const extras = result.extras_placed.length
        ? ` · ${result.extras_placed.join(', ')} art placed`
        : '';
      toasts.show({
        kind: 'ok',
        label: 'STEAM',
        title: 'Added to Steam',
        sub: `Restart Steam to see "${game.game_name}" in your library${extras}.`,
        catalog: fmtCatalog(game.catalog_number),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'STEAM · FAILED',
        title: "Couldn't add to Steam",
        sub: String(e),
      });
    } finally {
      addingToSteam = false;
    }
  }

</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-bg-0">
  <!-- Hero -->
  <div
    class="relative h-[280px] shrink-0 overflow-hidden border-b border-line-1"
    style:background="linear-gradient(135deg, color-mix(in srgb, {accentHex} 22%, var(--color-bg-1)) 0%, var(--color-bg-0) 100%)"
  >
    <!-- Hero image (when available) — full-bleed, fades into bg at bottom -->
    {#if heroUrl}
      <img
        src={heroUrl}
        alt=""
        class="absolute inset-0 h-full w-full object-cover object-center"
      />
      <!-- Dark scrim so title and controls stay legible over any image -->
      <div
        class="pointer-events-none absolute inset-0"
        style:background="linear-gradient(180deg, rgb(0 0 0 / 0.35) 0%, rgb(0 0 0 / 0.55) 100%)"
      ></div>
    {:else}
      <!-- Fallback: accent-driven decorative circles when no hero art -->
      <div
        class="absolute right-[-120px] top-[-80px] h-[420px] w-[420px] rounded-full border"
        style:border-color="color-mix(in srgb, {accentHex} 13%, transparent)"
        style:background="radial-gradient(circle at 35% 35%, color-mix(in srgb, {accentHex} 20%, transparent), transparent 55%)"
      ></div>
      <div
        class="absolute right-[-40px] top-[30px] h-[260px] w-[260px] rounded-full border border-dashed"
        style:border-color="color-mix(in srgb, {accentHex} 20%, transparent)"
      ></div>
      <!-- subtle film grain -->
      <div
        class="pointer-events-none absolute inset-0 opacity-40 mix-blend-overlay"
        style:background-image="radial-gradient(rgb(255 255 255 / 0.05) 1px, transparent 1px)"
        style:background-size="3px 3px"
      ></div>
    {/if}

    <!-- tape strip across top (always visible) -->
    <div
      class="absolute inset-x-0 top-0 h-1"
      style:background="linear-gradient(90deg, {accentHex} 0%, color-mix(in srgb, {accentHex} 60%, transparent) 50%, {accentHex} 100%)"
    ></div>

    <!-- bottom fade to bg-0 (always — blends image or gradient into the body) -->
    <div
      class="pointer-events-none absolute inset-0"
      style:background="linear-gradient(180deg, transparent 40%, var(--color-bg-0) 100%)"
    ></div>

    <!-- Content -->
    <div class="absolute inset-x-7 bottom-[22px] top-[26px] flex flex-col justify-between">
      <div class="flex items-center gap-2.5">
        <CatalogId id={fmtCatalog(game.catalog_number)} accent={accentHex} />
        {#if game.genres.length > 0}
          <MonoLabel size={10}>{game.genres[0].toUpperCase()}</MonoLabel>
        {/if}
      </div>

      <div>
        <h1
          data-testid="game-title"
          class="font-display max-w-[720px] text-[44px] font-bold leading-[1.04] text-ink-0 text-balance"
          style:letter-spacing="-0.025em"
          style:text-shadow="0 2px 16px rgb(0 0 0 / 0.4)"
        >
          {game.game_name}
        </h1>

        <div class="mt-3.5 flex items-center gap-3.5">
          {#if peerSource}
            <!-- Peer-sourced row: Download (from another device on the LAN)
                 replaces Play. Shows live progress + Cancel while in flight. -->
            {#if peerInflight && peerDownload}
              <div class="flex flex-col gap-1.5" style:min-width="260px">
                <div class="flex items-center gap-2.5">
                  <span class="font-sans text-[length:var(--text-base)] font-semibold text-ink-0">
                    Downloading…
                  </span>
                  <button
                    type="button"
                    onclick={() => onCancelDownload?.()}
                    class="inline-flex items-center gap-1 rounded-sm border border-line-2 bg-bg-2 px-2 py-1 text-[11px] text-ink-2 transition-colors hover:border-bad/60 hover:text-bad"
                    title="Cancel install"
                  >
                    <X size={12} />
                    Cancel
                  </button>
                </div>
                <div class="h-1.5 w-full overflow-hidden rounded-full bg-bg-2">
                  <div
                    class="h-full transition-[width] duration-150 ease-out"
                    style:width={peerDownload.bytes_total > 0
                      ? Math.min(100, (peerDownload.bytes_done / peerDownload.bytes_total) * 100) + '%'
                      : '0%'}
                    style:background={accentHex}
                  ></div>
                </div>
                <div class="font-mono flex justify-between gap-2 text-[10px] tracking-[0.04em] text-ink-3">
                  <span class="truncate" title={peerDownload.current_file}>
                    {peerDownload.current_file || '…'}
                  </span>
                  <span class="shrink-0 whitespace-nowrap">
                    {fmtRate(peerDownload.bytes_per_second)}
                    {#if peerDownload.bytes_total > 0}
                      · {Math.round((peerDownload.bytes_done / peerDownload.bytes_total) * 100)}%
                    {/if}
                  </span>
                </div>
              </div>
            {:else}
              <button
                type="button"
                data-testid="download-button"
                data-gp-autofocus={autofocusPlay ? '' : undefined}
                onclick={startDownload}
                disabled={!peerSource.shareable || anyInstallBusy || !onDownload}
                class="font-sans inline-flex items-center gap-2.5 rounded-md border-none font-semibold transition-opacity"
                style:height="var(--control-h)"
                style:padding-inline="calc(var(--space-unit) * 4)"
                style:font-size="var(--text-base)"
                class:cursor-pointer={peerSource.shareable && !anyInstallBusy && !!onDownload}
                class:cursor-not-allowed={!peerSource.shareable || anyInstallBusy || !onDownload}
                class:opacity-70={!peerSource.shareable || anyInstallBusy || !onDownload}
                style:background={accentHex}
                style:color="#0b0c0e"
                style:box-shadow="0 6px 20px color-mix(in srgb, {accentHex} 26%, transparent)"
                title={!peerSource.shareable
                  ? 'Source device has no install folder configured for this game'
                  : anyInstallBusy
                    ? 'Another install is in progress'
                    : `Download from ${peerSource.device_name}`}
              >
                <Download size={16} />
                {peerStarting ? 'Starting…' : 'Download'}
              </button>
            {/if}
          {:else}
            <button
              type="button"
              data-testid="play-button"
              data-gp-autofocus={autofocusPlay && canPlay ? '' : undefined}
              onclick={launch}
              disabled={!canPlay}
              class="font-sans inline-flex items-center gap-2.5 rounded-md border-none font-semibold transition-opacity"
              style:height="var(--control-h)"
              style:padding-inline="calc(var(--space-unit) * 4)"
              style:font-size="var(--text-base)"
              class:cursor-pointer={canPlay}
              class:cursor-not-allowed={!canPlay}
              class:opacity-70={!canPlay}
              style:background={accentHex}
              style:color="#0b0c0e"
              style:box-shadow="0 6px 20px color-mix(in srgb, {accentHex} 26%, transparent)"
              title={!game.installed
                ? 'Not installed — reinstall to play'
                : !game.exe_path
                  ? 'No executable set'
                  : isRunning
                    ? playLabel
                    : 'Restore saves, launch game, back up on exit'}
            >
              <Play size={16} fill="currentColor" />
              {playLabel}
            </button>

            {#if !game.installed}
              <!-- Uninstalled with no peer source: Play is greyed; offer a
                   Reinstall affordance that opens the Add flow (which reuses
                   this same library entry). -->
              <button
                type="button"
                data-testid="reinstall-button"
                data-gp-autofocus=""
                onclick={() => openView('add', { reinstall: game.id })}
                class="font-sans inline-flex cursor-pointer items-center gap-2 rounded-md font-semibold transition-colors"
                style:height="var(--control-h)"
                style:padding-inline="calc(var(--space-unit) * 4)"
                style:font-size="var(--text-base)"
                style:color={accentHex}
                style:background="transparent"
                style:border="1px solid color-mix(in srgb, {accentHex} 45%, transparent)"
                title="Add the game again to reinstall — your saves, playtime and artwork are kept"
              >
                <HardDriveDownload size={15} />
                Reinstall…
              </button>
            {/if}
          {/if}

          {#if peerSource}
            <div class="flex flex-col gap-px">
              <MonoLabel size={9.5}>
                <span style:color={accentHex}>FROM · {peerSource.device_name.toUpperCase()}</span>
              </MonoLabel>
              <span class="font-mono text-[11.5px] tracking-[0.04em] text-ink-2">
                {fmtSize(game.install_size_mb)} · LAN download
              </span>
            </div>
          {:else}
            <div class="flex flex-col gap-px">
              <MonoLabel size={9.5}>
                <span style:color={accentHex}>
                  LAST · {game.last_played_at ? relDate(game.last_played_at).toUpperCase() : 'NEVER'}
                </span>
              </MonoLabel>
              <span
                class="font-mono text-[11.5px] tracking-[0.04em] text-ink-2"
              >
                {fmtPlaytime(game.playtime_minutes)} · {game.save_backup_count} backup{game.save_backup_count === 1 ? '' : 's'}
              </span>
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>

  <!-- Stats strip -->
  <div class="grid shrink-0 grid-cols-4 border-b border-line-1 px-7 py-4">
    {#snippet stat(label: string, value: string, sub: string, first: boolean = false)}
      <div class="px-[18px] {first ? '' : 'border-l border-dashed border-line-1'}">
        <MonoLabel size={9.5}>{label}</MonoLabel>
        <div
          class="font-display mt-1 text-[20px] font-semibold text-ink-0"
          style:letter-spacing="-0.015em"
        >
          {value}
        </div>
        <div
          class="font-mono mt-0.5 text-[10.5px] tracking-[0.04em] text-ink-2"
        >
          {sub}
        </div>
      </div>
    {/snippet}
    {@render stat(
      'Last played',
      game.last_played_at ? relDate(game.last_played_at) : 'Never',
      game.last_played_at ? absDateTime(game.last_played_at) : '—',
      true,
    )}
    {@render stat(
      'Playtime',
      fmtPlaytime(game.playtime_minutes),
      game.playtime_minutes > 0 ? 'across sessions' : 'no sessions yet',
    )}
    {@render stat(
      'Install size',
      fmtSize(game.install_size_mb),
      game.exe_path ? 'on disk' : '—',
    )}
    {@render stat(
      'Saves',
      game.save_backup_count > 0
        ? `${game.save_backup_count} backup${game.save_backup_count === 1 ? '' : 's'}`
        : '—',
      game.save_backup_count > 0
        ? game.save_backup_size_mb > 0
          ? `${fmtSize(game.save_backup_size_mb)} · ${relDate(game.save_last_backed_up_at)}`
          : relDate(game.save_last_backed_up_at)
        : 'no backups yet',
    )}
  </div>

  <!-- Action toolbar — hidden for synthetic peer rows (no local entry to act on) -->
  {#if !isSyntheticPeer}
  <div class="flex shrink-0 items-center gap-1.5 border-b border-line-1 px-7 py-3">
    <Btn variant="ghost" onclick={openFolder} disabled={!folderForGame(game)}>
      {#snippet icon()}<Folder size={14} />{/snippet}
      Open folder
    </Btn>
    {#if isWindows}
      <Btn
        variant="ghost"
        onclick={generateArmouryLauncher}
        disabled={!game.exe_path || generatingArmoury}
      >
        {#snippet icon()}<Sparkles size={14} />{/snippet}
        {generatingArmoury ? 'Generating…' : 'Armoury Crate'}
      </Btn>
    {/if}
    <Btn
      variant="ghost"
      onclick={addToSteam}
      disabled={!game.exe_path || addingToSteam}
    >
      {#snippet icon()}<Play size={14} />{/snippet}
      {addingToSteam ? 'Adding…' : 'Add to Steam'}
    </Btn>
    {#if cloudConfigured}
      <!-- Pull the latest cloud saves down to this device without launching. -->
      <Btn variant="ghost" onclick={pullSaves} disabled={pulling || isRunning}>
        {#snippet icon()}<CloudDownload size={14} />{/snippet}
        {pulling ? 'Syncing…' : 'Sync now'}
      </Btn>
    {/if}
    <div class="flex-1"></div>
    <Btn variant="ghost" onclick={() => openView('edit', { id: game.id })}>
      {#snippet icon()}<Pencil size={14} />{/snippet}
      Edit
    </Btn>
    <Btn variant="danger" onclick={() => removeGameDialog.request(game)}>
      {#snippet icon()}<Trash2 size={14} />{/snippet}
      Remove
    </Btn>
  </div>
  {/if}

  <!-- Two-column body (scrolls independently so the hero + Play button stay
       visible on short displays) -->
  <div
    class="grid min-h-0 flex-1 gap-3.5 overflow-y-auto px-7 pb-7 pt-5"
    style:grid-template-columns="minmax(0, 1.4fr) minmax(0, 1fr)"
    style:align-content="start"
  >
    <div class="flex min-w-0 flex-col gap-3.5">
      <!-- About -->
      <DetailCard title="ABOUT" {accent}>
        {#if game.description || game.genres.length > 0}
          {#if game.description}
            <p class="m-0 text-[13px] leading-relaxed text-ink-1">
              {game.description}
            </p>
          {/if}
          {#if game.genres.length > 0}
            <div class="mt-3 flex flex-wrap gap-1.5">
              {#each game.genres as g (g)}
                <span
                  class="inline-flex items-center rounded-sm border border-line-2 bg-bg-2 px-2 py-px text-[11px] text-ink-1"
                >
                  {g}
                </span>
              {/each}
            </div>
          {/if}
        {:else}
          <p class="m-0 text-[12.5px] text-ink-3">
            No description on file. Spool fills this in from the Steam store when a game is
            identified.
          </p>
        {/if}
      </DetailCard>

      <!-- Saves -->
      <DetailCard title="SAVE BACKUP · LUDUSAVI" {accent}>
        {#if game.save_backup_count > 0}
          <div class="grid grid-cols-3 gap-[18px]">
            {#snippet stat(label: string, value: string, sub: string)}
              <div>
                <MonoLabel size={9}>{label}</MonoLabel>
                <div
                  class="font-display mt-0.5 text-[18px] font-semibold text-ink-0"
                  style:letter-spacing="-0.01em"
                >
                  {value}
                </div>
                <div
                  class="font-mono mt-0.5 text-[10.5px] tracking-[0.04em] text-ink-3"
                >
                  {sub}
                </div>
              </div>
            {/snippet}
            {@render stat('LAST BACKUP', relDate(game.save_last_backed_up_at), absDateTime(game.save_last_backed_up_at))}
            {@render stat('REVISIONS', `${game.save_backup_count}`, 'kept by ludusavi')}
            {@render stat(
              'SAVE SIZE',
              game.save_backup_size_mb > 0 ? fmtSize(game.save_backup_size_mb) : '—',
              game.save_backup_size_mb > 0 ? 'latest backup' : 'not measured yet',
            )}
          </div>
          <div
            class="mt-3 flex items-center gap-2 rounded-sm border px-3 py-2 text-[11.5px] text-ink-1"
            style:border-color="color-mix(in srgb, var(--color-ok) 20%, transparent)"
            style:background="rgb(126 226 164 / 0.06)"
          >
            <Cloud size={12} class="text-ok" />
            Saves restore before launch and back up on exit automatically.
          </div>

          {#if game.save_backup_count > 1}
            <div class="mt-3 border-t border-dashed border-line-1 pt-3">
              <button
                type="button"
                onclick={toggleRevisions}
                class="flex items-center gap-1.5 text-[11.5px] text-ink-2 transition-colors hover:text-ink-0"
                aria-expanded={revisionsOpen}
              >
                <RotateCcw size={12} />
                Restore an earlier save
                <ChevronDown
                  size={12}
                  class="transition-transform {revisionsOpen ? 'rotate-180' : ''}"
                />
              </button>

              {#if revisionsOpen}
                <div class="mt-2 flex flex-col gap-1">
                  {#if revisionsLoading}
                    <div class="text-[11px] text-ink-3">Loading revisions…</div>
                  {:else if revisions && revisions.length > 0}
                    {#each revisions as rev (rev.name)}
                      <div
                        class="flex items-center justify-between gap-2 rounded-sm border border-line-1 bg-bg-2 px-2.5 py-1.5"
                      >
                        <div class="min-w-0">
                          <div class="truncate text-[12px] text-ink-0">
                            {absDateTime(rev.when)}
                          </div>
                          <div class="font-mono text-[10px] tracking-[0.02em] text-ink-3">
                            {relDate(rev.when)}
                          </div>
                        </div>
                        {#if rev.is_current}
                          <span
                            class="font-mono shrink-0 text-[9.5px] uppercase tracking-[0.08em] text-ok"
                          >
                            Current
                          </span>
                        {:else}
                          <button
                            type="button"
                            onclick={() => rollBackTo(rev)}
                            disabled={isRunning || rollingBack != null}
                            class="shrink-0 rounded-sm border border-line-2 px-2 py-1 text-[11px] text-ink-1 transition-colors hover:border-line-3 hover:text-ink-0 disabled:cursor-not-allowed disabled:opacity-40"
                            title={isRunning
                              ? 'Stop the game before restoring an earlier save'
                              : `Restore the backup from ${absDateTime(rev.when)}`}
                          >
                            {rollingBack === rev.name ? 'Restoring…' : 'Restore'}
                          </button>
                        {/if}
                      </div>
                    {/each}
                    <div class="font-mono mt-0.5 text-[10px] leading-relaxed text-ink-3">
                      Restoring snapshots the chosen save as the newest
                      revision, so it survives the next launch.
                    </div>
                  {:else}
                    <div class="text-[11px] text-ink-3">No earlier revisions.</div>
                  {/if}
                </div>
              {/if}
            </div>
          {/if}
        {:else if game.save_paths.length > 0}
          <div class="flex items-start gap-2.5 text-[12.5px] text-ink-2">
            <Cloud size={14} class="mt-0.5 shrink-0 text-ink-3" />
            <div class="min-w-0">
              <div>No backups yet — Spool will create one the first time you launch.</div>
              <div class="font-mono mt-1.5 text-[10.5px] tracking-[0.02em] text-ink-3">
                Will track:
                <span class="text-ink-2">{game.save_paths[0]}</span>
              </div>
            </div>
          </div>
        {:else}
          <div class="flex items-center gap-2.5 text-[12.5px] text-ink-2">
            <Cloud size={14} class="text-ink-3" />
            No save info from ludusavi — saves won't be backed up automatically.
          </div>
        {/if}
      </DetailCard>
    </div>

    <div class="flex min-w-0 flex-col gap-3.5">
      <CrossDeviceActivityCard {game} {accent} />

      <DetailCard title="ENTRY · DETAILS" {accent}>
        <div class="flex flex-col">
          {#snippet row(
            label: string,
            value: string,
            mono: boolean = false,
            copy: boolean = false,
            last: boolean = false,
          )}
            <div
              class="grid items-center gap-2.5 py-2"
              class:border-b={!last}
              class:border-dashed={!last}
              class:border-line-1={!last}
              style:grid-template-columns="94px 1fr auto"
            >
              <div
                class="font-mono text-[9.5px] uppercase tracking-[0.1em] text-ink-3"
              >
                {label}
              </div>
              <div
                class="truncate text-ink-0"
                class:font-mono={mono}
                class:text-[11.5px]={mono}
                class:text-[12.5px]={!mono}
                title={value}
              >
                {value}
              </div>
              {#if copy}
                <button
                  type="button"
                  onclick={() => copyToClipboard(value)}
                  class="inline-flex p-1 text-ink-3 transition-colors hover:text-ink-0"
                  title="Copy"
                  aria-label="Copy {label}"
                >
                  <Copy size={12} />
                </button>
              {:else}
                <span></span>
              {/if}
            </div>
          {/snippet}
          {@render row('Developer', game.developer || '—')}
          {@render row('Publisher', game.publisher || '—')}
          {@render row('Released', absDate(game.release_date))}
          {@render row('Added', absDate(game.added_at))}
          {@render row('Executable', game.exe_path || '—', true, !!game.exe_path)}
          {@render row(
            'Install',
            folderForGame(game) ?? '—',
            true,
            !!folderForGame(game),
          )}
          {#if game.steam_id != null}
            {@render row('Steam ID', `${game.steam_id}`, true, true)}
          {/if}
          {@render row('Source', game.install_source, false, false, true)}
        </div>
      </DetailCard>
    </div>
  </div>
</div>
