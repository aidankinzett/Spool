<script lang="ts">
  /**
   * Collapsible "Collections" section in the library sidebar (design Direction
   * A). Lists an "All games" row plus each user collection (colour dot, name,
   * member count). A collection can be:
   *   - selected to scope the game list to its members
   *   - renamed / recoloured / deleted from its ⋯ menu
   *   - reordered by dragging one collection row onto another
   *   - filled by dragging a game row onto it (adds membership)
   *
   * All state mutations go through the library controller (`lib`), which
   * persists and broadcasts them.
   */
  import { onMount } from 'svelte';
  import { ChevronRight, MoreHorizontal, Pencil, Plus, Trash2 } from '@lucide/svelte';
  import MonoLabel from './MonoLabel.svelte';
  import { COLLECTION_ACCENTS, type Library } from '$lib/library.svelte';

  let { lib }: { lib: Library } = $props();

  const ACCENT = 'var(--color-spool)';

  let open = $state(true);
  let adding = $state(false);
  let draft = $state('');
  let addInputEl: HTMLInputElement | undefined = $state();

  let renamingId = $state<string | null>(null);
  let renameDraft = $state('');
  let renameInputEl: HTMLInputElement | undefined = $state();

  let menuId = $state<string | null>(null);
  // Which row a drag is hovering, and what would happen on drop.
  let dropTarget = $state<{ id: string; mode: 'add' | 'reorder' } | null>(null);

  function startAdding() {
    open = true;
    adding = true;
    draft = '';
    queueMicrotask(() => addInputEl?.focus());
  }
  function commitAdd() {
    const id = lib.createCollection(draft);
    lib.activeCollection = id;
    draft = '';
    adding = false;
  }

  function startRename(id: string, current: string) {
    menuId = null;
    renamingId = id;
    renameDraft = current;
    queueMicrotask(() => {
      renameInputEl?.focus();
      renameInputEl?.select();
    });
  }
  function commitRename() {
    if (renamingId) lib.renameCollection(renamingId, renameDraft);
    renamingId = null;
  }

  function dragMode(e: DragEvent): 'add' | 'reorder' | null {
    const types = Array.from(e.dataTransfer?.types ?? []);
    if (types.includes('text/game-id')) return 'add';
    if (types.includes('text/collection-id')) return 'reorder';
    return null;
  }

  // Close the ⋯ menu on any outside click.
  function handleOutside(e: MouseEvent) {
    if (!menuId) return;
    const t = e.target as HTMLElement;
    if (t.closest('[data-coll-menu]') || t.closest('[data-coll-menu-btn]')) return;
    menuId = null;
  }
  onMount(() => {
    document.addEventListener('mousedown', handleOutside, true);
    return () => document.removeEventListener('mousedown', handleOutside, true);
  });
</script>

<div class="px-2 pb-1.5 pt-0.5">
  <!-- Section header -->
  <div class="flex items-center gap-1.5 px-2 py-1 text-ink-2">
    <button
      type="button"
      onclick={() => (open = !open)}
      class="flex flex-1 cursor-pointer items-center gap-1.5 border-none bg-transparent p-0 text-inherit"
    >
      <span
        class="flex transition-transform duration-150"
        style:transform={open ? 'rotate(90deg)' : 'rotate(0deg)'}
      >
        <ChevronRight size={12} />
      </span>
      <MonoLabel size={9.5}>Collections</MonoLabel>
      <span class="text-[10.5px] text-ink-3">{lib.collections.length}</span>
    </button>
    <button
      type="button"
      onclick={startAdding}
      title="New collection"
      class="flex h-[22px] w-[22px] items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-white/5 hover:text-ink-0"
    >
      <Plus size={13} />
    </button>
  </div>

  {#if open}
    <div class="mt-0.5 flex flex-col gap-px">
      <!-- All games -->
      <button
        type="button"
        onclick={() => (lib.activeCollection = null)}
        class="relative flex w-full items-center gap-2.5 rounded-sm py-1.5 pl-3 pr-2 text-left transition-colors"
        style:background={!lib.activeCollection ? 'var(--color-bg-3)' : 'transparent'}
      >
        {#if !lib.activeCollection}
          <span
            class="absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-full"
            style:background={ACCENT}
          ></span>
        {/if}
        <span
          class="h-2.5 w-2.5 shrink-0 rounded-full border-[1.5px] border-ink-3"
        ></span>
        <span class="flex-1 text-[12.5px] text-ink-0">All games</span>
        <span class="text-[11px] text-ink-3">{lib.games.length}</span>
      </button>

      <!-- Collection rows -->
      {#each lib.collections as c (c.id)}
        {@const active = lib.activeCollection === c.id}
        {@const isDropAdd = dropTarget?.id === c.id && dropTarget.mode === 'add'}
        {@const isDropReorder = dropTarget?.id === c.id && dropTarget.mode === 'reorder'}
        <div
          role="listitem"
          draggable={renamingId !== c.id}
          ondragstart={(e) => {
            e.dataTransfer?.setData('text/collection-id', c.id);
            if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
          }}
          ondragover={(e) => {
            const m = dragMode(e);
            if (!m) return;
            e.preventDefault();
            if (e.dataTransfer) e.dataTransfer.dropEffect = m === 'add' ? 'copy' : 'move';
            dropTarget = { id: c.id, mode: m };
          }}
          ondragleave={() => {
            if (dropTarget?.id === c.id) dropTarget = null;
          }}
          ondrop={(e) => {
            e.preventDefault();
            dropTarget = null;
            const gid = e.dataTransfer?.getData('text/game-id');
            const cid = e.dataTransfer?.getData('text/collection-id');
            if (gid) {
              if (!c.games.includes(gid)) lib.toggleMembership(c.id, gid);
            } else if (cid) {
              lib.reorderCollections(cid, c.id);
            }
          }}
          class="relative rounded-sm"
          style:border-top={isDropReorder ? `2px solid ${c.accent}` : '2px solid transparent'}
        >
          {#if renamingId === c.id}
            <div class="px-3 py-0.5">
              <input
                bind:this={renameInputEl}
                bind:value={renameDraft}
                onblur={commitRename}
                onkeydown={(e) => {
                  if (e.key === 'Enter') commitRename();
                  else if (e.key === 'Escape') (renamingId = null);
                }}
                class="font-sans h-7 w-full rounded-sm bg-white/5 px-2.5 text-[12.5px] text-ink-0 outline-none"
                style:border="1px solid {c.accent}66"
              />
            </div>
          {:else}
            <!-- Row select and the ⋯ menu trigger are siblings (not nested) so
                 each is its own focusable control with consistent keyboard /
                 screen-reader behaviour; the wrapper carries the shared hover,
                 active, and drop-target styling. -->
            <div
              class="group relative flex w-full items-center gap-2.5 rounded-sm py-1.5 pl-3 pr-2 transition-colors hover:bg-white/[0.025]"
              style:background={isDropAdd
                ? `${c.accent}1f`
                : active
                  ? 'var(--color-bg-3)'
                  : 'transparent'}
              style:border={isDropAdd ? `1px dashed ${c.accent}` : '1px solid transparent'}
            >
              {#if active}
                <span
                  class="absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-full"
                  style:background={c.accent}
                ></span>
              {/if}
              <button
                type="button"
                onclick={() => (lib.activeCollection = c.id)}
                class="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 border-none bg-transparent p-0 text-left text-inherit"
              >
                <span
                  class="h-2.5 w-2.5 shrink-0 rounded-full"
                  style:background={c.accent}
                  style:box-shadow="0 0 0 3px {c.accent}22"
                ></span>
                <span class="min-w-0 flex-1 truncate text-[12.5px] text-ink-0">{c.name}</span>
              </button>
              <!-- Member count by default; swaps to the ⋯ menu button on hover
                   (or while this row's menu is open). -->
              <span
                class="text-[11px] text-ink-3 group-hover:hidden"
                style:display={menuId === c.id ? 'none' : undefined}
              >
                {c.games.length}
              </span>
              <button
                type="button"
                data-coll-menu-btn
                onclick={() => (menuId = menuId === c.id ? null : c.id)}
                title="Collection options"
                aria-label="Collection options"
                class="hidden h-5 w-5 shrink-0 cursor-pointer items-center justify-center rounded-sm border-none text-ink-2 hover:text-ink-0 group-hover:flex"
                style:display={menuId === c.id ? 'flex' : undefined}
                style:background={menuId === c.id ? 'rgba(255,255,255,0.1)' : 'transparent'}
              >
                <MoreHorizontal size={15} />
              </button>
            </div>
          {/if}

          {#if menuId === c.id}
            <div
              data-coll-menu
              class="absolute right-1.5 top-[calc(100%-2px)] z-50 w-[186px] rounded-md border border-line-2 bg-bg-1 p-1.5"
              style:box-shadow="0 16px 40px rgb(0 0 0 / 0.5)"
            >
              <button
                type="button"
                onclick={() => startRename(c.id, c.name)}
                class="flex w-full items-center gap-2.5 rounded-sm px-2.5 py-1.5 text-left text-[12.5px] text-ink-0 transition-colors hover:bg-white/5"
              >
                <Pencil size={13} /> Rename
              </button>
              <div class="px-2.5 pb-1 pt-1.5">
                <MonoLabel size={9}>Color</MonoLabel>
              </div>
              <div class="flex flex-wrap gap-1.5 px-2.5 pb-2">
                {#each COLLECTION_ACCENTS as col (col)}
                  <button
                    type="button"
                    onclick={() => lib.setCollectionAccent(c.id, col)}
                    title={col}
                    class="h-5 w-5 cursor-pointer rounded-full"
                    style:background={col}
                    style:border={c.accent === col ? '2px solid #fff' : '2px solid transparent'}
                  ></button>
                {/each}
              </div>
              <div class="my-1 h-px bg-line-1"></div>
              <button
                type="button"
                onclick={() => {
                  menuId = null;
                  lib.deleteCollection(c.id);
                }}
                class="flex w-full items-center gap-2.5 rounded-sm px-2.5 py-1.5 text-left text-[12.5px] text-bad transition-colors hover:bg-bad/10"
              >
                <Trash2 size={13} /> Delete collection
              </button>
            </div>
          {/if}
        </div>
      {/each}

      {#if adding}
        <div class="py-0.5 pl-3 pr-1">
          <input
            bind:this={addInputEl}
            bind:value={draft}
            placeholder="Collection name…"
            onkeydown={(e) => {
              if (e.key === 'Enter') commitAdd();
              else if (e.key === 'Escape') (adding = false);
            }}
            onblur={() => (adding = false)}
            class="font-sans h-7 w-full rounded-sm bg-white/5 px-2.5 text-[12.5px] text-ink-0 outline-none"
            style:border="1px solid var(--color-line-3)"
          />
        </div>
      {/if}

      <p class="px-3 pb-0.5 pt-1 text-[10px] text-ink-3">
        Drag a game onto a collection · drag to reorder
      </p>
    </div>
  {/if}

  <div class="mx-1.5 mt-2 h-px bg-line-1"></div>
</div>
