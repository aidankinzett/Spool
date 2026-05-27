<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import GameCard from '$lib/GameCard.svelte';
  import WindowChrome from '$lib/components/WindowChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import type { GameEntry } from '$lib/types';

  let games = $state<GameEntry[]>([]);
  let error = $state<string | null>(null);
  let loaded = $state(false);

  onMount(async () => {
    try {
      games = await api.listGames();
    } catch (e) {
      error = String(e);
    } finally {
      loaded = true;
    }
  });
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <WindowChrome sub="LIBRARY" />

  <main class="flex flex-1 flex-col overflow-hidden">
    <!-- Toolbar: section eyebrow + actions -->
    <header class="flex items-center justify-between gap-4 border-b border-line-1 px-6 py-3">
      <div class="flex items-baseline gap-3">
        <MonoLabel size={11}>Library</MonoLabel>
        {#if loaded && !error}
          <span class="font-mono text-[11px] tracking-[0.08em] text-ink-3 num">
            {games.length.toString().padStart(3, '0')} TITLES
          </span>
        {/if}
      </div>
      <div class="flex items-center gap-2">
        <Btn variant="ghost">
          {#snippet icon()}<Icon name="search" />{/snippet}
          Search
        </Btn>
        <Btn variant="primary">
          {#snippet icon()}<Icon name="plus" />{/snippet}
          Add game
        </Btn>
      </div>
    </header>

    <!-- Grid -->
    <div class="flex-1 overflow-auto px-6 py-6">
      {#if error}
        <div class="rounded-md border border-bad/40 bg-bad/10 p-4 text-sm text-bad">
          <div class="mb-1 font-medium">Failed to load library</div>
          <code class="font-mono text-[11px] opacity-80">{error}</code>
        </div>
      {:else if !loaded}
        <p class="font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">Loading…</p>
      {:else if games.length === 0}
        <div class="flex flex-col items-center justify-center gap-3 py-24 text-center">
          <MonoLabel>Empty shelf</MonoLabel>
          <p class="max-w-md text-sm text-ink-2">
            No games yet. Add an executable to start your collection.
          </p>
          <Btn variant="primary">
            {#snippet icon()}<Icon name="plus" />{/snippet}
            Add your first game
          </Btn>
        </div>
      {:else}
        <div class="flex flex-wrap gap-5">
          {#each games as game (game.id)}
            <GameCard {game} />
          {/each}
        </div>
      {/if}
    </div>
  </main>
</div>
