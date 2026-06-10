<script lang="ts">
  /**
   * Installed-games view for Settings → Library. Lists every installed game
   * grouped by the library folder it lives in, with an "Other folders" group for
   * installs sitting outside any configured library folder. Checkboxes select
   * across groups; the action bar moves the selection into one chosen library
   * folder (BatchMoveModal) or removes the selected games from disk (uninstall —
   * the saves are backed up and the dimmed library entry is kept).
   *
   * Settings is its own window and doesn't share the main library store, so this
   * panel fetches its own game list and refreshes on the `library:changed` event.
   */
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { listen } from '@tauri-apps/api/event';
  import { FolderInput, HardDrive, Trash2, FolderTree } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { fmtSize, fmtCatalog } from '$lib/format';
  import { confirmDialog } from '$lib/confirm.svelte';
  import { toasts } from '$lib/toasts.svelte';
  import { isCurrentRoot, parentOf } from '$lib/pathMatch';
  import type { GameEntry, LibraryFolder } from '$lib/types';
  import SettingsCard from '$lib/components/SettingsCard.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import BatchMoveModal from '$lib/components/BatchMoveModal.svelte';

  let {
    folders,
    folderFree,
  }: {
    /** Configured library folders from app config. */
    folders: LibraryFolder[];
    /** path → available bytes, maintained by the settings page. */
    folderFree: Record<string, number>;
  } = $props();

  let games = $state<GameEntry[]>([]);
  const selected = new SvelteSet<string>();
  let showMove = $state(false);
  let busy = $state(false);

  async function loadGames() {
    try {
      games = await api.listGames();
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'LIBRARY', title: "Couldn't load games", sub: String(e) });
    }
  }

  onMount(() => {
    void loadGames();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<string>('library:changed', () => void loadGames())
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((e) => console.error('[library-panel] listener failed:', e));
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  // Only games whose files are actually on disk can be moved or uninstalled.
  const installed = $derived(games.filter((g) => g.installed && g.game_folder_path));

  type Group = {
    /** Stable key for keyed iteration. */
    key: string;
    /** Display label (folder label, or the path). */
    label: string;
    /** The folder path this group represents. */
    path: string;
    /** Available bytes, when known (configured folders only). */
    free: number | null;
    /** True for a configured library folder, false for an "Other folders" dir. */
    isFolder: boolean;
    games: GameEntry[];
  };

  // Group installed games: one group per configured library folder, plus a group
  // per stray parent directory for installs that match no configured folder.
  const groups = $derived.by<{ folders: Group[]; other: Group[] }>(() => {
    const folderGroups: Group[] = folders.map((f) => ({
      key: `f:${f.path}`,
      label: f.label || f.path,
      path: f.path,
      free: folderFree[f.path] ?? null,
      isFolder: true,
      games: [],
    }));
    const otherByParent: Record<string, GameEntry[]> = {};

    for (const g of installed) {
      const fg = folderGroups.find((grp) => isCurrentRoot(grp.path, g.game_folder_path));
      if (fg) {
        fg.games.push(g);
      } else {
        const parent = parentOf(g.game_folder_path ?? '') || '(unknown)';
        (otherByParent[parent] ??= []).push(g);
      }
    }

    const otherGroups: Group[] = Object.entries(otherByParent).map(([parent, gs]) => ({
      key: `o:${parent}`,
      label: parent,
      path: parent,
      free: null,
      isFolder: false,
      games: gs,
    }));

    return { folders: folderGroups, other: otherGroups };
  });

  const totalInstalled = $derived(installed.length);
  const selectedGames = $derived(installed.filter((g) => selected.has(g.id)));
  const selectedSize = $derived(selectedGames.reduce((sum, g) => sum + (g.install_size_mb || 0), 0));

  function groupBytes(g: Group): number {
    return g.games.reduce((sum, x) => sum + (x.install_size_mb || 0), 0);
  }

  function toggle(id: string) {
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
  }

  function groupState(g: Group): 'none' | 'some' | 'all' {
    if (g.games.length === 0) return 'none';
    const n = g.games.filter((x) => selected.has(x.id)).length;
    return n === 0 ? 'none' : n === g.games.length ? 'all' : 'some';
  }

  function toggleGroup(g: Group) {
    const all = groupState(g) === 'all';
    for (const x of g.games) {
      if (all) selected.delete(x.id);
      else selected.add(x.id);
    }
  }

  function clearSelection() {
    selected.clear();
  }

  // Native checkboxes don't expose `indeterminate` as an attribute — set it
  // imperatively from the group's partial-selection state.
  function indeterminate(node: HTMLInputElement, value: boolean) {
    node.indeterminate = value;
    return {
      update(v: boolean) {
        node.indeterminate = v;
      },
    };
  }

  async function deleteSelected() {
    const ids = [...selected];
    const n = ids.length;
    if (n === 0 || busy) return;
    const ok = await confirmDialog({
      danger: true,
      label: 'LIBRARY',
      title: `Remove ${n} ${n === 1 ? 'game' : 'games'} from disk?`,
      body: `Saves are backed up first, then the install files are deleted. The library ${
        n === 1 ? 'entry stays' : 'entries stay'
      } (dimmed) so you can reinstall later.`,
      confirmLabel: 'Delete from disk',
    });
    if (!ok) return;

    busy = true;
    const progressId = toasts.show({
      kind: 'info',
      label: 'LIBRARY',
      title: `Removing ${n} ${n === 1 ? 'game' : 'games'} from disk…`,
      sub: 'Backing up saves, then deleting install files.',
      duration: 0,
    });
    // Per-game run locks make these independent, so they can run concurrently.
    const settled = await Promise.allSettled(ids.map((id) => api.uninstallGame(id)));
    toasts.dismiss(progressId);
    busy = false;

    const failures = settled.filter((s): s is PromiseRejectedResult => s.status === 'rejected');
    const okCount = n - failures.length;
    if (failures.length === 0) {
      toasts.show({
        kind: 'ok',
        label: 'LIBRARY',
        title: `Removed ${okCount} ${okCount === 1 ? 'game' : 'games'} from disk`,
        sub: 'Library entries are kept — reinstall any time.',
      });
    } else {
      toasts.show({
        kind: 'bad',
        label: 'LIBRARY',
        title: `Removed ${okCount} of ${n}; ${failures.length} failed`,
        sub: String(failures[0].reason),
      });
    }
    clearSelection();
    // The list refreshes via `library:changed` emitted by each uninstall.
  }

  function openMove() {
    if (selected.size === 0 || busy) return;
    if (folders.length === 0) {
      toasts.show({
        kind: 'warn',
        label: 'LIBRARY',
        title: 'No library folders yet',
        sub: 'Add a library folder above before moving games.',
      });
      return;
    }
    showMove = true;
  }
</script>

{#snippet gameRow(g: GameEntry)}
  {@const checked = selected.has(g.id)}
  <label
    class="flex items-center gap-2.5 px-[18px] py-[7px] cursor-pointer transition-colors hover:bg-bg-2"
    style:background={checked ? 'color-mix(in srgb, var(--color-spool) 9%, transparent)' : 'transparent'}
  >
    <input
      type="checkbox"
      {checked}
      onchange={() => toggle(g.id)}
      class="shrink-0 size-[15px] accent-[var(--color-spool)]"
      aria-label={`Select ${g.game_name}`}
    />
    <span class="min-w-0 flex-1 truncate text-[12.5px] text-ink-1">{g.game_name}</span>
    <span class="shrink-0 font-mono text-[10px] text-ink-3">{fmtCatalog(g.catalog_number)}</span>
    <span class="shrink-0 text-[11px] text-ink-3 w-[64px] text-right">{fmtSize(g.install_size_mb)}</span>
  </label>
{/snippet}

{#snippet groupBlock(grp: Group)}
  {@const state = groupState(grp)}
  <div class="flex flex-col">
    <!-- group header -->
    <div class="flex items-center gap-2.5 px-[18px] py-[9px] bg-bg-2/60 border-b border-dashed border-line-1">
      <input
        type="checkbox"
        checked={state === 'all'}
        use:indeterminate={state === 'some'}
        onchange={() => toggleGroup(grp)}
        disabled={grp.games.length === 0}
        class="shrink-0 size-[15px] accent-[var(--color-spool)] disabled:opacity-40"
        aria-label={`Select all in ${grp.label}`}
      />
      <span class="flex shrink-0 text-ink-3">
        {#if grp.isFolder}<HardDrive size={13} />{:else}<FolderTree size={13} />{/if}
      </span>
      <span class="min-w-0 flex-1 truncate font-mono text-[11.5px] text-ink-1" title={grp.path}>{grp.label}</span>
      {#if grp.free != null}
        <span class="shrink-0 text-[10.5px] text-ink-3">{fmtSize(grp.free / 1048576)} free</span>
      {/if}
      <span class="shrink-0 text-[10.5px] text-ink-3">
        {grp.games.length} · {fmtSize(groupBytes(grp))}
      </span>
    </div>
    <!-- games -->
    {#if grp.games.length === 0}
      <div class="px-[18px] py-[10px] text-[11.5px] text-ink-3">No games here.</div>
    {:else}
      {#each grp.games as g (g.id)}
        {@render gameRow(g)}
      {/each}
    {/if}
  </div>
{/snippet}

<SettingsCard
  title="Installed games"
  helper="Every game with files on disk, grouped by library folder. Select games to move them into another folder, or remove them from disk (their saves are backed up and the library entry is kept)."
>
  {#if totalInstalled === 0}
    <div class="px-[18px] py-[14px] text-[12.5px] text-ink-3">No games are installed yet.</div>
  {:else}
    <div class="flex flex-col divide-y divide-line-1">
      {#each groups.folders as grp (grp.key)}
        {@render groupBlock(grp)}
      {/each}
      {#each groups.other as grp (grp.key)}
        {@render groupBlock(grp)}
      {/each}
    </div>

    <!-- action bar -->
    <div
      class="flex items-center gap-2.5 px-[18px] py-[11px] border-t border-line-1 bg-bg-2/50"
    >
      <span class="text-[12px] text-ink-2">
        {#if selected.size === 0}
          {totalInstalled} installed
        {:else}
          {selected.size} selected · {fmtSize(selectedSize)}
        {/if}
      </span>
      <div class="flex-1"></div>
      {#if selected.size > 0}
        <Btn variant="ghost" onclick={clearSelection}>Clear</Btn>
      {/if}
      <Btn variant="ghost" disabled={selected.size === 0 || busy} onclick={openMove}>
        {#snippet icon()}<FolderInput size={14} />{/snippet}
        Move…
      </Btn>
      <Btn variant="danger" disabled={selected.size === 0 || busy} onclick={deleteSelected}>
        {#snippet icon()}<Trash2 size={14} />{/snippet}
        Delete from disk
      </Btn>
    </div>
  {/if}
</SettingsCard>

{#if showMove}
  <BatchMoveModal
    games={selectedGames}
    {folders}
    onClose={() => (showMove = false)}
    onDone={() => {
      clearSelection();
    }}
  />
{/if}
