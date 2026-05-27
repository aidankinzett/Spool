<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import GameCard from '$lib/GameCard.svelte';
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

<main class="min-h-screen bg-neutral-950 px-6 py-8 text-neutral-100">
  <header class="mb-6 flex items-baseline justify-between">
    <h1 class="text-2xl font-semibold">Library</h1>
    {#if loaded && !error}
      <span class="text-sm text-neutral-400">{games.length} games</span>
    {/if}
  </header>

  {#if error}
    <div class="rounded-md border border-red-900 bg-red-950/50 p-4 text-sm text-red-200">
      <div class="mb-1 font-medium">Failed to load library</div>
      <code class="text-xs text-red-300">{error}</code>
    </div>
  {:else if !loaded}
    <p class="text-sm text-neutral-500">Loading…</p>
  {:else if games.length === 0}
    <p class="text-sm text-neutral-500">No games in library yet.</p>
  {:else}
    <div class="flex flex-wrap gap-5">
      {#each games as game (game.id)}
        <GameCard {game} />
      {/each}
    </div>
  {/if}
</main>
