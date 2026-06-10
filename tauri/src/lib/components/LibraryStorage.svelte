<script lang="ts">
  /**
   * Settings → Library storage view ("Direction A · Drive meters").
   *
   * Merges what used to be two redundant cards — *Library folders* (a plain
   * list) and *Installed games* (grouped by those same folders) — into one
   * storage view. Each configured library folder is a drive panel with a
   * capacity bar (Spool games / other on disk / free) and its games listed
   * underneath, ranked largest-first with a proportional size meter so disk
   * hogs stand out. Installs that sit outside any configured folder fall into an
   * "Outside library folders" panel.
   *
   * Selection is always-on: each row and group header carries a themed accent
   * checkbox. Selecting games reveals a sticky bulk bar to move them into one
   * library folder (BatchMoveModal) or remove them from disk (uninstall — saves
   * are backed up first and the dimmed library entry is kept).
   *
   * Settings is its own window and doesn't share the main library store, so this
   * panel fetches its own game list and refreshes on the `library:changed` event.
   * Folder add/remove and capacity figures are owned by the settings page and
   * flow in via props/callbacks.
   */
  import { SvelteSet } from 'svelte/reactivity';
  import { Folder, FolderInput, FolderTree, HardDrive, Plus, Trash2 } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import { fmtSize, fmtCatalog } from '$lib/format';
  import { confirmDialog } from '$lib/confirm.svelte';
  import { toasts } from '$lib/toasts.svelte';
  import { onLibraryChanged } from '$lib/libraryEvents';
  import { isCurrentRoot, parentOf } from '$lib/pathMatch';
  import type { DriveInfo, FolderCapacity, GameEntry, LibraryFolder } from '$lib/types';
  import Btn from '$lib/components/Btn.svelte';
  import Checkbox from '$lib/components/Checkbox.svelte';
  import TextField from '$lib/components/TextField.svelte';
  import BatchMoveModal from '$lib/components/BatchMoveModal.svelte';

  let {
    folders,
    capacity,
    onAddFolder,
    onRemoveFolder,
  }: {
    /** Configured library folders from app config. */
    folders: LibraryFolder[];
    /** path → total/available bytes for that folder's drive (page-maintained). */
    capacity: Record<string, FolderCapacity>;
    /** Prepares + adds a raw path as a library folder. Returns true on success
     *  (so the add panel can close). Page owns canonicalisation + persistence. */
    onAddFolder: (rawPath: string) => Promise<boolean>;
    /** Removes a configured library folder (its installs become "outside"). */
    onRemoveFolder: (path: string) => Promise<void>;
  } = $props();

  const MB = 1048576; // bytes per MB — matches install_size_mb / fmtSize scaling

  // Capacity-bar segment colours. Spool games take the accent; "other on disk"
  // and the free rail are fixed neutral graphite tints (design constants, not
  // theme tokens). A near-full drive tints its free read-out amber.
  const SEG_OTHER = 'rgba(244,244,245,0.22)';
  const SEG_FREE = 'rgba(244,244,245,0.06)';
  const LOW_FREE = 0.12; // free/total below this flags the drive amber

  let games = $state<GameEntry[]>([]);
  const selected = new SvelteSet<string>();
  let showMove = $state(false);
  let busy = $state(false);

  // Generation counter: only the latest loadGames() call writes to `games`.
  // Rapid-fire library:changed events (e.g. N concurrent uninstalls) can cause
  // multiple in-flight api.listGames() calls; the counter ensures earlier
  // responses that arrive after a later one are discarded.
  let loadSeq = 0;

  async function loadGames() {
    const seq = ++loadSeq;
    try {
      const gs = await api.listGames();
      if (seq === loadSeq) games = gs;
    } catch (e) {
      if (seq === loadSeq)
        toasts.show({ kind: 'bad', label: 'LIBRARY', title: "Couldn't load games", sub: String(e) });
    }
  }

  // Load now and reload whenever any window mutates the library.
  onLibraryChanged(loadGames);

  // Only games whose files are actually on disk can be moved or uninstalled.
  const installed = $derived(games.filter((g) => g.installed && g.game_folder_path));

  type Group = {
    /** Stable key for keyed iteration. */
    key: string;
    /** Display label for an "other" group (the parent path). */
    label: string;
    /** The folder path this group represents. */
    path: string;
    /** Optional configured-folder label chip. */
    chip: string | null;
    /** True for a configured library folder, false for an "outside" dir. */
    isFolder: boolean;
    games: GameEntry[];
  };

  // Group installed games: one group per configured library folder, plus a group
  // per stray parent directory for installs that match no configured folder.
  const groups = $derived.by<{ folders: Group[]; other: Group[] }>(() => {
    const folderGroups: Group[] = folders.map((f) => ({
      key: `f:${f.path}`,
      label: f.path,
      path: f.path,
      chip: f.label,
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
      chip: null,
      isFolder: false,
      games: gs,
    }));

    return { folders: folderGroups, other: otherGroups };
  });

  const totalInstalled = $derived(installed.length);
  const selectedGames = $derived(installed.filter((g) => selected.has(g.id)));
  const selectedSize = $derived(selectedGames.reduce((sum, g) => sum + (g.install_size_mb || 0), 0));
  const totalSize = $derived(installed.reduce((sum, g) => sum + (g.install_size_mb || 0), 0));

  function groupBytes(g: Group): number {
    return g.games.reduce((sum, x) => sum + (x.install_size_mb || 0), 0);
  }

  // Largest install in the group — the shared scale every size meter maps onto.
  function groupScale(g: Group): number {
    return g.games.reduce((m, x) => Math.max(m, x.install_size_mb || 0), 1);
  }

  // Games largest-first, so disk hogs land at the top of each drive.
  function sortedGames(g: Group): GameEntry[] {
    return [...g.games].sort((a, b) => (b.install_size_mb || 0) - (a.install_size_mb || 0));
  }

  // Capacity-bar segments for a drive, in MB so they share a unit with
  // install_size_mb / fmtSize. Null when the drive couldn't be measured.
  type Segments = { spool: number; other: number; free: number; total: number; lowFree: boolean };

  // A group plus everything the template needs, computed once when the inputs
  // change rather than re-derived on every render (each selection toggle would
  // otherwise re-sort and re-reduce every panel).
  type DriveGroup = Group & {
    scale: number;
    sorted: GameEntry[];
    bytes: number;
    /** Capacity bar for this drive, or null when unmeasured or when an earlier
     *  folder already owns the bar for the same physical drive. */
    seg: Segments | null;
  };

  // One bar per physical drive. Two library folders on the same drive would each
  // otherwise draw a full-drive bar — double-counting free space and miscounting
  // the sibling's games as generic "other on disk". Instead the first folder
  // group for a drive owns the bar, and its "Spool" segment sums every configured
  // folder on that drive; later same-drive folders show a plain count line.
  const folderGroups = $derived.by<DriveGroup[]>(() => {
    // Total Spool footprint per drive, keyed by mount point.
    const spoolByMount: Record<string, number> = {};
    for (const g of groups.folders) {
      const mount = capacity[g.path]?.mount_point;
      if (mount) spoolByMount[mount] = (spoolByMount[mount] ?? 0) + groupBytes(g);
    }
    const barOwned: Record<string, true> = {};
    return groups.folders.map((g) => {
      const cap = capacity[g.path];
      let seg: Segments | null = null;
      if (cap && cap.total_bytes > 0 && cap.mount_point && !barOwned[cap.mount_point]) {
        barOwned[cap.mount_point] = true;
        const total = cap.total_bytes / MB;
        const free = cap.available_bytes / MB;
        const used = Math.max(0, total - free);
        // Recorded sizes can over-count (stale after files were trimmed on disk),
        // so clamp Spool to the drive's actual used space — it can't exceed it.
        const spool = Math.min(spoolByMount[cap.mount_point] ?? groupBytes(g), used);
        const other = Math.max(0, used - spool);
        seg = { spool, other, free, total, lowFree: free / total < LOW_FREE };
      }
      return { ...g, scale: groupScale(g), sorted: sortedGames(g), bytes: groupBytes(g), seg };
    });
  });

  const otherGroups = $derived.by<DriveGroup[]>(() =>
    groups.other.map((g) => ({
      ...g,
      scale: groupScale(g),
      sorted: sortedGames(g),
      bytes: groupBytes(g),
      seg: null,
    })),
  );

  type CheckState = 'none' | 'some' | 'all';
  function groupState(g: Group): CheckState {
    if (g.games.length === 0) return 'none';
    const n = g.games.filter((x) => selected.has(x.id)).length;
    return n === 0 ? 'none' : n === g.games.length ? 'all' : 'some';
  }

  function toggle(id: string) {
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
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

  // ── Add library folder sub-flow ───────────────────────────────────────────
  let adding = $state(false);
  let proposed = $state('');
  let drives = $state<DriveInfo[]>([]);

  async function startAdd() {
    adding = true;
    proposed = '';
    try {
      drives = await api.listDrives();
    } catch {
      drives = [];
    }
  }

  // Picking a drive seeds an editable `<drive>/Spool` path (auto subfolder).
  function pickDrive(mount: string) {
    const isWin = mount.includes('\\');
    const trimmed = mount.replace(/[\\/]+$/, '');
    proposed = isWin ? `${trimmed}\\Spool` : `${trimmed}/Spool`;
  }

  async function browse() {
    const picked = await openDialog({ title: 'Pick a library folder', directory: true, multiple: false });
    if (typeof picked === 'string') proposed = picked;
  }

  async function confirmAdd() {
    const raw = proposed.trim();
    if (!raw) return;
    if (await onAddFolder(raw)) {
      adding = false;
      proposed = '';
    }
  }

  async function removeFolder(path: string) {
    const ok = await confirmDialog({
      label: 'LIBRARY',
      title: 'Remove this library folder?',
      body: 'Spool stops tracking it as an install root. The games and their files stay on disk — they just move under "Outside library folders".',
      confirmLabel: 'Remove folder',
    });
    if (ok) await onRemoveFolder(path);
  }

  // ── Bulk actions ──────────────────────────────────────────────────────────
  async function deleteSelected() {
    // Snapshot the derived list so stale selected IDs (for games no longer
    // installed) are excluded before confirming and running the uninstalls.
    const ids = selectedGames.map((g) => g.id);
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
    // Uninstalls share a global backup lock — run them one at a time to avoid
    // lock-timeout failures when cloud uploads are slow (180 s timeout).
    const settled: PromiseSettledResult<unknown>[] = [];
    for (const id of ids) {
      settled.push(
        await api
          .uninstallGame(id)
          .then(() => ({ status: 'fulfilled' as const, value: undefined }))
          .catch((reason) => ({ status: 'rejected' as const, reason })),
      );
    }
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

  // % of total for a capacity segment, clamped so an over-counted segment can't
  // overflow the bar and swallow the free rail (segments are already clamped to
  // the drive's used space, but keep the bound as defence).
  function pct(mb: number, total: number): string {
    return `${Math.min(100, Math.max(0, (mb / total) * 100))}%`;
  }
  // Proportional width of a per-game size meter, floored so tiny games still show.
  function meterPct(mb: number, scale: number): string {
    return `${Math.max(2, Math.min(100, (mb / scale) * 100))}%`;
  }
</script>

<!-- ── group count + size summary (one phrasing everywhere) ── -->
{#snippet groupSummary(count: number, bytes: number)}
  {count} {count === 1 ? 'game' : 'games'} · <span class="font-mono text-ink-2">{fmtSize(bytes)}</span>
{/snippet}

<!-- ── one installed-game row ── -->
{#snippet gameRow(g: GameEntry, scale: number)}
  {@const checked = selected.has(g.id)}
  <div
    class="lt-row flex h-[38px] cursor-pointer items-center gap-3 border-b border-line-1 px-[14px] last:border-b-0"
    class:lt-on={checked}
    style:background={checked ? 'color-mix(in srgb, var(--color-spool) 8%, transparent)' : 'transparent'}
    onclick={() => toggle(g.id)}
    role="button"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === ' ' || e.key === 'Enter') {
        e.preventDefault();
        toggle(g.id);
      }
    }}
  >
    <Checkbox state={checked ? 'all' : 'none'} onToggle={() => toggle(g.id)} label={`Select ${g.game_name}`} />
    <span class="min-w-0 flex-1 truncate text-[13px] text-ink-0">{g.game_name}</span>
    <span class="shrink-0 font-mono text-[10px] text-ink-3">{fmtCatalog(g.catalog_number)}</span>
    <!-- proportional size meter, shared scale per drive -->
    <div class="flex shrink-0 items-center" style:width="88px">
      <div class="h-1 flex-1 overflow-hidden rounded-[2px]" style:background="rgba(255,255,255,0.06)">
        <div
          class="h-full rounded-[2px]"
          style:width={meterPct(g.install_size_mb, scale)}
          style:background="var(--color-spool)"
          style:opacity="0.85"
        ></div>
      </div>
    </div>
    <span class="w-[70px] shrink-0 text-right font-mono text-[12px] text-ink-1">{fmtSize(g.install_size_mb)}</span>
  </div>
{/snippet}

<div class="flex flex-col gap-[14px]">

  <!-- ── storage toolbar ── -->
  <div class="flex items-center gap-3 pb-0.5">
    <div class="flex flex-col gap-px">
      <span class="text-[13.5px] font-semibold text-ink-0">
        {totalInstalled} {totalInstalled === 1 ? 'game' : 'games'} installed
      </span>
      <span class="text-[11.5px] text-ink-3">
        <span class="font-mono text-ink-2">{fmtSize(totalSize)}</span>
        across {folders.length} library {folders.length === 1 ? 'folder' : 'folders'}
      </span>
    </div>
    <div class="flex-1"></div>
    <Btn variant="ghost" onclick={startAdd}>
      {#snippet icon()}<Plus size={14} />{/snippet}
      Add library folder
    </Btn>
  </div>

  <!-- ── add-folder panel ── -->
  {#if adding}
    <div class="flex flex-col gap-2 rounded-md border border-line-1 bg-bg-1 p-3">
      {#if drives.length > 0}
        <div class="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-3">Detected drives</div>
        <div class="flex flex-col gap-1">
          {#each drives as d (d.mount_point)}
            <button
              type="button"
              class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-left hover:bg-bg-2"
              onclick={() => pickDrive(d.mount_point)}
            >
              <HardDrive size={13} class="shrink-0 text-ink-3" />
              <span class="flex-1 truncate font-mono text-[12px] text-ink-1">{d.mount_point}</span>
              <span class="text-[11px] text-ink-3">{fmtSize(d.available_bytes / MB)} free</span>
            </button>
          {/each}
        </div>
      {/if}
      <div class="flex min-w-0 items-center gap-2">
        <TextField bind:value={proposed} placeholder="Pick a drive above, or browse…" mono full />
        <Btn variant="ghost" onclick={browse}>
          {#snippet icon()}<Folder size={14} />{/snippet}
          Browse
        </Btn>
      </div>
      <div class="flex items-center justify-end gap-2">
        <Btn variant="ghost" onclick={() => { adding = false; proposed = ''; }}>Cancel</Btn>
        <Btn variant="primary" disabled={!proposed.trim()} onclick={confirmAdd}>Add folder</Btn>
      </div>
    </div>
  {/if}

  {#if folders.length === 0 && !adding}
    <div class="rounded-md border border-dashed border-line-2 bg-bg-1 px-[16px] py-[18px] text-[12.5px] text-ink-3">
      No library folders yet. Add one per drive to see its capacity and move installs there.
    </div>
  {/if}

  <!-- ── one drive panel per configured library folder ── -->
  {#each folderGroups as grp (grp.key)}
    {@const seg = grp.seg}
    {@const scale = grp.scale}
    {@const state = groupState(grp)}
    <div class="overflow-hidden rounded-md border border-line-1 bg-bg-1">
      <!-- drive header -->
      <div class="border-b border-dashed border-line-1 bg-bg-2 px-[16px] pb-3 pt-[13px]">
        <div class="flex items-center gap-[11px]">
          <Checkbox {state} onToggle={() => toggleGroup(grp)} label={`Select all in ${grp.label}`} />
          <span class="flex text-ink-2"><HardDrive size={14} /></span>
          <span class="min-w-0 truncate font-mono text-[13px] text-ink-0" title={grp.path}>{grp.path}</span>
          {#if grp.chip}
            <span class="shrink-0 rounded-[3px] border border-line-2 px-1.5 py-px font-mono text-[9px] uppercase tracking-[0.1em] text-ink-3">{grp.chip}</span>
          {/if}
          <div class="flex-1"></div>
          {#if seg}
            <span class="text-[12px]" style:color={seg.lowFree ? 'var(--color-warn)' : 'var(--color-ink-2)'}>
              <span class="font-mono" style:color={seg.lowFree ? 'var(--color-warn)' : 'var(--color-ink-0)'}>{fmtSize(seg.free)}</span> free
            </span>
          {/if}
          <button
            type="button"
            onclick={() => removeFolder(grp.path)}
            title="Remove this library folder"
            aria-label={`Remove library folder ${grp.path}`}
            class="flex size-6 shrink-0 items-center justify-center rounded-sm text-ink-3 transition-colors hover:bg-white/[0.06] hover:text-ink-1"
          >
            <Trash2 size={13} />
          </button>
        </div>

        {#if seg}
          <!-- capacity bar: Spool games · other on disk · free -->
          <div class="mt-[11px]">
            <div
              class="flex h-2 w-full overflow-hidden rounded-[4px] border border-line-1"
              style:background={SEG_FREE}
            >
              <div class="h-full" style:width={pct(seg.spool, seg.total)} style:background="var(--color-spool)" title="Spool games"></div>
              <div
                class="h-full"
                style:width={pct(seg.other, seg.total)}
                style:background={SEG_OTHER}
                style:border-left="1px solid var(--color-bg-1)"
                title="Other on disk"
              ></div>
              <div class="h-full flex-1" style:background={seg.lowFree ? 'rgba(244,182,108,0.10)' : 'transparent'} title="Free"></div>
            </div>
            <!-- legend + group summary -->
            <div class="mt-[9px] flex items-center justify-between gap-4">
              <div class="flex flex-wrap items-center gap-4">
                <span class="inline-flex items-center gap-1.5">
                  <span class="size-[7px] rounded-[2px]" style:background="var(--color-spool)"></span>
                  <span class="text-[11px] text-ink-2">Spool</span>
                  <span class="font-mono text-[11px] text-ink-1">{fmtSize(seg.spool)}</span>
                </span>
                <span class="inline-flex items-center gap-1.5">
                  <span class="size-[7px] rounded-[2px]" style:background={SEG_OTHER}></span>
                  <span class="text-[11px] text-ink-2">Other</span>
                  <span class="font-mono text-[11px] text-ink-1">{fmtSize(seg.other)}</span>
                </span>
                <span class="inline-flex items-center gap-1.5">
                  <span class="size-[7px] rounded-[2px]" style:background={seg.lowFree ? 'rgba(244,182,108,0.5)' : 'rgba(244,244,245,0.18)'}></span>
                  <span class="text-[11px]" style:color={seg.lowFree ? 'var(--color-warn)' : 'var(--color-ink-2)'}>Free</span>
                  <span class="font-mono text-[11px]" style:color={seg.lowFree ? 'var(--color-warn)' : 'var(--color-ink-1)'}>{fmtSize(seg.free)}</span>
                </span>
              </div>
              <span class="shrink-0 text-[11px] text-ink-3">
                {@render groupSummary(grp.games.length, grp.bytes)}
              </span>
            </div>
          </div>
        {:else}
          <!-- drive couldn't be measured (or another folder owns its bar) — fall
               back to a plain game-count line -->
          <div class="mt-2 text-[11px] text-ink-3">
            {@render groupSummary(grp.games.length, grp.bytes)}
          </div>
        {/if}
      </div>

      <!-- games, largest first -->
      {#if grp.games.length === 0}
        <div class="px-[16px] py-[12px] text-[11.5px] text-ink-3">No games installed here yet.</div>
      {:else}
        {#each grp.sorted as g (g.id)}
          {@render gameRow(g, scale)}
        {/each}
      {/if}
    </div>
  {/each}

  <!-- ── stray installs outside any configured folder ── -->
  {#each otherGroups as grp (grp.key)}
    {@const scale = grp.scale}
    {@const state = groupState(grp)}
    <div class="overflow-hidden rounded-md border border-dashed border-line-2 bg-bg-1">
      <div class="flex items-center gap-[11px] border-b border-dashed border-line-1 bg-white/[0.015] px-[16px] py-[11px]">
        <Checkbox {state} onToggle={() => toggleGroup(grp)} label={`Select all in ${grp.label}`} />
        <span class="flex text-ink-3"><FolderTree size={14} /></span>
        <div class="min-w-0 flex-1">
          <div class="text-[12.5px] text-ink-1">Outside library folders</div>
          <div class="truncate font-mono text-[10.5px] text-ink-3" title={grp.path}>{grp.path}</div>
        </div>
        <span class="shrink-0 text-[11px] text-ink-3">
          {@render groupSummary(grp.games.length, grp.bytes)}
        </span>
      </div>
      {#each grp.sorted as g (g.id)}
        {@render gameRow(g, scale)}
      {/each}
    </div>
  {/each}

  <!-- ── sticky bulk-action bar ── -->
  {#if selectedGames.length > 0}
    <div
      class="sticky bottom-0 z-[5] mt-[14px] flex items-center gap-3 rounded-md border border-line-2 bg-bg-2 px-[16px] py-[11px]"
      style:box-shadow="0 8px 30px rgba(0,0,0,0.45)"
    >
      <span
        class="inline-flex h-[22px] min-w-[22px] items-center justify-center rounded-sm px-[7px] font-mono text-[11px] font-semibold"
        style:background="var(--color-spool)"
        style:color="var(--color-bg-0)"
      >{selectedGames.length}</span>
      <span class="text-[12.5px] text-ink-1">
        selected <span class="text-ink-3">·</span>
        <span class="font-mono text-ink-0">{fmtSize(selectedSize)}</span>
      </span>
      <div class="flex-1"></div>
      <Btn variant="ghost" onclick={clearSelection}>Clear</Btn>
      <Btn variant="ghost" disabled={busy} onclick={openMove}>
        {#snippet icon()}<FolderInput size={14} />{/snippet}
        Move…
      </Btn>
      <Btn variant="danger" disabled={busy} onclick={deleteSelected}>
        {#snippet icon()}<Trash2 size={14} />{/snippet}
        Delete from disk
      </Btn>
    </div>
  {/if}
</div>

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

<style>
  /* Subtle row-hover affordance — rows still feel clickable, but selected
     (lt-on) rows keep their accent tint rather than washing out on hover. */
  .lt-row:not(.lt-on):hover {
    background: rgba(255, 255, 255, 0.025) !important;
  }
</style>
