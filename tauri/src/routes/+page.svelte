<script lang="ts">
  /**
   * Library — two-pane layout (sidebar list + selected-game detail).
   *
   * Sidebar:
   *   - search input
   *   - filter tabs: All / Recent / Played
   *   - scrollable list of small entries (cover + name + catalog/last-played)
   *   - footer with the primary "Add a game" button
   *
   * Detail pane:
   *   - hero, stats, action toolbar, three info cards
   *   - rendered by `GameDetail.svelte`
   *
   * Listens to `library:changed` so adds/removes from the Add Game popup
   * (or anywhere else) refresh the list automatically. If the selected
   * entry vanishes, falls back to the first remaining game.
   */
  import { onMount } from 'svelte';
  import { Plus, Search, Settings } from '@lucide/svelte';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { listen } from '@tauri-apps/api/event';
  import { api, assetUrl } from '$lib/api';
  import { fmtCatalog, relDate } from '$lib/format';
  import type { GameEntry } from '$lib/types';
  import WindowChrome from '$lib/components/WindowChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import GameDetail from '$lib/components/GameDetail.svelte';

  let games = $state<GameEntry[]>([]);
  let loaded = $state(false);
  let error = $state<string | null>(null);

  let selectedId = $state<string | null>(null);
  let searchQuery = $state('');
  let filter = $state<'all' | 'recent' | 'played'>('all');

  // ── Derived ────────────────────────────────────────────────────────────
  const filteredGames = $derived.by(() => {
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
  });

  const selectedGame = $derived(
    selectedId ? games.find((g) => g.id === selectedId) ?? null : null,
  );

  // ── Data fetch + event subscription ─────────────────────────────────────
  async function refresh() {
    try {
      games = await api.listGames();
      // Keep selection if still present, else pick the first game.
      if (selectedId && !games.some((g) => g.id === selectedId)) {
        selectedId = games[0]?.id ?? null;
      } else if (!selectedId && games.length > 0) {
        selectedId = games[0].id;
      } else if (games.length === 0) {
        selectedId = null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loaded = true;
    }
  }

  let unlistenLibraryChanged: (() => void) | undefined;

  onMount(() => {
    refresh();
    listen<string>('library:changed', () => refresh())
      .then((fn) => (unlistenLibraryChanged = fn))
      .catch((e) => console.error('[library] listener failed:', e));
    return () => unlistenLibraryChanged?.();
  });

  // ── Add Game popup ─────────────────────────────────────────────────────
  function openAddGame() {
    WebviewWindow.getByLabel('add-game').then((win) => {
      if (win) {
        win.setFocus();
        return;
      }
      new WebviewWindow('add-game', {
        url: '/add',
        title: 'Add Game · Spool',
        width: 720,
        height: 560,
        minWidth: 600,
        minHeight: 480,
        decorations: false,
        resizable: true,
        center: true,
      });
    });
  }

  const filters: { id: typeof filter; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'recent', label: 'Recent' },
    { id: 'played', label: 'Played' },
  ];
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <WindowChrome sub="LIBRARY">
    {#snippet children()}
      <div class="flex h-full items-center justify-end gap-1 pr-2">
        <a
          href="/settings"
          aria-label="Settings"
          class="inline-flex h-7 w-7 items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
          data-tauri-drag-region="false"
        >
          <Settings size={14} />
        </a>
      </div>
    {/snippet}
  </WindowChrome>

  <div class="grid min-h-0 flex-1" style:grid-template-columns="320px 1fr">
    <!-- ── Sidebar ────────────────────────────────────────────────── -->
    <aside class="flex min-h-0 flex-col border-r border-line-1 bg-bg-1">
      <!-- Search + filters -->
      <div class="flex flex-col gap-2.5 px-3 py-3">
        <div
          class="flex h-[30px] items-center gap-2 rounded-sm border border-line-1 bg-bg-2 px-2.5"
        >
          <Search size={14} class="text-ink-2" />
          <input
            bind:value={searchQuery}
            placeholder={`Search ${games.length || 0} games`}
            class="font-sans min-w-0 flex-1 bg-transparent text-[12.5px] text-ink-0 outline-none placeholder:text-ink-3"
          />
        </div>
        <div class="flex gap-1">
          {#each filters as f (f.id)}
            {@const active = filter === f.id}
            <button
              type="button"
              onclick={() => (filter = f.id)}
              class="inline-flex items-center gap-1.5 rounded-sm border px-2 py-1 text-[11.5px] font-medium transition-colors"
              style:background={active ? 'var(--color-bg-3)' : 'transparent'}
              style:border-color={active ? 'var(--color-line-2)' : 'transparent'}
              style:color={active ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
            >
              {f.label}
              <span
                class="font-mono text-[9.5px]"
                style:color={active ? 'var(--color-ink-2)' : 'var(--color-ink-3)'}
              >
                {f.id === 'all'
                  ? games.length
                  : f.id === 'recent'
                    ? games.filter((g) => g.last_played_at || g.added_at).length
                    : games.filter((g) => g.playtime_minutes > 0).length}
              </span>
            </button>
          {/each}
        </div>
      </div>

      <!-- Section header -->
      <div class="flex items-center justify-between px-3.5 pb-1.5 pt-2.5">
        <MonoLabel size={9.5}>
          {filter === 'recent' ? 'By last activity' : 'By catalog'}
        </MonoLabel>
        <span class="text-[11px] text-ink-3">{filteredGames.length}</span>
      </div>

      <!-- List -->
      <div class="min-h-0 flex-1 overflow-y-auto pb-2">
        {#if !loaded}
          <p
            class="font-mono px-4 py-3 text-[10px] uppercase tracking-[0.12em] text-ink-3"
          >
            Loading…
          </p>
        {:else if error}
          <p class="px-4 py-3 text-[12px] text-bad">{error}</p>
        {:else if filteredGames.length === 0 && games.length === 0}
          <div class="flex flex-col items-center gap-2 px-4 py-10 text-center">
            <MonoLabel>Empty shelf</MonoLabel>
            <p class="text-[12px] text-ink-2">No games yet.</p>
          </div>
        {:else if filteredGames.length === 0}
          <p class="px-4 py-3 text-[12px] text-ink-3">No matches.</p>
        {:else}
          {#each filteredGames as g (g.id)}
            {@const selected = selectedId === g.id}
            {@const cover = assetUrl(g.cover_image_path)}
            <button
              type="button"
              onclick={() => (selectedId = g.id)}
              class="flex w-full items-center gap-2.5 border-l-2 px-3 py-2 text-left transition-colors"
              style:background={selected
                ? 'rgb(215 201 160 / 0.10)'
                : 'transparent'}
              style:border-left-color={selected ? 'var(--color-spool)' : 'transparent'}
            >
              <div
                class="h-11 w-8 shrink-0 overflow-hidden rounded-sm border border-line-1 bg-bg-2"
              >
                {#if cover}
                  <img
                    src={cover}
                    alt={g.game_name}
                    class="h-full w-full object-cover"
                  />
                {:else}
                  <div
                    class="flex h-full w-full items-center justify-center"
                    style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
                  >
                    <span
                      class="font-mono text-[7px] uppercase tracking-[0.1em] text-ink-3"
                    >
                      {g.game_name.slice(0, 1)}
                    </span>
                  </div>
                {/if}
              </div>
              <div class="min-w-0 flex-1">
                <div
                  class="truncate text-[12.5px] font-medium text-ink-0"
                  title={g.game_name}
                >
                  {g.game_name}
                </div>
                <div
                  class="font-mono mt-0.5 flex items-center gap-1.5 text-[9.5px] tracking-[0.06em] text-ink-3"
                >
                  <span>{fmtCatalog(g.catalog_number)}</span>
                  <span>·</span>
                  <span>{g.last_played_at ? relDate(g.last_played_at) : 'unplayed'}</span>
                </div>
              </div>
            </button>
          {/each}
        {/if}
      </div>

      <!-- Footer -->
      <div class="border-t border-line-1 bg-bg-0 px-3 py-2.5">
        <button
          type="button"
          onclick={openAddGame}
          class="inline-flex h-8 w-full cursor-pointer items-center justify-center gap-1.5 rounded-sm bg-spool px-3 text-[12.5px] font-medium text-bg-0 transition-colors hover:brightness-95"
        >
          <Plus size={14} />
          Add a game
        </button>
      </div>
    </aside>

    <!-- ── Detail pane ──────────────────────────────────────────────── -->
    {#if selectedGame}
      <GameDetail game={selectedGame} />
    {:else if loaded && games.length === 0}
      <div class="flex flex-col items-center justify-center gap-3 text-center">
        <MonoLabel>Empty library</MonoLabel>
        <p class="max-w-md text-sm text-ink-2">
          No games yet. Add an executable to start your collection.
        </p>
        <button
          type="button"
          onclick={openAddGame}
          class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-sm bg-spool px-3 text-[12.5px] font-medium text-bg-0 transition-colors hover:brightness-95"
        >
          <Plus size={14} />
          Add your first game
        </button>
      </div>
    {:else}
      <div class="flex items-center justify-center">
        <MonoLabel>Pick a game from the sidebar</MonoLabel>
      </div>
    {/if}
  </div>
</div>
