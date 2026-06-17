<script lang="ts">
  /**
   * Checkable "add this game to a collection" list. Each row toggles the game's
   * membership in that collection (filled accent box + check when a member); a
   * footer row creates a new collection inline and adds the game to it.
   *
   * Shared by the detail-pane membership strip's popover and the library
   * right-click menu's "Add to collection" submenu, so both stay in lockstep.
   */
  import { Check, Plus } from '@lucide/svelte';
  import type { Collection } from '$lib/types';

  let {
    collections,
    gameId,
    accent = 'var(--color-spool)',
    onToggle,
    onCreate,
  }: {
    collections: Collection[];
    gameId: string;
    accent?: string;
    onToggle: (collectionId: string, gameId: string) => void;
    /** Create a collection seeded with `gameId`; returns the new id. */
    onCreate: (name: string, seedGameId: string) => string;
  } = $props();

  let adding = $state(false);
  let draft = $state('');
  let inputEl: HTMLInputElement | undefined = $state();

  function startAdding() {
    adding = true;
    draft = '';
    queueMicrotask(() => inputEl?.focus());
  }

  function commit() {
    // Ignore an empty/whitespace name so an accidental Enter doesn't create a
    // junk "New collection" (the controller would otherwise substitute that).
    if (!draft.trim()) {
      adding = false;
      return;
    }
    onCreate(draft, gameId);
    draft = '';
    adding = false;
  }
</script>

<div class="flex flex-col">
  <div class="flex max-h-[230px] flex-col gap-px overflow-y-auto">
    {#each collections as c (c.id)}
      {@const member = c.games.includes(gameId)}
      <button
        type="button"
        onclick={() => onToggle(c.id, gameId)}
        class="flex w-full items-center gap-2.5 rounded-sm px-2.5 py-1.5 text-left text-[12.5px] text-ink-0 transition-colors hover:bg-white/5"
      >
        <span
          class="h-2.5 w-2.5 shrink-0 rounded-full"
          style:background={c.accent}
          style:box-shadow="0 0 0 3px {c.accent}22"
        ></span>
        <span class="min-w-0 flex-1 truncate">{c.name}</span>
        <span
          class="flex h-4 w-4 shrink-0 items-center justify-center rounded-[3px] text-bg-0"
          style:background={member ? c.accent : 'transparent'}
          style:border={member ? 'none' : '1.5px solid var(--color-line-3)'}
        >
          {#if member}<Check size={12} strokeWidth={3} />{/if}
        </span>
      </button>
    {/each}
    {#if collections.length === 0}
      <p class="px-2.5 py-2 text-[11.5px] text-ink-3">No collections yet.</p>
    {/if}
  </div>

  <div class="mt-1 border-t border-line-1 pt-1.5">
    {#if adding}
      <div class="flex items-center gap-1.5">
        <input
          bind:this={inputEl}
          bind:value={draft}
          placeholder="Collection name…"
          onkeydown={(e) => {
            if (e.key === 'Enter') commit();
            else if (e.key === 'Escape') adding = false;
          }}
          class="font-sans h-7 min-w-0 flex-1 rounded-sm bg-white/5 px-2.5 text-[12.5px] text-ink-0 outline-none"
          style:border="1px solid {accent}66"
        />
        <button
          type="button"
          onclick={commit}
          class="h-7 shrink-0 rounded-sm px-2.5 text-[12px] font-semibold text-bg-0"
          style:background={accent}
        >
          Add
        </button>
      </div>
    {:else}
      <button
        type="button"
        onclick={startAdding}
        class="flex w-full items-center gap-2 rounded-sm px-2.5 py-1.5 text-left text-[12.5px] transition-colors hover:bg-white/5"
        style:color={accent}
      >
        <Plus size={13} /> New collection…
      </button>
    {/if}
  </div>
</div>
