<script lang="ts">
  import { Search, X } from '@lucide/svelte';
  import { assetUrl } from '$lib/api';
  import { relDate } from '$lib/format';
  import type { Library } from '$lib/library.svelte';
  import MonoLabel from './MonoLabel.svelte';

  let { lib, onclose }: { lib: Library; onclose: () => void } = $props();
</script>

<div class="fixed inset-0 z-50 flex flex-col bg-bg-0/[0.97] text-ink-0">
  <!-- Search bar + cancel -->
  <div class="flex items-center gap-3 border-b border-line-1 px-4 py-3">
    <div
      class="flex flex-1 items-center gap-3 rounded-full border border-line-2 bg-bg-2"
      style:height="var(--control-h)"
      style:padding-inline="calc(var(--space-unit) * 4)"
    >
      <Search size={18} class="shrink-0 text-ink-2" />
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:value={lib.searchQuery}
        placeholder="Search {lib.games.length} games…"
        class="min-w-0 flex-1 bg-transparent font-display font-semibold tracking-[-0.01em] text-ink-0 outline-none placeholder:text-ink-3"
        style:font-size="var(--text-lg)"
        autofocus
      />
      {#if lib.searchQuery}
        <button
          type="button"
          onclick={() => (lib.searchQuery = '')}
          class="inline-flex shrink-0 cursor-pointer items-center justify-center rounded-full border-none bg-white/10 text-ink-2"
          style:width="var(--control-h-icon)"
          style:height="var(--control-h-icon)"
          aria-label="Clear search"
        >
          <X size={13} />
        </button>
      {/if}
    </div>
    <button
      type="button"
      onclick={onclose}
      class="shrink-0 cursor-pointer border-none bg-transparent text-ink-1"
      style:font-size="var(--text-base)"
    >
      Cancel
    </button>
  </div>

  <!-- Filter chips + result count -->
  <div class="flex flex-wrap items-center gap-2 px-4 py-2">
    {#each [
      { id: 'all' as const, label: 'All' },
      { id: 'recent' as const, label: 'Recent' },
      { id: 'played' as const, label: 'Played' },
    ] as f (f.id)}
      <button
        type="button"
        onclick={() => (lib.filter = f.id)}
        class="cursor-pointer whitespace-nowrap rounded-full border transition-colors"
        style:height="calc(var(--control-h) * 0.8)"
        style:padding-inline="calc(var(--space-unit) * 3)"
        style:font-size="var(--text-sm)"
        style:font-weight={lib.filter === f.id ? 600 : 500}
        style:background={lib.filter === f.id ? 'var(--color-bg-3)' : 'transparent'}
        style:border-color={lib.filter === f.id ? 'var(--color-line-2)' : 'transparent'}
        style:color={lib.filter === f.id ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
      >
        {f.label}
      </button>
    {/each}
    <div class="flex-1"></div>
    <MonoLabel size={9.5}>{lib.filteredGames.length} result{lib.filteredGames.length === 1 ? '' : 's'}</MonoLabel>
  </div>

  <!-- Cover grid -->
  <div class="min-h-0 flex-1 overflow-y-auto px-4 py-2">
    {#if lib.filteredGames.length === 0}
      <div class="flex items-center justify-center py-12">
        <p class="text-ink-3" style:font-size="var(--text-base)">
          {lib.searchQuery ? `No games match "${lib.searchQuery}".` : 'No games.'}
        </p>
      </div>
    {:else}
      <div
        class="grid gap-4"
        style:grid-template-columns="repeat(auto-fill, minmax(calc(var(--control-h) * 2.8), 1fr))"
      >
        {#each lib.filteredGames as game (game.id)}
          {@const cover = assetUrl(game.cover_image_path)}
          <button
            type="button"
            onclick={() => { lib.selectedId = game.id; onclose(); }}
            class="flex cursor-pointer flex-col gap-2 border-none bg-transparent text-left"
          >
            <div
              class="w-full overflow-hidden rounded-sm"
              style:aspect-ratio="2 / 2.8"
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
            <div>
              <div class="truncate font-medium" style:font-size="var(--text-sm)">
                {game.game_name}
              </div>
              <div class="mt-px font-mono text-ink-3" style:font-size="9.5px">
                {game.last_played_at ? relDate(game.last_played_at) : 'unplayed'}
              </div>
            </div>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>
