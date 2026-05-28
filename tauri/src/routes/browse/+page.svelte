<script lang="ts">
  /**
   * Browse Games — Hydra source aggregator.
   *
   * Three-pane window: feed sidebar (240px) | searchable game list
   * (flex) | detail pane (380px with hero, action toolbar, release
   * rows). Opens as a child WebviewWindow from the library's Browse
   * button.
   *
   * Data flow:
   *   - onMount → api.hydraFetchAll() → BrowseFetchResult
   *   - Group entries by (case-insensitive) title to dedupe across
   *     feeds; each group's `releases` array is one row per source
   *   - Cross-reference local library to flag "In library" entries
   *   - Filter by selected feed + search query + sort order
   *
   * Phase 4 will wire the Download CTA through TorBox (magnet → poll
   * → request_download_link → stream) and the local LAN cache. For
   * now Download is a stub that surfaces a toast.
   */
  import { onMount, onDestroy } from 'svelte';
  import {
    ChevronLeft,
    Cloud,
    Download,
    Loader2,
    RefreshCw,
    Search,
    Wifi,
    X,
  } from '@lucide/svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { api } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import type {
    BrowseDownloadProgress,
    BrowseFetchResult,
    GameEntry,
    HydraEntry,
    LanPeer,
  } from '$lib/types';
  import WindowChrome from '$lib/components/WindowChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import Pill from '$lib/components/Pill.svelte';

  // ── State ──────────────────────────────────────────────────────────────
  let loading = $state(true);
  let result = $state<BrowseFetchResult>({ entries: [], feeds: [] });
  let localGames = $state<GameEntry[]>([]);
  let lanPeers = $state<LanPeer[]>([]);

  let selectedFeed = $state<string>('all'); // 'all' | source_url
  let searchQuery = $state('');
  let sort = $state<'newest' | 'size' | 'az'>('newest');
  let pickedTitle = $state<string | null>(null);
  let activeDownload = $state<BrowseDownloadProgress | null>(null);
  let unlistenBrowseDownload: (() => void) | undefined;

  const downloadInFlight = $derived(
    activeDownload != null &&
      (activeDownload.status === 'starting' ||
        activeDownload.status === 'queuing' ||
        activeDownload.status === 'downloading'),
  );

  // ── Derived ────────────────────────────────────────────────────────────
  // Group entries by case-insensitive title — across feeds the same
  // game shows up under each source we merge them into one row whose
  // `releases` is the per-feed list.
  type Grouped = {
    title: string;
    releases: HydraEntry[];
    inLibrary: boolean;
    onLan: boolean;
    /** "Top" release = the most recent across sources. */
    top: HydraEntry;
  };

  const groups = $derived.by<Grouped[]>(() => {
    if (!result.entries.length) return [];
    const map = new Map<string, HydraEntry[]>();
    for (const entry of result.entries) {
      const key = entry.title.trim().toLowerCase();
      if (!key) continue;
      const arr = map.get(key) ?? [];
      arr.push(entry);
      map.set(key, arr);
    }
    const out: Grouped[] = [];
    const localNames = new Set(localGames.map((g) => g.game_name.toLowerCase()));
    const lanNames = new Set<string>(); // TODO: peer-game cross-ref (needs cached peer game lists)
    for (const [, releases] of map) {
      const top = pickTop(releases);
      out.push({
        title: top.title,
        releases,
        inLibrary: localNames.has(top.title.toLowerCase()),
        onLan: lanNames.has(top.title.toLowerCase()),
        top,
      });
    }
    return out;
  });

  const filtered = $derived.by<Grouped[]>(() => {
    let list = groups.slice();
    if (selectedFeed !== 'all') {
      list = list.filter((g) =>
        g.releases.some((r) => r.source_url === selectedFeed),
      );
    }
    const q = searchQuery.trim().toLowerCase();
    if (q) {
      list = list.filter((g) => g.title.toLowerCase().includes(q));
    }
    list.sort((a, b) => {
      switch (sort) {
        case 'size':
          return parseSize(b.top.file_size) - parseSize(a.top.file_size);
        case 'az':
          return a.title.localeCompare(b.title);
        default:
          return b.top.upload_date.localeCompare(a.top.upload_date);
      }
    });
    return list;
  });

  const picked = $derived(
    filtered.find((g) => g.title === pickedTitle) ?? filtered[0] ?? null,
  );

  // ── Helpers ────────────────────────────────────────────────────────────
  function pickTop(releases: HydraEntry[]): HydraEntry {
    // Most recent upload_date wins (string compare on ISO dates is
    // chronological; non-ISO strings fall through but most feeds use
    // ISO).
    let top = releases[0];
    for (const r of releases) {
      if (r.upload_date > top.upload_date) top = r;
    }
    return top;
  }

  function parseSize(s: string): number {
    // "38.4 GB" / "720 MB" / "8400" — best-effort.
    const m = s.match(/([\d.]+)\s*(KB|MB|GB|TB)?/i);
    if (!m) return 0;
    const n = parseFloat(m[1]);
    const u = (m[2] ?? '').toUpperCase();
    const mul =
      u === 'TB' ? 1024 ** 4 : u === 'GB' ? 1024 ** 3 : u === 'MB' ? 1024 ** 2 : u === 'KB' ? 1024 : 1;
    return n * mul;
  }

  function relDate(s: string): string {
    if (!s) return '—';
    const d = new Date(s);
    if (Number.isNaN(d.getTime())) return s.slice(0, 10);
    const diff = (Date.now() - d.getTime()) / 1000;
    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
    const days = Math.round(diff / 86400);
    if (days < 7) return `${days}d ago`;
    if (days < 30) return `${Math.round(days / 7)}w ago`;
    return `${Math.round(days / 30)}mo ago`;
  }

  // Per-feed colour token — derived from URL hash so it's stable.
  function feedColor(name: string): string {
    const hash = [...name].reduce((a, c) => a + c.charCodeAt(0), 0);
    const palette = [
      'var(--color-spool)',
      'var(--color-info)',
      'var(--color-ok)',
      'var(--color-warn)',
      '#c97aff',
      '#ff9ed6',
    ];
    return palette[hash % palette.length];
  }

  function uriKind(uri: string): 'magnet' | 'http' | 'unknown' {
    if (uri.startsWith('magnet:')) return 'magnet';
    if (uri.startsWith('http://') || uri.startsWith('https://')) return 'http';
    return 'unknown';
  }

  // ── Actions ────────────────────────────────────────────────────────────
  async function refresh() {
    loading = true;
    try {
      const [browseResult, games, peers] = await Promise.all([
        api.hydraFetchAll(),
        api.listGames(),
        api.listLanPeers(),
      ]);
      result = browseResult;
      localGames = games;
      lanPeers = peers;
      if (!pickedTitle && groups.length > 0) {
        pickedTitle = groups[0].title;
      }
    } catch (e) {
      console.error('[browse] refresh failed:', e);
      toasts.show({
        kind: 'bad',
        label: 'BROWSE',
        title: "Couldn't fetch sources",
        sub: String(e),
      });
    } finally {
      loading = false;
    }
  }

  async function startDownload(release: HydraEntry) {
    if (downloadInFlight) {
      toasts.show({
        kind: 'warn',
        label: 'BROWSE',
        title: 'A download is already in progress',
        sub: `Finish ${activeDownload?.game_name ?? 'the current one'} first.`,
      });
      return;
    }
    try {
      await api.startBrowseDownload(release);
      toasts.show({
        kind: 'info',
        label: 'BROWSE',
        title: `Queued ${release.title}`,
        sub: 'Progress shown at the top of the window.',
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'BROWSE',
        title: "Couldn't start download",
        sub: String(e),
      });
    }
  }

  async function cancelActiveDownload() {
    if (!activeDownload) return;
    try {
      await api.cancelBrowseDownload(activeDownload.install_token);
    } catch (e) {
      console.error('[browse] cancel failed:', e);
    }
  }

  function fmtBytes(b: number): string {
    if (b <= 0) return '—';
    if (b < 1024) return `${b} B`;
    if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
    if (b < 1024 * 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB`;
    return `${(b / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  async function close() {
    await getCurrentWindow().close();
  }

  onMount(async () => {
    await refresh();
    // Pick up an in-flight download if the window was just opened
    // while one is running.
    try {
      activeDownload = await api.currentBrowseDownload();
    } catch (e) {
      console.error('[browse] currentBrowseDownload failed:', e);
    }
    listen<BrowseDownloadProgress>('browse:download', (event) => {
      const p = event.payload;
      const prev = activeDownload;
      activeDownload = p;
      // Surface terminal states as toasts so the user notices even if
      // they've scrolled away.
      if (p.status === 'done' && prev?.install_token !== p.install_token) {
        toasts.show({
          kind: 'ok',
          label: 'DOWNLOAD',
          title: 'Download complete',
          sub: p.dest_path ?? p.game_name,
        });
      } else if (p.status === 'error') {
        toasts.show({
          kind: 'bad',
          label: 'DOWNLOAD',
          title: 'Download failed',
          sub: p.message ?? p.game_name,
        });
      } else if (p.status === 'canceled') {
        toasts.show({
          kind: 'info',
          label: 'DOWNLOAD',
          title: 'Cancelled',
          sub: `${p.game_name} — partial files may remain in your downloads folder.`,
        });
      }
    })
      .then((fn) => (unlistenBrowseDownload = fn))
      .catch((e) => console.error('[browse] listener failed:', e));
  });

  onDestroy(() => {
    unlistenBrowseDownload?.();
  });
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <WindowChrome sub="BROWSE · SOURCES">
    <div class="flex h-full items-center gap-2 px-2">
      <span
        class="font-mono text-[10.5px] tracking-[0.06em] text-ink-2"
      >
        {result.entries.length.toLocaleString()} ENTRIES
      </span>
      <div class="flex-1"></div>
      <button
        type="button"
        onclick={refresh}
        title="Refresh feeds"
        aria-label="Refresh feeds"
        class="inline-flex h-7 w-7 items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
        data-tauri-drag-region="false"
      >
        {#if loading}
          <Loader2 size={14} class="animate-[spool-spin_1s_linear_infinite]" />
        {:else}
          <RefreshCw size={14} />
        {/if}
      </button>
    </div>
  </WindowChrome>

  {#if activeDownload && downloadInFlight}
    {@const pct =
      activeDownload.bytes_total > 0
        ? Math.round((activeDownload.bytes_done / activeDownload.bytes_total) * 100)
        : 0}
    <div
      class="flex items-center gap-3 border-b border-line-1 px-5 py-2.5"
      style:background="color-mix(in srgb, var(--color-spool) 6%, transparent)"
    >
      {#if activeDownload.source_kind === 'torbox'}
        <Cloud size={14} class="text-info" />
      {:else}
        <Download size={14} class="text-spool" />
      {/if}
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="truncate text-[12.5px] font-medium text-ink-0">
            {activeDownload.game_name}
          </span>
          <span
            class="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-3"
          >
            via {activeDownload.source_name}
          </span>
        </div>
        <div class="mt-1 h-1 w-full overflow-hidden rounded-full bg-bg-0">
          <div
            class="h-full transition-[width] duration-150 ease-out"
            style:width="{activeDownload.bytes_total > 0 ? pct : 0}%"
            style:background="var(--color-spool)"
          ></div>
        </div>
        <div
          class="font-mono mt-1 flex justify-between text-[10px] tracking-[0.04em] text-ink-3"
        >
          <span class="min-w-0 truncate" title={activeDownload.current_file}>
            {activeDownload.current_file || activeDownload.status}
          </span>
          <span>
            {activeDownload.bytes_total > 0
              ? `${fmtBytes(activeDownload.bytes_done)} / ${fmtBytes(activeDownload.bytes_total)} · ${pct}%`
              : activeDownload.status}
          </span>
        </div>
      </div>
      <button
        type="button"
        onclick={cancelActiveDownload}
        aria-label="Cancel download"
        title="Cancel"
        class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-2 transition-colors hover:border-bad/60 hover:text-bad"
      >
        <X size={13} />
      </button>
    </div>
  {/if}

  <div class="grid min-h-0 flex-1" style:grid-template-columns="240px 1fr 380px">
    <!-- ── Feed sidebar ──────────────────────────────────────────────── -->
    <aside class="flex min-h-0 flex-col border-r border-line-1 bg-bg-1">
      <div class="px-4 py-3">
        <MonoLabel size={9.5}>FEEDS</MonoLabel>
      </div>
      <div class="flex-1 overflow-y-auto px-2">
        <button
          type="button"
          onclick={() => (selectedFeed = 'all')}
          class="flex w-full items-center justify-between gap-2 rounded-sm px-2.5 py-2 text-left text-[12px] transition-colors"
          class:bg-bg-3={selectedFeed === 'all'}
          style:color={selectedFeed === 'all' ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
        >
          <span>All feeds</span>
          <span
            class="font-mono text-[9.5px] tracking-[0.06em] text-ink-3"
          >
            {result.entries.length.toLocaleString()}
          </span>
        </button>
        {#each result.feeds as feed (feed.url)}
          {@const active = selectedFeed === feed.url}
          {@const color = feedColor(feed.name ?? feed.url)}
          <button
            type="button"
            onclick={() => (selectedFeed = feed.url)}
            class="flex w-full items-center gap-2.5 rounded-sm px-2.5 py-2 text-left text-[12px] transition-colors"
            style:background={active ? 'var(--color-bg-3)' : 'transparent'}
            style:color={active ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
          >
            <span
              class="h-1.5 w-1.5 shrink-0 rounded-full"
              style:background={color}
            ></span>
            <div class="min-w-0 flex-1">
              <div class="truncate" title={feed.name ?? feed.url}>
                {feed.name ?? new URL(feed.url).host}
              </div>
              {#if feed.error}
                <div class="font-mono mt-0.5 truncate text-[9px] text-bad" title={feed.error}>
                  failed
                </div>
              {/if}
            </div>
            <span class="font-mono text-[9.5px] tracking-[0.06em] text-ink-3">
              {feed.entry_count.toLocaleString()}
            </span>
          </button>
        {/each}
      </div>
      <div class="border-t border-line-1 px-3 py-2.5">
        <a
          href="/settings"
          class="font-mono inline-flex items-center gap-1.5 text-[10.5px] uppercase tracking-[0.08em] text-ink-2 transition-colors hover:text-ink-0"
        >
          <ChevronLeft size={12} />
          Manage feeds
        </a>
      </div>
    </aside>

    <!-- ── Game list ──────────────────────────────────────────────────── -->
    <section class="flex min-h-0 flex-col">
      <!-- Search + sort -->
      <div
        class="flex items-center gap-3 border-b border-line-1 bg-bg-1 px-4 py-2.5"
      >
        <div
          class="flex h-[28px] flex-1 items-center gap-2 rounded-sm border border-line-1 bg-bg-2 px-2.5"
        >
          <Search size={14} class="text-ink-2" />
          <input
            bind:value={searchQuery}
            placeholder="Filter {filtered.length} titles"
            class="font-sans min-w-0 flex-1 bg-transparent text-[12.5px] text-ink-0 outline-none placeholder:text-ink-3"
          />
          {#if searchQuery}
            <button
              type="button"
              onclick={() => (searchQuery = '')}
              class="text-ink-3 transition-colors hover:text-ink-0"
              aria-label="Clear search"
            >
              <X size={12} />
            </button>
          {/if}
        </div>
        <div
          class="inline-flex rounded-sm border border-line-1 bg-bg-2 p-0.5"
        >
          {#each [{ id: 'newest' as const, label: 'Newest' }, { id: 'size' as const, label: 'Size' }, { id: 'az' as const, label: 'A–Z' }] as opt (opt.id)}
            <button
              type="button"
              onclick={() => (sort = opt.id)}
              class="rounded-[2px] px-2.5 py-1 text-[11px] font-medium transition-colors"
              style:background={sort === opt.id ? 'var(--color-bg-3)' : 'transparent'}
              style:color={sort === opt.id ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
            >
              {opt.label}
            </button>
          {/each}
        </div>
      </div>

      <!-- Column headers -->
      <div
        class="grid items-center gap-2.5 border-b border-line-1 bg-bg-1 px-4 py-2"
        style:grid-template-columns="1fr 96px 88px 80px"
      >
        <MonoLabel size={9}>Title</MonoLabel>
        <MonoLabel size={9}>Releases</MonoLabel>
        <MonoLabel size={9}>Top size</MonoLabel>
        <MonoLabel size={9}>Posted</MonoLabel>
      </div>

      <!-- List -->
      <div class="flex-1 overflow-y-auto">
        {#if loading && result.entries.length === 0}
          <div class="flex h-full items-center justify-center text-[12px] text-ink-3">
            <Loader2 size={16} class="mr-2 animate-[spool-spin_1s_linear_infinite]" />
            Fetching feeds…
          </div>
        {:else if filtered.length === 0}
          <div class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <p class="text-[13px] text-ink-1">
              {result.entries.length === 0
                ? 'No feeds configured'
                : `No matches for "${searchQuery}"`}
            </p>
            {#if result.entries.length === 0}
              <p class="text-[11.5px] text-ink-3">
                Add Hydra source feeds in Settings → Sources & Downloads to populate this list.
              </p>
            {/if}
          </div>
        {:else}
          {#each filtered as group (group.title)}
            {@const isPicked = picked?.title === group.title}
            <button
              type="button"
              onclick={() => (pickedTitle = group.title)}
              class="grid w-full cursor-pointer items-center gap-2.5 border-b border-dashed border-line-1 px-4 py-2.5 text-left transition-colors"
              style:grid-template-columns="1fr 96px 88px 80px"
              style:background={isPicked
                ? 'color-mix(in srgb, var(--color-spool) 8%, transparent)'
                : 'transparent'}
              style:border-left="2px solid {isPicked ? 'var(--color-spool)' : 'transparent'}"
            >
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span
                    class="truncate text-[12.5px] font-medium text-ink-0"
                    title={group.title}
                  >
                    {group.title}
                  </span>
                  {#if group.inLibrary}
                    <Pill kind="ok">In library</Pill>
                  {/if}
                </div>
                <div class="mt-0.5 flex gap-2.5">
                  {#each group.releases.slice(0, 4) as r (r.source_url + r.upload_date)}
                    <span
                      class="font-mono inline-flex items-center gap-1 text-[9.5px] tracking-[0.04em] text-ink-2"
                    >
                      <span
                        class="h-[5px] w-[5px] rounded-full"
                        style:background={feedColor(r.source_name)}
                      ></span>
                      {r.source_name || new URL(r.source_url).host}
                    </span>
                  {/each}
                </div>
              </div>
              <span class="font-mono text-[10.5px] text-ink-2">
                {group.releases.length} release{group.releases.length === 1 ? '' : 's'}
              </span>
              <span class="font-mono text-[10.5px] text-ink-2">
                {group.top.file_size || '—'}
              </span>
              <span class="font-mono text-[10.5px] text-ink-3">
                {relDate(group.top.upload_date)}
              </span>
            </button>
          {/each}
        {/if}
      </div>
    </section>

    <!-- ── Detail pane ────────────────────────────────────────────────── -->
    <aside class="flex min-h-0 flex-col border-l border-line-1 bg-bg-1">
      {#if picked}
        <!-- Hero -->
        <div
          class="relative flex h-[180px] shrink-0 items-end overflow-hidden"
          style:background="linear-gradient(135deg, var(--color-bg-2) 0%, var(--color-bg-0) 100%)"
        >
          <span
            class="absolute left-0 right-0 top-0 h-[3px]"
            style:background="var(--color-spool)"
          ></span>
          <div class="px-5 pb-4">
            <MonoLabel size={9.5}>
              <span style:color="var(--color-spool)">
                {picked.releases.length} RELEASE{picked.releases.length === 1 ? '' : 'S'}
              </span>
            </MonoLabel>
            <div
              class="font-display mt-1.5 text-[20px] font-bold leading-[1.1]"
              style:letter-spacing="-0.014em"
              style:text-wrap="balance"
            >
              {picked.title}
            </div>
          </div>
        </div>

        <!-- Action bar -->
        <div class="flex items-center gap-2 border-b border-line-1 px-5 py-3">
          <div class="flex-1">
            <Btn
              onclick={() => picked && startDownload(picked.top)}
              disabled={downloadInFlight}
            >
              {#snippet icon()}<Download size={14} />{/snippet}
              Download · best match
            </Btn>
          </div>
        </div>

        <!-- Releases -->
        <div class="flex-1 overflow-y-auto px-5 py-3">
          <MonoLabel size={9.5}>RELEASES · {picked.releases.length}</MonoLabel>
          <div class="mt-2 flex flex-col gap-1.5">
            {#each picked.releases.slice().sort((a, b) => b.upload_date.localeCompare(a.upload_date)) as r, i (r.source_url + r.upload_date + i)}
              {@const isBest = r === picked.top}
              <div
                class="rounded-sm border px-3 py-2.5"
                style:background={isBest
                  ? 'color-mix(in srgb, var(--color-spool) 8%, transparent)'
                  : 'var(--color-bg-2)'}
                style:border-color={isBest
                  ? 'color-mix(in srgb, var(--color-spool) 30%, transparent)'
                  : 'var(--color-line-1)'}
              >
                <div class="flex items-center gap-2">
                  <span class="text-[12.5px] font-medium text-ink-0">
                    {r.source_name || new URL(r.source_url).host}
                  </span>
                  {#if isBest}
                    <MonoLabel size={9}>
                      <span style:color="var(--color-spool)">BEST</span>
                    </MonoLabel>
                  {/if}
                  <div class="flex-1"></div>
                  <button
                    type="button"
                    onclick={() => startDownload(r)}
                    class="font-mono inline-flex items-center gap-1 rounded-sm border border-line-1 bg-transparent px-2 py-1 text-[10px] uppercase tracking-[0.06em] text-ink-1 transition-colors hover:bg-white/5 hover:text-ink-0"
                  >
                    <Download size={10} />
                    Get
                  </button>
                </div>
                <div
                  class="font-mono mt-1.5 grid gap-2 text-[10px] text-ink-2 tracking-[0.04em]"
                  style:grid-template-columns="auto 1fr auto"
                >
                  <span>{r.file_size || '—'}</span>
                  <span class="truncate text-ink-3" title={r.uris[0] ?? ''}>
                    {r.uris.length} URI{r.uris.length === 1 ? '' : 's'}
                  </span>
                  <span class="text-ink-3">{relDate(r.upload_date)}</span>
                </div>
              </div>
            {/each}
          </div>

          <!-- LAN callout (placeholder — Phase 4 cross-refs peer game lists) -->
          {#if lanPeers.length > 0}
            <div class="mt-4 border-t border-dashed border-line-1 pt-3">
              <MonoLabel size={9.5}>OR PULL FROM YOUR LAN</MonoLabel>
              <div
                class="mt-2 flex items-center gap-2.5 rounded-sm border px-3 py-2.5"
                style:background="color-mix(in srgb, var(--color-ok) 6%, transparent)"
                style:border-color="color-mix(in srgb, var(--color-ok) 25%, transparent)"
              >
                <Wifi size={14} class="text-ok" />
                <div class="flex-1 text-[11.5px] text-ink-1">
                  {lanPeers.length} peer{lanPeers.length === 1 ? '' : 's'} online — open the LAN
                  popover to browse their libraries.
                </div>
              </div>
            </div>
          {/if}
        </div>
      {:else}
        <div class="flex h-full items-center justify-center px-6 text-center text-[12px] text-ink-3">
          {loading ? 'Loading…' : 'Select a game on the left to see release details.'}
        </div>
      {/if}
    </aside>
  </div>
</div>
