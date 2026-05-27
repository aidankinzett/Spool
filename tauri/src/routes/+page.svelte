<script lang="ts">
  import { onMount } from 'svelte';
  import { Plus, Search, Settings } from '@lucide/svelte';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { listen } from '@tauri-apps/api/event';
  import { api } from '$lib/api';
  import GameCard from '$lib/GameCard.svelte';
  import WindowChrome from '$lib/components/WindowChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import type { GameEntry } from '$lib/types';

  let games = $state<GameEntry[]>([]);
  let error = $state<string | null>(null);
  let loaded = $state(false);

  async function refresh() {
    try {
      games = await api.listGames();
    } catch (e) {
      error = String(e);
    } finally {
      loaded = true;
    }
  }

  let unlistenLibraryChanged: (() => void) | undefined;

  onMount(() => {
    refresh();
    // Backend emits `library.changed` from add/update/remove. Listen
    // here so the library refreshes when the Add Game popup adds a
    // game in a sibling window.
    listen<string>('library:changed', (event) => {
      console.debug('[library] library.changed received for id', event.payload);
      refresh();
    })
      .then((fn) => {
        unlistenLibraryChanged = fn;
        console.debug('[library] library.changed listener registered');
      })
      .catch((e) => console.error('[library] failed to register listener:', e));

    return () => unlistenLibraryChanged?.();
  });

  /**
   * Opens Add Game as a child webview window. The new window loads the
   * `/add` route; closing it returns focus to the library, and any
   * library.changed events emitted from the add flow are picked up
   * automatically via the listener above.
   */
  function openAddGame() {
    const existing = WebviewWindow.getByLabel('add-game');
    existing.then((win) => {
      if (win) {
        win.setFocus();
        return;
      }
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const _added = new WebviewWindow('add-game', {
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
          {#snippet icon()}<Search size={14} />{/snippet}
          Search
        </Btn>
        <a
          href="/settings"
          aria-label="Settings"
          class="inline-flex h-8 w-8 items-center justify-center rounded-sm border border-line-1 text-ink-1 transition-colors hover:bg-white/5 hover:text-ink-0"
        >
          <Settings size={14} />
        </a>
        <button
          type="button"
          onclick={openAddGame}
          class="inline-flex h-8 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-sm border border-transparent bg-spool px-3 text-[12.5px] font-medium text-bg-0 transition-colors hover:brightness-95"
        >
          <Plus size={14} />
          Add game
        </button>
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
          <button
            type="button"
            onclick={openAddGame}
            class="inline-flex h-8 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-sm border border-transparent bg-spool px-3 text-[12.5px] font-medium text-bg-0 transition-colors hover:brightness-95"
          >
            <Plus size={14} />
            Add your first game
          </button>
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
