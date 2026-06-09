import { onMount } from 'svelte';
import { SvelteSet } from 'svelte/reactivity';
import { listen } from '@tauri-apps/api/event';
import { api } from '$lib/api';
import { fmtCatalog } from '$lib/format';
import { downloadIsActive, downloadPercent, liveUploads } from '$lib/transfers';
import { toasts } from '$lib/toasts.svelte';
import { startUpdateChecks } from '$lib/updater';
import type {
  DisplayGame,
  DownloadProgress,
  GameEntry,
  LanPeer,
  PeerGame,
  PeerSource,
  RunPhase,
  RunPhaseEvent,
  SavesBackupEvent,
  SyncStatus,
  UploadSnapshot,
} from '$lib/types';

// ── Pure filter helper — exported so tests can call it without a component ──
/** Filter and sort the game list by the current filter + search query.
 *  Operates on `DisplayGame` so it covers both local entries and the synthetic
 *  peer rows merged into the sidebar (which carry null timestamps + 0 playtime,
 *  so they fall out of the Recent/Played tabs on their own). */
export function filterGames(
  games: DisplayGame[],
  filter: 'all' | 'recent' | 'played',
  searchQuery: string,
): DisplayGame[] {
  let list = games.slice();
  if (filter === 'recent') {
    list = list
      .filter((g) => g.last_played_at != null || g.added_at != null)
      .sort((a, b) => {
        const at = a.last_played_at ?? a.added_at ?? '';
        const bt = b.last_played_at ?? b.added_at ?? '';
        return bt.localeCompare(at);
      });
  } else if (filter === 'played') {
    list = list.filter((g) => g.playtime_minutes > 0);
  }
  if (searchQuery.trim()) {
    const q = searchQuery.toLowerCase();
    list = list.filter((g) => g.game_name.toLowerCase().includes(q));
  }
  return list;
}

// ── Peer-game merge helpers ────────────────────────────────────────────────
// These mirror the Rust `find_reusable_entry` match rule (library.rs) on the
// frontend so the merged sidebar dedups exactly the way an actual install
// would. Pure + module-level so they're unit-testable without a component.

/** What a peer game is matched against — only the two fields the match rule
 *  reads, so both `GameEntry` and `PeerGame` satisfy it. */
type Matchable = { steam_id: number | null; game_name: string };

/** Prefix on a synthetic "available on LAN" row's id. Minted by
 *  `syntheticPeerEntry`, tested by `isSyntheticPeerId` — the single source of
 *  truth for "this DisplayGame has no real library entry behind it". */
const PEER_ID_PREFIX = 'peer:';

/** True for a synthetic peer-only row (no backing DB entry on this device), so
 *  callers can skip backend ops keyed on a real id. Note an *uninstalled local*
 *  row that a peer can supply keeps its real id and returns false here. */
export function isSyntheticPeerId(id: string): boolean {
  return id.startsWith(PEER_ID_PREFIX);
}

/** The subset of a `LanPeer` the merge needs — no `last_seen_ago_secs`/
 *  `game_count`, so it stays stable across discovery heartbeats. */
type PeerMeta = Pick<LanPeer, 'device_id' | 'device_name' | 'addr' | 'file_server_port'>;

/** A fetched peer catalogue plus the metadata needed to build a `PeerSource`
 *  from it — captured at fetch time so the merge never depends on the churny
 *  `lanPeers` array (whose `last_seen_ago_secs` ticks every announce). */
export type PeerCatalog = { peer: PeerMeta; games: PeerGame[] };

/**
 * Find the local entry a peer game corresponds to: steam_id first, then exact
 * game name with a steam_id conflict guard (two known, differing steam_ids
 * never merge). Returns null when there's no match.
 */
export function matchLocal<T extends Matchable>(candidates: T[], pg: Matchable): T | null {
  if (pg.steam_id != null) {
    const byId = candidates.find((g) => g.steam_id === pg.steam_id);
    if (byId) return byId;
  }
  return (
    candidates.find(
      (g) =>
        g.game_name === pg.game_name &&
        !(g.steam_id != null && pg.steam_id != null && g.steam_id !== pg.steam_id),
    ) ?? null
  );
}

/** Stable key for a peer game with no local match: steam_id when known
 *  (stable across peers), else the normalized name. Drives both the synthetic
 *  row id and cross-peer dedup. */
export function dedupKey(pg: Matchable): string {
  return pg.steam_id != null ? `sid:${pg.steam_id}` : `name:${pg.game_name.toLowerCase()}`;
}

/** The shape the source helpers below read — satisfied by `DisplayGame` and by
 *  any `{ peer_source?, peer_sources? }` literal. */
type Sourced = { peer_source?: PeerSource; peer_sources?: PeerSource[] };

/** Every device a row can be fetched from. Normalises `peer_sources` (the source
 *  of truth) with a fallback to the lone `peer_source`, so callers don't each
 *  re-spell the `?? [peer_source]` dance. Includes non-shareable sources — use
 *  `shareableSources` when you need the devices that can actually serve a copy. */
export function sourcesOf(game: Sourced): PeerSource[] {
  return game.peer_sources ?? (game.peer_source ? [game.peer_source] : []);
}

/** The sources a row can actually be installed from — `shareable` only. A peer
 *  that lists the game but has no folder to stream (`shareable: false`) is kept
 *  in `peer_sources` for context (it explains a disabled Download button) but is
 *  never a real download candidate. */
export function shareableSources(game: Sourced): PeerSource[] {
  return sourcesOf(game).filter((s) => s.shareable);
}

/** True when an in-flight download belongs to one of the devices a row can be
 *  fetched from. Matched against *every* source (not just the primary) so the
 *  detail page's inline progress still binds to the row when the user picked a
 *  non-primary device in the source chooser. */
export function downloadMatchesGame(
  download: Pick<DownloadProgress, 'source_game_id' | 'source_device_id'> | null | undefined,
  game: Sourced,
): boolean {
  if (!download) return false;
  return sourcesOf(game).some(
    (s) => s.source_game_id === download.source_game_id && s.device_id === download.source_device_id,
  );
}

function toPeerSource(peer: PeerMeta, pg: PeerGame): PeerSource {
  return {
    device_id: peer.device_id,
    device_name: peer.device_name,
    addr: peer.addr,
    file_server_port: peer.file_server_port,
    source_game_id: pg.id,
    shareable: pg.shareable,
  };
}

/**
 * A `GameEntry`-shaped shell for a peer game that has no local entry. The id is
 * `peer:<dedupKey>` so it's stable across discovery heartbeats and across which
 * peer happens to supply it. Backend-only fields are zeroed/blanked — the
 * uninstalled + no-exe code paths already render those gracefully.
 */
function syntheticPeerEntry(peer: PeerMeta, pg: PeerGame): DisplayGame {
  const source = toPeerSource(peer, pg);
  return {
    id: `${PEER_ID_PREFIX}${dedupKey(pg)}`,
    catalog_number: 0,
    game_name: pg.game_name,
    exe_path: '',
    safe_name: '',
    cover_image_path: null,
    hero_image_path: null,
    added_at: null,
    last_played_at: null,
    launcher_exe_path: null,
    game_folder_path: null,
    installed: false,
    run_as_admin: false,
    use_proton: false,
    proton_version_path: null,
    wine_prefix_path: null,
    launch_args: null,
    description: '',
    developer: pg.developer,
    publisher: pg.publisher,
    genres: pg.genres,
    release_date: pg.release_date,
    install_size_mb: pg.install_size_mb,
    playtime_minutes: 0,
    lan_shared: false,
    lan_share_folder: null,
    save_backup_count: 0,
    save_last_backed_up_at: null,
    save_backup_size_mb: 0,
    install_source: 'lan',
    lan_install_source_device_name: peer.device_name,
    lan_install_source_device_id: peer.device_id,
    steam_id: pg.steam_id,
    gog_id: pg.gog_id,
    lutris_slug: pg.lutris_slug,
    manifest_install_dir: null,
    save_paths: [],
    custom_save: null,
    manifest_override: null,
    accent_color: null,
    sync_badge: null,
    cloud_sync_baseline: null,
    save_last_backer_device: null,
    save_cloud_revision_at: null,
    steam_app_id: null,
    peer_source: source,
    peer_sources: [source],
  };
}

/** Append a source to a row's `peer_sources`, skipping a device already listed
 *  (a peer can't sensibly offer the same game twice; guards against a double
 *  announce). Initialises the array on first use. */
function addPeerSource(row: DisplayGame, ps: PeerSource) {
  row.peer_sources ??= [];
  if (!row.peer_sources.some((s) => s.device_id === ps.device_id)) {
    row.peer_sources.push(ps);
  }
}

/**
 * Merge local games with peer catalogues into one sidebar list.
 *   - installed locally → drop the peer copy (no duplicate row)
 *   - uninstalled locally → annotate that existing row as downloadable
 *   - no local entry → a synthetic "available on LAN" row (deduped across peers)
 * Pure (takes plain data) so it's unit-testable; the store wraps it in a derived.
 *
 * Every device that offers a game is collected into the row's `peer_sources`;
 * `peer_source` is then the primary (first after a stable sort by device name),
 * so the source no longer depends on the non-deterministic order peers were
 * discovered in. The Download action reads `peer_sources` to offer a chooser
 * when more than one device has the game.
 *
 * Depends only on the catalogues (each carrying its peer's stable metadata), not
 * on the live `lanPeers` array — so a discovery heartbeat that only bumps a
 * peer's `last_seen_ago_secs` doesn't force a re-merge / sidebar re-render.
 */
export function mergeDisplayGames(
  games: GameEntry[],
  peerCatalogs: Record<string, PeerCatalog>,
): DisplayGame[] {
  const installed = games.filter((g) => g.installed);
  const uninstalled = games.filter((g) => !g.installed);
  const out: DisplayGame[] = games.map((g) => ({ ...g }));
  // Synthetic rows already created, keyed by dedupKey, so later peers append to
  // the same row instead of spawning a duplicate (plain object, not a Set — this
  // is a pure helper, nothing reactive).
  const synthByKey: Record<string, DisplayGame> = {};

  for (const { peer, games: catalog } of Object.values(peerCatalogs)) {
    for (const pg of catalog) {
      // (1) already installed here → no duplicate.
      if (matchLocal(installed, pg)) continue;
      const source = toPeerSource(peer, pg);
      // (2) uninstalled here → make that row downloadable, collecting every
      // device that can supply it.
      const local = matchLocal(uninstalled, pg);
      if (local) {
        const row = out.find((r) => r.id === local.id);
        if (row) {
          addPeerSource(row, source);
          // The entry is uninstalled here, so its own recorded install size is
          // meaningless (0 after a Spool uninstall, or a stale value if the
          // folder vanished out-of-band). Show a peer's size — the bytes that
          // will download (any peer's is close enough; they share the game).
          if (pg.install_size_mb > 0) row.install_size_mb = pg.install_size_mb;
        }
        continue;
      }
      // (3) no local entry → one synthetic row per game, accumulating sources.
      const key = dedupKey(pg);
      const existing = synthByKey[key];
      if (existing) {
        addPeerSource(existing, source);
      } else {
        synthByKey[key] = syntheticPeerEntry(peer, pg);
        out.push(synthByKey[key]);
      }
    }
  }

  // Resolve the primary source per row. Rank shareable sources first so the
  // primary is one the user can actually download from whenever any device can
  // serve it — otherwise a non-shareable copy (peer opted in but has no folder)
  // could sort ahead and disable the Download button while a working copy
  // exists elsewhere. Within each group, a stable sort by device name (then id)
  // keeps the label and chooser default from shuffling between sessions.
  for (const row of out) {
    if (!row.peer_sources || row.peer_sources.length === 0) continue;
    row.peer_sources.sort(
      (a, b) =>
        Number(b.shareable) - Number(a.shareable) ||
        a.device_name.localeCompare(b.device_name) ||
        a.device_id.localeCompare(b.device_id),
    );
    row.peer_source = row.peer_sources[0];
  }
  return out;
}

// ── Controller ───────────────────────────────────────────────────────────────
export function createLibrary() {
  // Core library state
  let games = $state<GameEntry[]>([]);
  let loaded = $state(false);
  let error = $state<string | null>(null);

  // Selection + search
  let selectedId = $state<string | null>(null);
  let searchQuery = $state('');
  let filter = $state<'all' | 'recent' | 'played'>('all');
  let conflictGameId = $state<string | null>(null);
  // Set when a launch is blocked by another device that's *suspended*
  // mid-session — drives the "Play here instead" override modal.
  let suspendedConflict = $state<{ gameId: string; deviceName: string } | null>(null);

  // Run tracking
  let runningId = $state<string | null>(null);
  let runningPhase = $state<RunPhase | null>(null);

  // Game ids with a forced post-override backup in flight (drives the Play
  // button's "Backing up…" state). Usually holds at most one id — the backup lock
  // serialises them.
  const backupsInProgress = new SvelteSet<string>();

  // LAN state
  let lanPeers = $state<LanPeer[]>([]);
  let openPeer = $state<LanPeer | null>(null);
  let peerGames = $state<PeerGame[]>([]);
  let peerGamesLoading = $state(false);
  let peerGamesError = $state<string | null>(null);
  let activeDownload = $state<DownloadProgress | null>(null);
  let startingGameId = $state<string | null>(null);
  // Set when a Download targets a game offered by more than one device — drives
  // the source-chooser modal (issue #321). Null whenever the chooser is closed.
  let peerChoice = $state<{ game: DisplayGame; sources: PeerSource[] } | null>(null);
  // Per-device peer catalogues aggregated for the merged sidebar (keyed by
  // device_id; each value carries the peer's stable metadata + its games).
  // Populated lazily in the background; never blocks first paint. Mutated in
  // place (property add/delete) — its deep $state proxy makes those reactive,
  // so it never needs reassignment.
  const peerCatalogs = $state<Record<string, PeerCatalog>>({});
  // device_id → "addr:port" last fetched, so the heartbeat-driven
  // `lan:peers-changed` doesn't refetch peers whose endpoint hasn't moved.
  // Plain object (not a Map) — it only gates network calls, never rendering.
  const peerFetchKey: Record<string, string> = {};
  // When a peer-only download finishes, the synthetic `peer:` row it was shown
  // as is replaced by a real installed entry with a fresh id. This carries the
  // new id so `refresh()` can retarget selection onto it.
  let pendingSelectFollow: string | null = null;
  // Lives outside $state — changing it never affects rendering.
  const toastedDownloadTokens = new SvelteSet<string>();
  let activeUploads = $state<UploadSnapshot[]>([]);

  // Sync state
  let syncStatus = $state<SyncStatus>({
    reachability: 'unconfigured',
    server_version: null,
    error: null,
    last_ok_ago_secs: null,
  });

  // Derived
  // Local library merged with peer catalogues (auto-tracks games + peerCatalogs;
  // deliberately NOT lanPeers, whose freshness counter ticks every heartbeat).
  // Everything downstream — filtering, selection — reads this so peer rows are
  // first-class sidebar entries.
  const displayGames = $derived(mergeDisplayGames(games, peerCatalogs));
  const filteredGames = $derived(filterGames(displayGames, filter, searchQuery));
  const selectedGame = $derived(
    selectedId ? displayGames.find((g) => g.id === selectedId) ?? null : null,
  );
  // Sidebar filter-tab counts, computed once per displayGames change rather than
  // re-filtering three times on every render.
  const tabCounts = $derived({
    all: displayGames.length,
    recent: displayGames.filter((g) => g.last_played_at || g.added_at).length,
    played: displayGames.filter((g) => g.playtime_minutes > 0).length,
  });
  const syncOk = $derived(syncStatus.reachability === 'online');
  const syncOff = $derived(syncStatus.reachability === 'offline');
  const syncTitle = $derived(
    syncStatus.reachability === 'unconfigured'
      ? 'Cloud remote not configured — open Settings to set it up'
      : syncOk
        ? 'Cloud remote reachable'
        : `Cloud remote unreachable${syncStatus.error ? ` · ${syncStatus.error}` : ''}`,
  );
  const downloadActive = $derived(downloadIsActive(activeDownload));
  const downloadCount = $derived(downloadActive ? 1 : 0);
  const downloadPct = $derived(downloadPercent(activeDownload));
  const liveUploadList = $derived(liveUploads(activeUploads));
  const uploadCount = $derived(liveUploadList.length);
  const uploadPercent = $derived.by(() => {
    if (uploadCount === 0) return 0;
    const total = liveUploadList.reduce((s, u) => s + u.bytes_total, 0);
    const sent = liveUploadList.reduce((s, u) => s + u.bytes_sent, 0);
    return total > 0 ? Math.round((sent / total) * 100) : 0;
  });

  // Methods
  async function refresh() {
    try {
      games = await api.listGames();
      // A just-completed peer-only install: the synthetic `peer:` row the user
      // was viewing is gone (the game is now installed locally) — follow the
      // selection onto the real entry so the detail pane doesn't jump away.
      if (
        pendingSelectFollow &&
        selectedId?.startsWith('peer:') &&
        games.some((g) => g.id === pendingSelectFollow)
      ) {
        selectedId = pendingSelectFollow;
        pendingSelectFollow = null;
      }
      // Reconcile against the merged list (not just local games) so a valid
      // `peer:` selection isn't reset to the first row on every change.
      const list = displayGames;
      if (selectedId && !list.some((g) => g.id === selectedId)) {
        selectedId = list[0]?.id ?? null;
      } else if (!selectedId && list.length > 0) {
        selectedId = list[0].id;
      } else if (list.length === 0) {
        selectedId = null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loaded = true;
    }
  }

  /**
   * Fetch every browsable peer's catalogue into `peerCatalogs` for the merged
   * sidebar. Fire-and-forget — failures are isolated per peer (one offline peer
   * never blanks the rest), and the `peerFetchKey` cache skips peers whose
   * endpoint hasn't changed so the frequent `lan:peers-changed` heartbeat is a
   * no-op in the common case.
   */
  async function refreshPeerCatalogs() {
    const browsable = lanPeers.filter((p) => p.file_server_port > 0);
    const liveIds = browsable.map((p) => p.device_id);
    // Drop catalogues for peers that have gone away.
    for (const id of Object.keys(peerCatalogs)) {
      if (!liveIds.includes(id)) {
        delete peerCatalogs[id];
        delete peerFetchKey[id];
      }
    }
    await Promise.allSettled(
      browsable.map(async (p) => {
        const key = `${p.addr}:${p.file_server_port}`;
        if (peerFetchKey[p.device_id] === key) return; // already cached
        try {
          const games = await api.fetchPeerGames(p.addr, p.file_server_port);
          // Capture the peer's stable metadata alongside its games so the merge
          // never has to read the churny `lanPeers` array.
          peerCatalogs[p.device_id] = {
            peer: {
              device_id: p.device_id,
              device_name: p.device_name,
              addr: p.addr,
              file_server_port: p.file_server_port,
            },
            games,
          };
          peerFetchKey[p.device_id] = key;
        } catch (e) {
          delete peerCatalogs[p.device_id];
          delete peerFetchKey[p.device_id];
          console.warn('[lan] fetchPeerGames failed for', p.device_name, e);
        }
      }),
    );
  }

  async function refreshLanPeers() {
    try {
      lanPeers = await api.listLanPeers();
    } catch (e) {
      console.error('[lan] listLanPeers failed:', e);
    }
  }

  async function refreshActiveUploads() {
    try {
      activeUploads = await api.listActiveUploads();
    } catch (e) {
      console.error('[lan] listActiveUploads failed:', e);
    }
  }

  async function kickUpload(session: UploadSnapshot) {
    try {
      await api.cancelUpload(session.session_id);
    } catch (e) {
      console.error('[lan] cancelUpload failed:', e);
    }
  }

  async function openPeerView(peer: LanPeer) {
    openPeer = peer;
    peerGames = [];
    peerGamesError = null;
    if (peer.file_server_port === 0) {
      peerGamesError = 'This peer is discovery-only (no file server yet).';
      return;
    }
    peerGamesLoading = true;
    try {
      const games = await api.fetchPeerGames(peer.addr, peer.file_server_port);
      // Guard against a stale resolve: the user may have switched to a
      // different peer (or closed the drill-down) while this request was in
      // flight, in which case a newer call owns the view state.
      if (openPeer?.device_id !== peer.device_id) return;
      peerGames = games;
    } catch (e) {
      if (openPeer?.device_id !== peer.device_id) return;
      peerGamesError = String(e);
    } finally {
      if (openPeer?.device_id === peer.device_id) peerGamesLoading = false;
    }
  }

  function backToPeerList() {
    openPeer = null;
    peerGames = [];
    peerGamesError = null;
  }

  /** Called by LibraryDesktop's closeLanPopover to reset peer drill-down state. */
  function clearPeerView() {
    openPeer = null;
    peerGames = [];
    peerGamesError = null;
  }

  async function cancelActiveInstall() {
    if (!activeDownload) return;
    try {
      await api.cancelPeerInstall(activeDownload.install_token);
    } catch (e) {
      console.error('[lan] cancel install failed:', e);
    }
  }

  async function installFromPeer(peer: LanPeer, game: PeerGame) {
    if (
      activeDownload &&
      (activeDownload.status === 'starting' || activeDownload.status === 'transferring')
    ) {
      toasts.show({
        kind: 'warn',
        label: 'LAN',
        title: 'Another install is in progress',
        sub: `Finish ${activeDownload.game_name} first.`,
      });
      return;
    }
    startingGameId = game.id;
    try {
      await api.startPeerInstall(peer.addr, peer.file_server_port, game.id);
      toasts.show({
        kind: 'info',
        label: 'LAN',
        title: 'Install started',
        sub: `${game.game_name} · from ${peer.device_name}`,
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'LAN',
        title: 'Install failed to start',
        sub: String(e),
      });
    } finally {
      if (startingGameId === game.id) startingGameId = null;
    }
  }

  /**
   * The sources for a row that are usable *right now* — a device must be able to
   * serve the game (`shareable`) and still be announcing a file server
   * (`file_server_port > 0`). Drives the single-vs-chooser branch below, so a
   * non-shareable or dropped-off peer never becomes a download target.
   */
  function liveSourcesFor(g: DisplayGame): PeerSource[] {
    return shareableSources(g).filter((ps) => {
      const peer = lanPeers.find((p) => p.device_id === ps.device_id);
      return peer != null && peer.file_server_port > 0;
    });
  }

  /**
   * Resolve a `PeerSource` to its live peer and start the install. Shared by the
   * single-source fast path and the chooser's pick handler. `installFromPeer`
   * only reads id/game_name plus the display fields below; the rest of PeerGame
   * is reconstructed from the merged row.
   */
  async function installFromSource(g: DisplayGame, ps: PeerSource) {
    const peer = lanPeers.find((p) => p.device_id === ps.device_id);
    if (!peer || peer.file_server_port === 0) {
      toasts.show({
        kind: 'warn',
        label: 'LAN',
        title: 'Source device offline',
        sub: `${ps.device_name} is no longer sharing ${g.game_name} on the network.`,
      });
      return;
    }
    await installFromPeer(peer, {
      id: ps.source_game_id,
      catalog_number: g.catalog_number,
      game_name: g.game_name,
      developer: g.developer,
      publisher: g.publisher,
      genres: g.genres,
      install_size_mb: g.install_size_mb,
      release_date: g.release_date,
      steam_id: g.steam_id,
      gog_id: g.gog_id,
      lutris_slug: g.lutris_slug,
      shareable: ps.shareable,
    });
  }

  /**
   * Start a download for a merged sidebar row (the detail page's Download
   * button). With one live source it installs straight away; with several it
   * opens the source chooser so the user picks which device to pull from
   * (issue #321) rather than getting an arbitrary one. Toasts if every source
   * has dropped off the network since the row was rendered.
   */
  async function downloadGame(g: DisplayGame) {
    const sources = liveSourcesFor(g);
    if (sources.length === 0) {
      // No usable source: only nag if the row claimed one (else it's a plain
      // uninstalled game and Download shouldn't have been offered at all).
      if (sourcesOf(g).length > 0) {
        toasts.show({
          kind: 'warn',
          label: 'LAN',
          title: 'Source device offline',
          sub: `${g.game_name} is no longer shared on the network.`,
        });
      }
      return;
    }
    if (sources.length === 1) {
      await installFromSource(g, sources[0]);
      return;
    }
    peerChoice = { game: g, sources };
  }

  /**
   * Pick handler for the source chooser: dismiss the modal, then install from
   * the chosen device. Clears `peerChoice` first so a slow install start can't
   * leave the modal hanging open.
   */
  async function chooseDownloadSource(ps: PeerSource) {
    const choice = peerChoice;
    peerChoice = null;
    if (!choice) return;
    await installFromSource(choice.game, ps);
  }

  function showRunErrorToast(gameId: string, message: string) {
    const game = games.find((g) => g.id === gameId);
    const catalog = game ? fmtCatalog(game.catalog_number) : undefined;
    const subjectName = game?.game_name ?? 'this game';
    if (/cloud sync conflict/i.test(message)) {
      toasts.show({
        kind: 'warn',
        label: 'LUDUSAVI · CONFLICT',
        catalog,
        title: 'Cloud sync conflict',
        sub: `${subjectName} has different saves locally and in the cloud. Open Ludusavi to pick which to keep.`,
        cta: {
          label: 'Open Ludusavi',
          onClick: () => {
            api.openLudusaviGui().catch((e) => console.error('[ludusavi] open failed:', e));
          },
        },
      });
      return;
    }
    toasts.show({
      kind: 'bad',
      label: 'LAUNCH · FAILED',
      catalog,
      title: "Couldn't launch game",
      sub: message,
    });
  }

  // onMount: initial fetch + Tauri event subscriptions
  onMount(() => {
    refresh();
    // Spool hides to tray instead of quitting, so this webview can stay
    // mounted for days — poll for updates on an interval, not just once.
    const stopUpdateChecks = startUpdateChecks();

    let unlistenLibraryChanged: (() => void) | undefined;
    let unlistenRunPhase: (() => void) | undefined;
    let unlistenCloudNotice: (() => void) | undefined;
    let unlistenTrayIntro: (() => void) | undefined;
    let unlistenLanPeers: (() => void) | undefined;
    let unlistenLanDownload: (() => void) | undefined;
    let unlistenLanUploads: (() => void) | undefined;
    let unlistenSyncStatus: (() => void) | undefined;
    let unlistenSavesBackup: (() => void) | undefined;
    // Guards against a leaked subscription when the component unmounts before a
    // listen() promise resolves: the teardown sets this, and each handler below
    // immediately unlistens a late-resolving handle instead of storing it on a
    // now-orphaned local that nothing will ever call. (#291)
    let disposed = false;

    listen<string>('library:changed', () => refresh())
      .then((fn) => {
        if (disposed) fn();
        else unlistenLibraryChanged = fn;
      })
      .catch((e) => console.error('[library] listener failed:', e));

    listen<RunPhaseEvent>('run:phase', (event) => {
      const { game_id, phase, message, cloud_used } = event.payload;
      if (phase === 'done' || phase === 'error') {
        runningId = null;
        runningPhase = null;
      } else {
        runningId = game_id;
        runningPhase = phase;
      }
      if (phase === 'error') {
        // Only one launch-blocking modal is valid at a time. Clear both first
        // so a stale overlay from an earlier launch can't linger or stack with
        // a newly-set one; then set whichever (if any) this error maps to.
        conflictGameId = null;
        suspendedConflict = null;
        // Capture the device name by anchoring to the fixed suffix the Rust
        // side emits (runner.rs), rather than stopping at the first ". " —
        // device names can legitimately contain a dot. Both the "actively
        // playing elsewhere" and "unsynced session elsewhere" cases offer the
        // same "Play here anyway" override (re-runs the launch with steal).
        const overrideMatch =
          message?.match(
            /^Already playing on (.+?)(?=\. Close it there, or play here anyway)/,
          ) ??
          message?.match(
            /^Unsynced session on (.+?)(?=\. Its latest saves aren't in the cloud yet)/,
          );
        if (message && /cloud sync conflict/i.test(message)) {
          conflictGameId = game_id;
        } else if (overrideMatch) {
          // Another device holds (or hasn't synced) this game's session —
          // offer the override instead of a dead-end error toast.
          suspendedConflict = { gameId: game_id, deviceName: overrideMatch[1] };
        } else {
          showRunErrorToast(game_id, message ?? 'Game launch failed');
        }
      } else if (phase === 'done') {
        const game = games.find((g) => g.id === game_id);
        if (message) {
          // A message on `done` means backup succeeded but cloud upload failed.
          toasts.show({
            kind: 'warn',
            label: 'LUDUSAVI',
            title: 'Cloud upload failed',
            sub: game ? `${game.game_name} · ${message}` : message,
            catalog: game ? fmtCatalog(game.catalog_number) : undefined,
          });
        } else if (!cloud_used) {
          // No cloud remote configured — saves are safe locally but weren't synced.
          toasts.show({
            kind: 'info',
            label: 'LUDUSAVI',
            title: 'Saves backed up locally',
            sub: game
              ? `${game.game_name} · no cloud remote configured`
              : 'No cloud remote configured',
            catalog: game ? fmtCatalog(game.catalog_number) : undefined,
          });
        } else if (game) {
          toasts.show({
            kind: 'ok',
            label: 'LUDUSAVI',
            title: 'Saves backed up + synced',
            sub: `${game.game_name} · session complete`,
            catalog: fmtCatalog(game.catalog_number),
          });
        }
      }
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return undefined;
        }
        unlistenRunPhase = fn;
        return api.takePendingRun();
      })
      .then((pendingId) => {
        if (pendingId) {
          api
            .launchGame(pendingId)
            .catch((e) => console.error('[lifecycle] pending --run failed:', e));
        }
      })
      .catch((e) => console.error('[library] run-phase listener failed:', e));

    // Informational note when the backend auto-resolves a clean cloud
    // fast-forward (one side cleanly ahead). No modal — just a brief toast.
    // True divergence still surfaces via the run:phase 'error' → conflict modal.
    listen<string>('cloud:notice', (event) => {
      const message = event.payload;
      if (message) {
        toasts.show({ kind: 'ok', label: 'LUDUSAVI · SYNC', title: 'Saves synced', sub: message });
      }
    })
      .then((fn) => {
        if (disposed) fn();
        else unlistenCloudNotice = fn;
      })
      .catch((e) => console.error('[library] cloud-notice listener failed:', e));

    listen<null>('tray:first-hide', () => {
      toasts.show({
        kind: 'info',
        label: 'TRAY',
        title: 'Spool is still running',
        sub: 'Click the tray icon to bring the window back. You can quit fully from the tray menu.',
        duration: 0,
      });
    })
      .then((fn) => {
        if (disposed) fn();
        else unlistenTrayIntro = fn;
      })
      .catch((e) => console.error('[tray] intro listener failed:', e));

    refreshLanPeers().then(() => refreshPeerCatalogs());
    listen<null>('lan:peers-changed', () =>
      refreshLanPeers().then(() => refreshPeerCatalogs()),
    )
      .then((fn) => {
        if (disposed) fn();
        else unlistenLanPeers = fn;
      })
      .catch((e) => console.error('[lan] peers listener failed:', e));

    api
      .currentPeerDownload()
      .then((p) => {
        if (p) activeDownload = p;
      })
      .catch((e) => console.error('[lan] currentPeerDownload failed:', e));

    api
      .currentSyncStatus()
      .then((s) => (syncStatus = s))
      .catch((e) => console.error('[sync] currentSyncStatus failed:', e));
    listen<SyncStatus>('sync:status-changed', (event) => {
      syncStatus = event.payload;
    })
      .then((fn) => {
        if (disposed) fn();
        else unlistenSyncStatus = fn;
      })
      .catch((e) => console.error('[sync] status listener failed:', e));

    refreshActiveUploads();
    listen<null>('lan:uploads-changed', () => refreshActiveUploads())
      .then((fn) => {
        if (disposed) fn();
        else unlistenLanUploads = fn;
      })
      .catch((e) => console.error('[lan] uploads listener failed:', e));

    listen<DownloadProgress>('lan:download', (event) => {
      const p = event.payload;
      const isTerminal =
        p.status === 'done' || p.status === 'error' || p.status === 'canceled';
      const firstTerminal = isTerminal && !toastedDownloadTokens.has(p.install_token);
      if (firstTerminal) toastedDownloadTokens.add(p.install_token);
      activeDownload = p;
      if (p.status === 'done' && firstTerminal) {
        // The new entry replaces the synthetic peer row; carry its id so the
        // refresh below (and any racing library:changed) can follow selection.
        pendingSelectFollow = p.new_game_id;
        refresh();
        toasts.show({ kind: 'ok', label: 'LAN', title: 'Install complete',
          sub: `${p.game_name} · from ${p.source_device_name}` });
      } else if (p.status === 'error' && firstTerminal) {
        toasts.show({ kind: 'bad', label: 'LAN', title: 'Install failed',
          sub: p.message ?? `${p.game_name} could not be installed` });
      } else if (p.status === 'canceled' && firstTerminal) {
        toasts.show({ kind: 'info', label: 'LAN', title: 'Install cancelled',
          sub: `${p.game_name} · partial files cleaned up` });
      }
    })
      .then((fn) => {
        if (disposed) fn();
        else unlistenLanDownload = fn;
      })
      .catch((e) => console.error('[lan] download listener failed:', e));

    // Forced backup after a manifest-override change (set in the Saves editor).
    // Tracks the in-flight set for the Play button and toasts start/finish.
    listen<SavesBackupEvent>('saves:backup', (event) => {
      const { game_id, game_name, phase, cloud_synced } = event.payload;
      const game = games.find((g) => g.id === game_id);
      const catalog = game ? fmtCatalog(game.catalog_number) : undefined;
      if (phase === 'started') {
        backupsInProgress.add(game_id);
        toasts.show({
          kind: 'info',
          label: 'LUDUSAVI',
          title: 'Backing up saves',
          sub: `${game_name} · applying your changes`,
          catalog,
        });
        return;
      }
      backupsInProgress.delete(game_id);
      if (phase === 'failed') {
        toasts.show({
          kind: 'bad',
          label: 'LUDUSAVI · FAILED',
          title: 'Backup failed',
          sub: `${game_name} · your changes are saved; the next launch will back up`,
          catalog,
        });
      } else if (cloud_synced) {
        toasts.show({
          kind: 'ok',
          label: 'LUDUSAVI',
          title: 'Saves backed up & synced',
          sub: `${game_name} · cloud updated`,
          catalog,
        });
      } else {
        toasts.show({
          kind: 'warn',
          label: 'LUDUSAVI',
          title: 'Backed up locally',
          sub: `${game_name} · cloud sync pending`,
          catalog,
        });
      }
    })
      .then((fn) => {
        if (disposed) fn();
        else unlistenSavesBackup = fn;
      })
      .catch((e) => console.error('[library] saves-backup listener failed:', e));

    return () => {
      disposed = true;
      stopUpdateChecks();
      unlistenLibraryChanged?.();
      unlistenRunPhase?.();
      unlistenCloudNotice?.();
      unlistenTrayIntro?.();
      unlistenLanPeers?.();
      unlistenLanDownload?.();
      unlistenLanUploads?.();
      unlistenSyncStatus?.();
      unlistenSavesBackup?.();
    };
  });

  return {
    // Read state
    get games() { return games; },
    get loaded() { return loaded; },
    get error() { return error; },
    get runningId() { return runningId; },
    get runningPhase() { return runningPhase; },
    /** Whether a forced post-override backup is currently running for this game. */
    isBackingUp(id: string) { return backupsInProgress.has(id); },
    get lanPeers() { return lanPeers; },
    get openPeer() { return openPeer; },
    get peerGames() { return peerGames; },
    get peerGamesLoading() { return peerGamesLoading; },
    get peerGamesError() { return peerGamesError; },
    get activeDownload() { return activeDownload; },
    get startingGameId() { return startingGameId; },
    get activeUploads() { return activeUploads; },
    get syncStatus() { return syncStatus; },
    // Writable state (view binds directly)
    get selectedId() { return selectedId; },
    set selectedId(v: string | null) { selectedId = v; },
    get searchQuery() { return searchQuery; },
    set searchQuery(v: string) { searchQuery = v; },
    get filter() { return filter; },
    set filter(v: 'all' | 'recent' | 'played') { filter = v; },
    get conflictGameId() { return conflictGameId; },
    set conflictGameId(v: string | null) { conflictGameId = v; },
    get peerChoice() { return peerChoice; },
    set peerChoice(v: { game: DisplayGame; sources: PeerSource[] } | null) { peerChoice = v; },
    get suspendedConflict() { return suspendedConflict; },
    set suspendedConflict(v: { gameId: string; deviceName: string } | null) { suspendedConflict = v; },
    // Derived (read-only)
    get displayGames() { return displayGames; },
    get tabCounts() { return tabCounts; },
    get filteredGames() { return filteredGames; },
    get selectedGame() { return selectedGame; },
    get syncOk() { return syncOk; },
    get syncOff() { return syncOff; },
    get syncTitle() { return syncTitle; },
    get downloadActive() { return downloadActive; },
    get downloadCount() { return downloadCount; },
    get downloadPercent() { return downloadPct; },
    get liveUploads() { return liveUploadList; },
    get uploadCount() { return uploadCount; },
    get uploadPercent() { return uploadPercent; },
    // Methods
    refresh,
    refreshLanPeers,
    refreshPeerCatalogs,
    refreshActiveUploads,
    kickUpload,
    openPeerView,
    backToPeerList,
    clearPeerView,
    cancelActiveInstall,
    installFromPeer,
    downloadGame,
    chooseDownloadSource,
  };
}

export type Library = ReturnType<typeof createLibrary>;
