<script lang="ts">
  import { BookOpen, Info, Play, Search, Settings } from '@lucide/svelte';
  import { api, assetUrl } from '$lib/api';
  import { fmtCatalog, fmtPlaytime, relDate } from '$lib/format';
  import { openView } from '$lib/nav';
  import { toasts } from '$lib/toasts.svelte';
  import type { GameEntry } from '$lib/types';
  import type { Library } from '$lib/library.svelte';
  import AppChrome from '$lib/components/AppChrome.svelte';
  import GameDetail from '$lib/components/GameDetail.svelte';
  import LibraryContextMenu from '$lib/components/LibraryContextMenu.svelte';
  import LibrarySearch from '$lib/components/LibrarySearch.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';

  let { lib }: { lib: Library } = $props();

  // Local overlay state
  let detailOpen = $state(false);
  let searchOpen = $state(false);
  let ctxMenu = $state<{ game: GameEntry; x: number; y: number } | null>(null);

  // Shelf category
  type ShelfCat = 'continue' | 'all' | 'lan';
  let shelfCat = $state<ShelfCat>('continue');

  // Long-press detection
  let pressTimer: ReturnType<typeof setTimeout> | undefined;

  function startLongPress(game: GameEntry) {
    pressTimer = setTimeout(() => {
      // Position near center-bottom of screen
      ctxMenu = { game, x: window.innerWidth / 2 - 120, y: window.innerHeight - 290 };
    }, 500);
  }

  function cancelLongPress() {
    clearTimeout(pressTimer);
  }

  // Category derivations
  const continueCat = $derived(
    lib.games
      .filter((g) => g.last_played_at != null)
      .slice()
      .sort((a, b) => (b.last_played_at ?? '').localeCompare(a.last_played_at ?? '')),
  );
  const allCat = $derived(
    [...lib.games].sort((a, b) => a.catalog_number - b.catalog_number),
  );
  const lanCat = $derived(lib.games.filter((g) => g.lan_shared));

  const shelfGames = $derived(
    shelfCat === 'continue' ? continueCat :
    shelfCat === 'lan' ? lanCat :
    allCat,
  );

  const selectedGame = $derived(lib.selectedGame);
  const accent = $derived(selectedGame?.accent_color ?? null);

  // Tapping a tile: first tap selects (features in banner), second opens detail
  function onTileTap(game: GameEntry) {
    cancelLongPress();
    if (lib.selectedId === game.id) {
      detailOpen = true;
    } else {
      lib.selectedId = game.id;
    }
  }

  // Launch from banner Play button
  async function launchSelected() {
    if (!lib.selectedId) return;
    try {
      await api.launchGame(lib.selectedId);
      detailOpen = true; // navigate to detail so run-phase events are visible
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'LAUNCH', title: "Couldn't launch game", sub: String(e) });
    }
  }

  const cats = $derived<{ id: ShelfCat; label: string }[]>([
    { id: 'continue', label: 'Continue' },
    { id: 'all', label: 'All games' },
    { id: 'lan', label: `LAN · ${lanCat.length}` },
  ]);
</script>

{#if detailOpen && selectedGame}
  <!-- Full-screen detail overlay -->
  <div class="fixed inset-0 z-50 flex flex-col overflow-hidden bg-bg-0">
    <AppChrome
      sub={selectedGame.game_name.toUpperCase().slice(0, 18)}
      accent={accent ?? undefined}
      onback={() => (detailOpen = false)}
    />
    <div class="min-h-0 flex-1 overflow-y-auto">
      <GameDetail
        game={selectedGame}
        runPhase={lib.runningId === selectedGame.id ? lib.runningPhase : null}
      />
    </div>
  </div>
{:else if searchOpen}
  <LibrarySearch {lib} onclose={() => (searchOpen = false)} />
{:else}
  <!-- Shelf view -->
  <div
    class="flex h-screen flex-col overflow-hidden bg-bg-0 text-ink-0"
    style:background={accent
      ? `radial-gradient(ellipse 90% 55% at 75% 25%, ${accent}28, transparent 62%), var(--color-bg-0)`
      : undefined}
  >
    <AppChrome
      sub="LIBRARY"
      peers={lib.lanPeers.length}
      transfers={lib.downloadCount}
      conflict={lib.syncOff}
    >
      <!-- Chrome right slot -->
      <div class="flex h-full items-center justify-end gap-2 pr-2">
        <button
          type="button"
          onclick={() => (searchOpen = true)}
          class="inline-flex cursor-pointer items-center gap-2 rounded-full border border-line-2 bg-bg-2 text-ink-2 transition-colors hover:text-ink-0"
          style:height="calc(var(--control-h) * 0.75)"
          style:padding-inline="calc(var(--space-unit) * 3)"
        >
          <Search size={13} />
          <span style:font-size="var(--text-sm)">{lib.games.length} games</span>
        </button>
        {#if lib.config && lib.config.download_sources.length > 0}
          <button
            type="button"
            onclick={() => openView('browse')}
            class="inline-flex cursor-pointer items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
            style:height="var(--control-h-icon)"
            style:width="var(--control-h-icon)"
            aria-label="Browse games"
          >
            <BookOpen size={14} />
          </button>
        {/if}
        <button
          type="button"
          onclick={() => openView('settings')}
          class="inline-flex cursor-pointer items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
          style:height="var(--control-h-icon)"
          style:width="var(--control-h-icon)"
          aria-label="Settings"
        >
          <Settings size={14} />
        </button>
      </div>
    </AppChrome>

    <!-- Featured banner or loading state -->
    {#if !lib.loaded}
      <div class="flex flex-1 items-center justify-center">
        <p class="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-3">Loading…</p>
      </div>
    {:else if lib.games.length === 0}
      <div class="flex flex-1 flex-col items-center justify-center gap-4 px-6">
        <MonoLabel>Empty shelf</MonoLabel>
        <p class="text-ink-2" style:font-size="var(--text-base)">No games yet.</p>
        <button
          type="button"
          onclick={() => openView('add')}
          class="inline-flex cursor-pointer items-center gap-2 rounded-sm bg-spool font-medium text-bg-0"
          style:height="var(--control-h)"
          style:padding-inline="calc(var(--space-unit) * 4)"
          style:font-size="var(--text-base)"
        >
          Add your first game
        </button>
      </div>
    {:else if selectedGame}
      <div class="flex min-h-0 flex-1 items-center gap-6 overflow-hidden px-6 py-5">
        <!-- Left: metadata + actions -->
        <div class="flex min-w-0 max-w-[54%] flex-1 flex-col gap-3">
          <div class="flex flex-wrap items-center gap-2">
            <span class="font-mono text-[10px] tracking-[0.12em] text-ink-3">
              {fmtCatalog(selectedGame.catalog_number)}
            </span>
            {#if selectedGame.last_played_at}
              <span class="inline-flex items-center gap-1 rounded-full border border-ok/40 bg-ok/10 px-2 py-0.5 font-mono text-[9.5px] text-ok">
                Last played
              </span>
            {/if}
          </div>

          <h1
            class="m-0 font-display font-bold leading-[0.97] tracking-[-0.028em]"
            style:font-size="var(--text-lg)"
            style:text-wrap="balance"
            style:text-shadow="0 2px 22px rgba(0,0,0,0.55)"
          >
            {selectedGame.game_name}
          </h1>

          <div
            class="flex flex-wrap items-center gap-2 font-mono tracking-[0.05em] text-ink-2"
            style:font-size="var(--text-sm)"
          >
            {#if selectedGame.developer}
              <span>{selectedGame.developer}</span>
              <span class="text-ink-3">·</span>
            {/if}
            <span>{fmtPlaytime(selectedGame.playtime_minutes)}</span>
            <span class="text-ink-3">·</span>
            <span>LAST · {selectedGame.last_played_at ? relDate(selectedGame.last_played_at).toUpperCase() : 'NEVER'}</span>
          </div>

          {#if selectedGame.description}
            <p class="m-0 line-clamp-2 leading-[1.5] text-ink-1" style:font-size="var(--text-base)">
              {selectedGame.description}
            </p>
          {/if}

          <!-- Action strip -->
          <div class="mt-1 flex flex-wrap items-center gap-2">
            <button
              type="button"
              onclick={launchSelected}
              disabled={!selectedGame.exe_path}
              class="inline-flex cursor-pointer items-center gap-2 rounded-sm font-semibold text-bg-0 transition-opacity disabled:cursor-not-allowed disabled:opacity-50"
              style:background={accent ?? 'var(--color-spool)'}
              style:height="var(--control-h)"
              style:padding-inline="calc(var(--space-unit) * 5)"
              style:font-size="var(--text-base)"
            >
              <Play size={15} fill="currentColor" />
              {selectedGame.last_played_at ? 'Resume' : 'Play'}
            </button>
            <button
              type="button"
              onclick={() => (detailOpen = true)}
              class="inline-flex cursor-pointer items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-1 transition-colors hover:text-ink-0"
              style:height="var(--control-h-icon)"
              style:width="var(--control-h-icon)"
              title="Details"
              aria-label="Game details"
            >
              <Info size={14} />
            </button>
          </div>
        </div>

        <!-- Right: cover art -->
        <div class="flex shrink-0 items-center justify-center">
          {#if assetUrl(selectedGame.cover_image_path)}
            <img
              src={assetUrl(selectedGame.cover_image_path)}
              alt={selectedGame.game_name}
              class="rounded-md object-cover shadow-2xl"
              style:width="calc(var(--control-h) * 4.8)"
              style:height="calc(var(--control-h) * 6.7)"
            />
          {:else}
            <div
              class="flex items-center justify-center rounded-md"
              style:width="calc(var(--control-h) * 4.8)"
              style:height="calc(var(--control-h) * 6.7)"
              style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
            >
              <span class="font-mono text-[11px] uppercase tracking-[0.1em] text-ink-3">
                {selectedGame.game_name.slice(0, 1)}
              </span>
            </div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Shelf row -->
    <div class="shrink-0 border-t border-line-1 bg-black/[0.28]" style:backdrop-filter="blur(12px)">
      <!-- Category tabs -->
      <div class="flex flex-wrap items-center gap-2 px-4 pb-2 pt-3">
        {#each cats as cat (cat.id)}
          <button
            type="button"
            onclick={() => (shelfCat = cat.id)}
            class="cursor-pointer whitespace-nowrap rounded-full border transition-colors"
            style:height="var(--control-h)"
            style:padding-inline="calc(var(--space-unit) * 4)"
            style:font-size="var(--text-base)"
            style:font-weight={shelfCat === cat.id ? 600 : 500}
            style:background={shelfCat === cat.id ? 'rgba(255,255,255,0.08)' : 'transparent'}
            style:border-color={shelfCat === cat.id ? 'var(--color-line-2)' : 'transparent'}
            style:color={shelfCat === cat.id ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
          >
            {cat.label}
          </button>
        {/each}
      </div>

      <!-- Horizontal cover rail -->
      <div
        class="flex gap-3 overflow-x-auto px-4 pb-5"
        style:scroll-snap-type="x mandatory"
        style:-webkit-overflow-scrolling="touch"
      >
        {#if shelfGames.length === 0}
          <p class="py-4 text-ink-3" style:font-size="var(--text-sm)">
            {shelfCat === 'continue' ? 'No recently played games.' : shelfCat === 'lan' ? 'No LAN-shared games.' : 'No games.'}
          </p>
        {:else}
          {#each shelfGames as game (game.id)}
            {@const active = lib.selectedId === game.id}
            {@const cover = assetUrl(game.cover_image_path)}
            {@const tileW = 'calc(var(--control-h) * 2.8)'}
            {@const tileH = 'calc(var(--control-h) * 3.9)'}
            <button
              type="button"
              onclick={() => onTileTap(game)}
              onpointerdown={() => startLongPress(game)}
              onpointerup={cancelLongPress}
              onpointercancel={cancelLongPress}
              onpointermove={cancelLongPress}
              class="shrink-0 flex cursor-pointer flex-col gap-2 border-none bg-transparent text-left transition-transform duration-150"
              style:scroll-snap-align="start"
              style:width={tileW}
              style:transform={active ? 'translateY(-6px)' : 'none'}
            >
              <div
                class="overflow-hidden rounded-sm"
                style:width={tileW}
                style:height={tileH}
                style:outline={active
                  ? `2.5px solid ${game.accent_color ?? 'var(--color-spool)'}`
                  : '2.5px solid transparent'}
                style:outline-offset="3px"
              >
                {#if cover}
                  <img src={cover} alt={game.game_name} class="h-full w-full object-cover" />
                {:else}
                  <div
                    class="flex h-full w-full items-center justify-center"
                    style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
                  >
                    <span class="font-mono text-[7px] uppercase tracking-[0.1em] text-ink-3">
                      {game.game_name.slice(0, 1)}
                    </span>
                  </div>
                {/if}
              </div>
              <div style:width={tileW}>
                <div
                  class="truncate font-medium"
                  style:font-size="var(--text-sm)"
                  style:color={active ? 'var(--color-ink-0)' : 'var(--color-ink-1)'}
                >
                  {game.game_name}
                </div>
                <div class="mt-px font-mono text-ink-3" style:font-size="9.5px">
                  {game.last_played_at ? relDate(game.last_played_at) : 'unplayed'}
                </div>
              </div>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if ctxMenu}
  <LibraryContextMenu
    game={ctxMenu.game}
    x={ctxMenu.x}
    y={ctxMenu.y}
    onclose={() => (ctxMenu = null)}
  />
{/if}
