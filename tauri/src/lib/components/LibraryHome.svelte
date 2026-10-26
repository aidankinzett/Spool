<script lang="ts">
  /**
   * Desktop library Home view: a recently played section followed by every
   * game. `layout` (Settings → Display → Home layout) picks one of three
   * arrangements:
   *   - shelf:     wide capsules for recent games, then a cover grid
   *   - spotlight: the last-played game as a banner, then the rest of the
   *                recent row as covers, then the cover grid
   *   - list:      recent covers, then a sortable table
   *
   * Clicking a game opens its detail page (`onOpen`); the hover Play button
   * opens it and launches in one go (`onPlay`).
   */
  import { Info, Play } from '@lucide/svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { assetUrl, peerAssetUrl } from '$lib/api';
  import { fmtPlaytime, fmtSize, relDate } from '$lib/format';
  import { HOME_SORTS, recentGames, sortGames, type HomeSort } from '$lib/home';
  import type { DisplayGame, HomeLayout } from '$lib/types';
  import { isSyntheticPeerId, type Library } from '$lib/library.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import PlayStatusBadge from '$lib/components/PlayStatusBadge.svelte';

  let {
    lib,
    layout,
    onOpen,
    onPlay,
    onContext,
  }: {
    lib: Library;
    layout: HomeLayout;
    onOpen: (id: string) => void;
    onPlay: (id: string) => void;
    onContext: (e: MouseEvent, g: DisplayGame) => void;
  } = $props();

  const RECENT_LIMIT = 10;
  const SORT_KEY = 'spool.home.sort';

  function loadSort(): HomeSort {
    try {
      const v = localStorage.getItem(SORT_KEY);
      if (HOME_SORTS.some((s) => s.value === v)) return v as HomeSort;
    } catch {
      // Storage unavailable — use the default.
    }
    return 'name';
  }
  let sort = $state<HomeSort>(loadSort());
  function setSort(v: string) {
    sort = v as HomeSort;
    try {
      localStorage.setItem(SORT_KEY, sort);
    } catch {
      // Not persisted; the choice still applies for this session.
    }
  }

  const coverErrors = new SvelteSet<string>();
  const heroErrors = new SvelteSet<string>();

  const recent = $derived(recentGames(lib.displayGames, RECENT_LIMIT));
  const spotlight = $derived(layout === 'spotlight' ? (recent[0] ?? null) : null);
  const recentRow = $derived(spotlight ? recent.slice(1) : recent);
  const allGames = $derived(sortGames(lib.displayGames, sort));

  function coverFor(g: DisplayGame): string | null {
    if (coverErrors.has(g.id)) return null;
    return assetUrl(g.cover_image_path) ?? (g.peer_source ? peerAssetUrl(g.peer_source, 'cover') : null);
  }
  function heroFor(g: DisplayGame): string | null {
    if (heroErrors.has(g.id)) return null;
    return assetUrl(g.hero_image_path) ?? (g.peer_source ? peerAssetUrl(g.peer_source, 'hero') : null);
  }
  const canPlay = (g: DisplayGame) => g.installed && !!g.exe_path && !isSyntheticPeerId(g.id);
  const accentOf = (g: DisplayGame) => g.accent_color ?? 'var(--color-spool)';
  const subline = (g: DisplayGame) =>
    !g.installed && g.peer_source
      ? `from ${g.peer_source.device_name}`
      : g.last_played_at
        ? relDate(g.last_played_at)
        : 'unplayed';
</script>

{#snippet letterTile(g: DisplayGame)}
  <div
    class="flex h-full w-full items-center justify-center"
    style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
  >
    <span class="font-display text-[22px] font-bold text-ink-3">{g.game_name.slice(0, 1)}</span>
  </div>
{/snippet}

{#snippet playButton(g: DisplayGame, size: number)}
  {#if canPlay(g)}
    <button
      type="button"
      tabindex="-1"
      onclick={() => onPlay(g.id)}
      aria-label={`Play ${g.game_name}`}
      title="Play"
      class="pointer-events-auto absolute bottom-2 right-2 flex cursor-pointer items-center justify-center rounded-full border-none text-bg-0 opacity-0 shadow-lg transition-opacity group-hover:opacity-100"
      style:width="{size}px"
      style:height="{size}px"
      style:background={accentOf(g)}
    >
      <Play size={size * 0.45} fill="currentColor" />
    </button>
  {/if}
{/snippet}

{#snippet coverTile(g: DisplayGame, width: string | null)}
  {@const cover = coverFor(g)}
  <div class="group relative shrink-0" style:width={width ?? undefined} style:opacity={g.installed ? 1 : 0.45}>
    <button
      type="button"
      data-game-id={g.id}
      onclick={() => onOpen(g.id)}
      oncontextmenu={(e) => onContext(e, g)}
      class="flex w-full cursor-pointer flex-col gap-1.5 border-none bg-transparent p-0 text-left"
    >
      <div
        class="aspect-[2/3] w-full overflow-hidden rounded-sm border border-line-1 bg-bg-2 transition-transform duration-150 group-hover:-translate-y-0.5"
      >
        {#if cover}
          <img src={cover} alt={g.game_name} class="h-full w-full object-cover" loading="lazy" onerror={() => coverErrors.add(g.id)} />
        {:else}
          {@render letterTile(g)}
        {/if}
      </div>
      <div class="min-w-0">
        <div class="truncate text-[12.5px] font-medium text-ink-0" title={g.game_name}>{g.game_name}</div>
        <div class="font-mono mt-px truncate text-[9.5px] tracking-[0.06em] text-ink-3">{subline(g)}</div>
      </div>
    </button>
    <!-- Positioned over the cover (not the caption): the caption is ~34px tall. -->
    <div class="pointer-events-none absolute inset-x-0 top-0 aspect-[2/3]">
      {#if g.play_status}
        <span class="absolute left-1.5 top-1.5"><PlayStatusBadge game={g} size="sm" /></span>
      {/if}
      {@render playButton(g, 34)}
    </div>
  </div>
{/snippet}

{#snippet capsule(g: DisplayGame)}
  {@const hero = heroFor(g)}
  {@const cover = coverFor(g)}
  <div class="group relative shrink-0" style:width="300px" style:opacity={g.installed ? 1 : 0.45}>
    <button
      type="button"
      data-game-id={g.id}
      onclick={() => onOpen(g.id)}
      oncontextmenu={(e) => onContext(e, g)}
      class="relative block h-[150px] w-full cursor-pointer overflow-hidden rounded-md border border-line-1 bg-bg-2 p-0 text-left transition-transform duration-150 group-hover:-translate-y-0.5"
    >
      {#if hero}
        <img src={hero} alt="" class="absolute inset-0 h-full w-full object-cover" loading="lazy" onerror={() => heroErrors.add(g.id)} />
      {:else if cover}
        <!-- No hero art: a blurred cover fills the capsule, with the sharp cover on the left. -->
        <img src={cover} alt="" class="absolute inset-0 h-full w-full scale-125 object-cover blur-xl brightness-50" />
        <img src={cover} alt="" class="absolute bottom-0 left-3 top-3 h-[calc(100%-24px)] rounded-sm object-cover shadow-xl" />
      {:else}
        <div class="absolute inset-0" style:background="linear-gradient(135deg, color-mix(in srgb, {accentOf(g)} 30%, #0a0807), #0a0807)"></div>
      {/if}
      <div class="absolute inset-0" style:background="linear-gradient(to top, rgb(0 0 0 / 0.85), transparent 60%)"></div>
      <div class="absolute inset-x-3 bottom-2.5 pr-10">
        <div class="truncate text-[14px] font-semibold text-ink-0" style:text-shadow="0 1px 8px rgb(0 0 0 / 0.6)" title={g.game_name}>
          {g.game_name}
        </div>
        <div class="font-mono mt-0.5 text-[9.5px] tracking-[0.06em] text-ink-2">
          {subline(g).toUpperCase()} · {fmtPlaytime(g.playtime_minutes)}
        </div>
      </div>
    </button>
    {#if g.play_status}
      <span class="pointer-events-none absolute left-2 top-2"><PlayStatusBadge game={g} size="sm" /></span>
    {/if}
    {@render playButton(g, 34)}
  </div>
{/snippet}

<div class="min-h-0 min-w-0 flex-1 overflow-y-auto">
  <div class="flex flex-col gap-7 px-6 pb-8 pt-5">
    {#if spotlight}
      {@const hero = heroFor(spotlight)}
      {@const cover = coverFor(spotlight)}
      <section
        class="relative h-[300px] shrink-0 overflow-hidden rounded-md border border-line-1"
        style:background="linear-gradient(135deg, color-mix(in srgb, {accentOf(spotlight)} 35%, #0a0807), #0a0807 70%)"
      >
        {#if hero}
          <img src={hero} alt="" class="absolute inset-0 h-full w-full object-cover" onerror={() => heroErrors.add(spotlight.id)} />
        {:else if cover}
          <img src={cover} alt="" class="absolute inset-0 h-full w-full scale-125 object-cover blur-2xl brightness-[0.4]" />
        {/if}
        <div class="absolute inset-0" style:background="linear-gradient(to right, rgb(0 0 0 / 0.85), rgb(0 0 0 / 0.2) 65%, transparent)"></div>
        <div class="absolute inset-0 flex items-end gap-5 p-6">
          {#if cover && !hero}
            <img src={cover} alt={spotlight.game_name} class="h-full rounded-sm object-cover shadow-2xl" />
          {/if}
          <div class="flex min-w-0 max-w-[560px] flex-col gap-2.5">
            <div class="flex items-center gap-2.5">
              <MonoLabel size={9.5}>Continue playing</MonoLabel>
              <PlayStatusBadge game={spotlight} size="md" label />
            </div>
            <h2
              class="m-0 font-display text-[34px] font-bold leading-[1] tracking-[-0.025em]"
              style:text-wrap="balance"
              style:text-shadow="0 2px 22px rgb(0 0 0 / 0.55)"
            >
              {spotlight.game_name}
            </h2>
            <div class="font-mono flex flex-wrap gap-2 text-[11px] tracking-[0.05em] text-ink-2">
              {#if spotlight.developer}<span>{spotlight.developer}</span><span class="text-ink-3">·</span>{/if}
              <span>{fmtPlaytime(spotlight.playtime_minutes)}</span>
              <span class="text-ink-3">·</span>
              <span>LAST · {relDate(spotlight.last_played_at).toUpperCase()}</span>
            </div>
            <div class="mt-1 flex items-center gap-2">
              {#if canPlay(spotlight)}
                <button
                  type="button"
                  data-gp-autofocus
                  onclick={() => onPlay(spotlight.id)}
                  class="inline-flex h-9 cursor-pointer items-center gap-2 rounded-sm border-none px-5 text-[13px] font-semibold text-bg-0 transition-[filter] hover:brightness-110"
                  style:background={accentOf(spotlight)}
                >
                  <Play size={14} fill="currentColor" /> Play
                </button>
              {/if}
              <button
                type="button"
                onclick={() => onOpen(spotlight.id)}
                class="inline-flex h-9 cursor-pointer items-center gap-2 rounded-sm border border-line-2 bg-black/40 px-4 text-[13px] text-ink-1 transition-colors hover:text-ink-0"
              >
                <Info size={14} /> Details
              </button>
            </div>
          </div>
        </div>
      </section>
    {/if}

    {#if recentRow.length > 0}
      <section class="flex flex-col gap-3">
        <div class="flex items-baseline gap-2">
          <MonoLabel size={10}>{spotlight ? 'Also played recently' : 'Recently played'}</MonoLabel>
          <span class="text-[11px] text-ink-3">{recentRow.length}</span>
        </div>
        <div class="-mx-6 flex gap-4 overflow-x-auto px-6 pb-2 pt-1">
          {#each recentRow as g (g.id)}
            {#if layout === 'shelf'}
              {@render capsule(g)}
            {:else}
              {@render coverTile(g, '140px')}
            {/if}
          {/each}
        </div>
      </section>
    {:else if !spotlight}
      <section class="rounded-md border border-dashed border-line-2 px-4 py-5 text-[12.5px] text-ink-3">
        Nothing played yet. Games you launch from Spool show up here.
      </section>
    {/if}

    <section class="flex flex-col gap-3">
      <div class="flex items-center justify-between gap-3">
        <div class="flex items-baseline gap-2">
          <MonoLabel size={10}>All games</MonoLabel>
          <span class="text-[11px] text-ink-3">{allGames.length}</span>
        </div>
        <div class="flex items-center gap-1" role="group" aria-label="Sort games">
          <span class="mr-1 text-[11px] text-ink-3">Sort</span>
          {#each HOME_SORTS as s (s.value)}
            {@const on = sort === s.value}
            <button
              type="button"
              aria-pressed={on}
              onclick={() => setSort(s.value)}
              class="cursor-pointer rounded-sm border px-2 py-1 text-[11.5px] font-medium transition-colors"
              style:background={on ? 'var(--color-bg-3)' : 'transparent'}
              style:border-color={on ? 'var(--color-line-2)' : 'transparent'}
              style:color={on ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
            >
              {s.label}
            </button>
          {/each}
        </div>
      </div>

      {#if layout === 'list'}
        <div class="overflow-hidden rounded-md border border-line-1">
          <div
            class="font-mono grid items-center gap-3 border-b border-line-1 bg-bg-1 px-3 py-2 text-[9.5px] uppercase tracking-[0.1em] text-ink-3"
            style:grid-template-columns="32px minmax(0,1fr) 110px 90px 80px"
          >
            <span></span><span>Name</span><span>Last played</span><span class="text-right">Playtime</span><span class="text-right">Size</span>
          </div>
          {#each allGames as g (g.id)}
            {@const cover = coverFor(g)}
            <button
              type="button"
              data-game-id={g.id}
              onclick={() => onOpen(g.id)}
              ondblclick={() => canPlay(g) && onPlay(g.id)}
              oncontextmenu={(e) => onContext(e, g)}
              class="grid w-full cursor-pointer items-center gap-3 border-none border-b border-line-1 bg-transparent px-3 py-1.5 text-left transition-colors last:border-b-0 hover:bg-white/[0.04]"
              style:grid-template-columns="32px minmax(0,1fr) 110px 90px 80px"
              style:opacity={g.installed ? 1 : 0.45}
            >
              <div class="relative h-11 w-8">
                <div class="h-full w-full overflow-hidden rounded-sm border border-line-1 bg-bg-2">
                  {#if cover}
                    <img src={cover} alt="" class="h-full w-full object-cover" loading="lazy" onerror={() => coverErrors.add(g.id)} />
                  {:else}
                    {@render letterTile(g)}
                  {/if}
                </div>
                {#if g.play_status}
                  <span class="absolute -bottom-0.5 -left-0.5"><PlayStatusBadge game={g} size="dot" /></span>
                {/if}
              </div>
              <div class="min-w-0">
                <div class="truncate text-[12.5px] font-medium text-ink-0" title={g.game_name}>{g.game_name}</div>
                {#if g.developer || (!g.installed && g.peer_source)}
                  <div class="truncate text-[11px] text-ink-3">
                    {!g.installed && g.peer_source ? `On LAN · from ${g.peer_source.device_name}` : g.developer}
                  </div>
                {/if}
              </div>
              <span class="font-mono text-[10.5px] text-ink-2">{g.last_played_at ? relDate(g.last_played_at) : '—'}</span>
              <span class="font-mono text-right text-[10.5px] text-ink-2">{fmtPlaytime(g.playtime_minutes)}</span>
              <span class="font-mono text-right text-[10.5px] text-ink-2">{fmtSize(g.install_size_mb)}</span>
            </button>
          {/each}
        </div>
      {:else}
        <div class="grid gap-x-4 gap-y-5" style:grid-template-columns="repeat(auto-fill, minmax(128px, 1fr))">
          {#each allGames as g (g.id)}
            {@render coverTile(g, null)}
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>
