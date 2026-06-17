<script lang="ts">
  /**
   * Detail-pane membership strip — sits above the game's hero. Shows the
   * collections the selected game belongs to as removable chips (click the name
   * to scope the sidebar to that collection, the × to drop the game from it),
   * plus an "Add to collection" button that opens the checkable list.
   */
  import { onMount } from 'svelte';
  import { Plus, X } from '@lucide/svelte';
  import MonoLabel from './MonoLabel.svelte';
  import AddToCollectionList from './AddToCollectionList.svelte';
  import type { Collection } from '$lib/types';

  let {
    collections,
    gameId,
    accent = 'var(--color-spool)',
    onToggle,
    onCreate,
    onSelectCollection,
  }: {
    collections: Collection[];
    gameId: string;
    accent?: string;
    onToggle: (collectionId: string, gameId: string) => void;
    onCreate: (name: string, seedGameId: string) => string;
    onSelectCollection: (collectionId: string) => void;
  } = $props();

  const mine = $derived(collections.filter((c) => c.games.includes(gameId)));

  let open = $state(false);
  let popoverEl: HTMLDivElement | undefined = $state();
  let anchorEl: HTMLDivElement | undefined = $state();

  function handleOutside(e: MouseEvent) {
    if (!open) return;
    if (popoverEl?.contains(e.target as Node)) return;
    if (anchorEl?.contains(e.target as Node)) return;
    open = false;
  }
  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') open = false;
  }
  onMount(() => {
    document.addEventListener('mousedown', handleOutside, true);
    document.addEventListener('keydown', handleKey, true);
    return () => {
      document.removeEventListener('mousedown', handleOutside, true);
      document.removeEventListener('keydown', handleKey, true);
    };
  });
</script>

<div
  class="flex shrink-0 items-center gap-3 border-b border-line-1 bg-black/20 px-6 py-2.5"
>
  <MonoLabel size={10}>Collections</MonoLabel>
  <div class="flex flex-1 flex-wrap items-center gap-1.5">
    {#if mine.length === 0}
      <span class="text-[12px] text-ink-3">Not in any collection yet</span>
    {/if}
    {#each mine as c (c.id)}
      <span
        class="inline-flex h-6 items-center gap-1.5 rounded-full pl-2.5 pr-1.5 text-[12px] text-ink-0"
        style:background="{c.accent}1f"
        style:border="1px solid {c.accent}55"
      >
        <span class="h-[7px] w-[7px] shrink-0 rounded-full" style:background={c.accent}></span>
        <button
          type="button"
          onclick={() => onSelectCollection(c.id)}
          class="cursor-pointer border-none bg-transparent p-0 text-inherit"
          title="Show this collection"
        >
          {c.name}
        </button>
        <button
          type="button"
          onclick={() => onToggle(c.id, gameId)}
          title="Remove from collection"
          class="flex h-4 w-4 items-center justify-center rounded-full border-none bg-transparent p-0 text-ink-2 transition-colors hover:text-ink-0"
        >
          <X size={11} />
        </button>
      </span>
    {/each}
  </div>

  <div bind:this={anchorEl} class="relative shrink-0">
    <button
      type="button"
      onclick={() => (open = !open)}
      class="inline-flex h-[30px] items-center gap-1.5 rounded-sm px-3 text-[12.5px] font-medium text-ink-0 transition-colors"
      style:background={open ? 'var(--color-bg-3)' : 'var(--color-bg-2)'}
      style:border="1px solid {open ? 'var(--color-line-3)' : 'var(--color-line-2)'}"
    >
      <Plus size={13} /> Add to collection
    </button>
    {#if open}
      <div
        bind:this={popoverEl}
        class="absolute right-0 z-50 mt-1.5 w-[250px] rounded-md border border-line-2 bg-bg-1 p-1.5"
        style:box-shadow="0 16px 40px rgb(0 0 0 / 0.5)"
      >
        <AddToCollectionList {collections} {gameId} {accent} {onToggle} {onCreate} />
      </div>
    {/if}
  </div>
</div>
