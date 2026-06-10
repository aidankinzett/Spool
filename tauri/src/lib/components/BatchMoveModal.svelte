<script lang="ts">
  /**
   * Batch move-install chooser — relocate several games' install folders into a
   * single chosen library folder. Lists the configured library folders as
   * destinations with live free space; the free-space check is against the sum
   * of the selected games' sizes (minus any already in that folder).
   *
   * The backend allows only one move at a time (single move slot), so the queue
   * runs sequentially: each `moveGameInstall` is awaited before the next starts,
   * and a single `move:progress` listener drives the active game's bar. Games
   * already living in the chosen folder are skipped, not moved. A per-game error
   * is recorded and the batch continues; Cancel aborts the in-flight game (its
   * source is left intact) and stops the queue.
   *
   * Modelled on MoveInstallModal, extended for N games + a combined progress bar.
   */
  import { onDestroy } from 'svelte';
  import { FolderInput, HardDrive, Check, X } from '@lucide/svelte';
  import { listen } from '@tauri-apps/api/event';
  import { api } from '$lib/api';
  import { fmtSize } from '$lib/format';
  import { shadeHex } from '$lib/tokens';
  import { isCurrentRoot, neededBytes } from '$lib/pathMatch';
  import type { GameEntry, LibraryFolder, MoveProgress } from '$lib/types';
  import ModalShell from '$lib/components/ModalShell.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    games,
    folders,
    onClose,
    onDone,
  }: {
    /** The games whose installs are being moved. */
    games: GameEntry[];
    /** Configured library folders (destinations) from app config. */
    folders: LibraryFolder[];
    /** Dismiss without moving (Cancel / Escape / close / scrim). */
    onClose: () => void;
    /** Run after the batch settles (e.g. clear the selection). */
    onDone?: () => void;
  } = $props();

  const acc = BRAND_SPOOL;

  function sizeBytesOf(g: GameEntry): number {
    return Math.round((g.install_size_mb || 0) * 1048576);
  }

  type FolderRow = {
    path: string;
    label: string | null;
    free: number;
    /** How many of the selected games already live in this folder. */
    alreadyHere: number;
    /** Bytes that would actually move into this folder (excludes already-here). */
    bytesToMove: number;
    tooSmall: boolean;
    /** Every selected game already lives here — nothing to do. */
    allHere: boolean;
    /** True when the free-space query failed (path inaccessible / unmounted). */
    freeError: boolean;
  };
  let rows = $state<FolderRow[]>([]);
  let selected = $state<string | null>(null);
  let hover = $state<Record<string, boolean>>({});

  async function loadRows() {
    const out = await Promise.all(
      folders.map(async (f): Promise<FolderRow> => {
        let free = 0;
        let freeError = false;
        try {
          free = await api.folderFreeSpace(f.path);
        } catch {
          freeError = true;
        }
        const here = games.filter((g) => isCurrentRoot(f.path, g.game_folder_path));
        const bytesToMove = games
          .filter((g) => !isCurrentRoot(f.path, g.game_folder_path))
          .reduce((sum, g) => sum + sizeBytesOf(g), 0);
        return {
          path: f.path,
          label: f.label,
          free,
          alreadyHere: here.length,
          bytesToMove,
          tooSmall: !freeError && free > 0 && bytesToMove > 0 && free < neededBytes(bytesToMove),
          allHere: here.length === games.length,
          freeError,
        };
      }),
    );
    rows = out;
    const usable = (r: FolderRow) => !r.tooSmall && !r.allHere && !r.freeError;
    const stillValid = out.some((r) => r.path === selected && usable(r));
    if (!stillValid) selected = out.find(usable)?.path ?? null;
  }

  type Phase = 'choose' | 'moving' | 'done' | 'error';
  let phase = $state<Phase>('choose');

  // Per-game outcomes, in queue order.
  type Outcome = 'pending' | 'moving' | 'done' | 'skipped' | 'error' | 'canceled';
  let results = $state<{ id: string; name: string; status: Outcome; message?: string }[]>([]);
  let currentIndex = $state(0);
  let queueLen = $state(0);
  let progress = $state<MoveProgress | null>(null);
  let activeId = $state<string | null>(null);
  let canceled = $state(false);
  let errorMsg = $state('');
  let unlisten: (() => void) | null = null;
  let closeTimer: ReturnType<typeof setTimeout> | null = null;

  // Refresh destination rows only while choosing, never mid-move.
  $effect(() => {
    if (phase === 'choose') void loadRows();
  });

  const locked = $derived(phase === 'moving');
  const totalSize = $derived(games.reduce((sum, g) => sum + sizeBytesOf(g), 0));

  // Fraction [0,1] of the active game, from its byte stream (rename fast-path
  // reports no total, so it reads 0 until the game completes — acceptable).
  const activeFraction = $derived(
    progress && progress.total_bytes > 0
      ? Math.min(1, progress.copied_bytes / progress.total_bytes)
      : 0,
  );
  const completedCount = $derived(
    results.filter((r) => r.status === 'done' || r.status === 'skipped' || r.status === 'error' || r.status === 'canceled')
      .length,
  );
  // Combined progress across the whole batch. Denominator is results.length
  // (queue + skipped) so pre-seeded skipped entries don't inflate the fraction.
  const pct = $derived(
    phase === 'done'
      ? 100
      : results.length === 0
        ? 0
        : Math.min(100, Math.round(((completedCount + activeFraction) / results.length) * 100)),
  );

  const summary = $derived.by(() => {
    const moved = results.filter((r) => r.status === 'done').length;
    const skipped = results.filter((r) => r.status === 'skipped').length;
    const failed = results.filter((r) => r.status === 'error').length;
    const cancels = results.filter((r) => r.status === 'canceled').length;
    return { moved, skipped, failed, cancels };
  });

  function pick(path: string) {
    if (locked) return;
    const row = rows.find((r) => r.path === path);
    if (!row || row.tooSmall || row.allHere || row.freeError) return;
    selected = path;
  }

  async function confirm() {
    if (!selected || locked) return;
    const dest = selected;
    // Skip games already living in the destination; only the rest are queued.
    const queue = games.filter((g) => !isCurrentRoot(dest, g.game_folder_path));
    const skippedGames = games.filter((g) => isCurrentRoot(dest, g.game_folder_path));

    results = [
      ...queue.map((g) => ({ id: g.id, name: g.game_name, status: 'pending' as Outcome })),
      ...skippedGames.map((g) => ({ id: g.id, name: g.game_name, status: 'skipped' as Outcome })),
    ];
    queueLen = queue.length;
    currentIndex = 0;
    canceled = false;
    errorMsg = '';

    if (queue.length === 0) {
      // Nothing to move — everything already there.
      phase = 'done';
      onDone?.();
      closeTimer = setTimeout(() => onClose(), 900);
      return;
    }

    phase = 'moving';
    try {
      unlisten = await listen<MoveProgress>('move:progress', (e) => {
        if (e.payload.game_id === activeId) progress = e.payload;
      });

      for (let i = 0; i < queue.length; i++) {
        if (canceled) {
          // Remaining (still-pending) games are marked canceled.
          results = results.map((r) => (r.status === 'pending' ? { ...r, status: 'canceled' } : r));
          break;
        }
        const g = queue[i];
        currentIndex = i;
        activeId = g.id;
        progress = null;
        setStatus(g.id, 'moving');
        try {
          await api.moveGameInstall(g.id, dest);
          setStatus(g.id, 'done');
        } catch (err) {
          // `canceled` is set before api.cancelMove() is called, so any
          // rejection that follows a user cancel is treated as canceled, not
          // as a failure (source files are left intact in both cases).
          if (canceled) {
            setStatus(g.id, 'canceled');
          } else {
            setStatus(g.id, 'error', String(err));
          }
          // Independent games: record and keep going.
        }
      }
      phase = 'done';
      // Only clear the caller's selection when everything succeeded or was
      // skipped — leave it intact so the user can see and retry any failures.
      const hadProblems = results.some((r) => r.status === 'error' || r.status === 'canceled');
      if (!hadProblems) onDone?.();
      closeTimer = setTimeout(() => onClose(), 1100);
    } catch (e) {
      errorMsg = String(e);
      phase = 'error';
    } finally {
      activeId = null;
      unlisten?.();
      unlisten = null;
    }
  }

  function setStatus(id: string, status: Outcome, message?: string) {
    results = results.map((r) => (r.id === id ? { ...r, status, message } : r));
  }

  async function cancelOrClose() {
    if (phase === 'moving') {
      canceled = true;
      if (activeId) await api.cancelMove(activeId);
    } else {
      onClose();
    }
  }

  onDestroy(() => {
    unlisten?.();
    if (closeTimer) clearTimeout(closeTimer);
  });

  function rowSubtitle(r: FolderRow): string {
    if (r.allHere) return 'All selected games are already here';
    if (r.freeError) return 'Could not read free space — check the drive is accessible';
    if (r.tooSmall) return `Not enough space — ${fmtSize(r.free / 1048576)} free`;
    const base = `${fmtSize(r.free / 1048576)} free`;
    return r.alreadyHere > 0 ? `${base} · ${r.alreadyHere} already here` : base;
  }
</script>

{#snippet folderCard(r: FolderRow)}
  {@const active = selected === r.path}
  {@const disabled = r.tooSmall || r.allHere || r.freeError || locked}
  {@const borderCol = disabled
    ? 'var(--color-line-1)'
    : active
      ? acc
      : hover[r.path]
        ? 'var(--color-line-3)'
        : 'var(--color-line-2)'}
  {@const bg = active ? `${acc}12` : hover[r.path] && !disabled ? 'var(--color-bg-2)' : 'var(--color-bg-1)'}
  <button
    type="button"
    onclick={() => pick(r.path)}
    data-gp-autofocus={active ? '' : undefined}
    onmouseenter={() => (hover[r.path] = true)}
    onmouseleave={() => (hover[r.path] = false)}
    {disabled}
    class="flex w-full items-center gap-3 rounded-md text-left transition-[background,border-color,opacity] duration-150"
    style:padding="11px 13px"
    style:background={bg}
    style:border="1px solid {borderCol}"
    style:opacity={r.tooSmall || r.allHere || r.freeError ? 0.55 : 1}
    style:cursor={disabled ? 'default' : 'pointer'}
    style:box-shadow={active ? `0 0 0 1px ${acc}66, 0 8px 26px ${acc}1f` : 'none'}
  >
    <span
      class="inline-flex shrink-0 items-center justify-center rounded-sm"
      style:width="30px"
      style:height="30px"
      style:background={active ? `${acc}22` : 'var(--color-bg-3)'}
      style:color={active ? acc : 'var(--color-ink-1)'}
    >
      <HardDrive size={15} />
    </span>
    <div class="min-w-0 flex-1">
      <div class="truncate font-mono text-[12.5px] text-ink-0">{r.label || r.path}</div>
      <div class="text-[11.5px]" style:color={r.tooSmall ? 'var(--color-bad)' : 'var(--color-ink-2)'}>
        {rowSubtitle(r)}
      </div>
    </div>
    {#if active}<Check size={15} style="color: {acc}" />{/if}
  </button>
{/snippet}

<ModalShell
  breadcrumb="MOVE · {games.length} GAMES"
  breadcrumbColor="var(--color-ink-2)"
  accent={acc}
  width="540px"
  closeDisabled={locked}
  {onClose}
  ariaLabelledBy="bmv-modal-title"
>
  <!-- hero -->
  <div class="flex items-start gap-[18px]" style:padding="20px 24px 18px" style:border-bottom="1px solid var(--color-line-1)">
    <div class="min-w-0 flex-1">
      <h1
        id="bmv-modal-title"
        class="font-display"
        style:margin="0"
        style:font-size="24px"
        style:font-weight="700"
        style:letter-spacing="-0.02em"
        style:line-height="1.05"
      >
        Move {games.length} {games.length === 1 ? 'game' : 'games'}
      </h1>
      <div
        class="mt-2 flex items-center gap-1.5 truncate font-mono"
        style:font-size="10.5px"
        style:color="var(--color-ink-3)"
      >
        <HardDrive size={11} class="shrink-0" />
        <span class="truncate">Pick one destination for the whole selection</span>
        <span class="shrink-0" style:color="var(--color-ink-2)">· {fmtSize(totalSize / 1048576)}</span>
      </div>
    </div>
  </div>

  <!-- body -->
  <div class="flex flex-col" style:padding="18px 24px 16px" style:gap="10px">
    {#if folders.length === 0}
      <p style:font-size="13px" style:color="var(--color-ink-2)" style:line-height="1.5">
        No library folders are set up yet. Add one per drive in
        <strong class="font-semibold text-ink-1">Settings → Library</strong>, then move the games there.
      </p>
    {:else if phase === 'choose'}
      {#each rows as r (r.path)}
        {@render folderCard(r)}
      {/each}
    {:else}
      <!-- Per-game progress list -->
      <div class="flex flex-col gap-1 max-h-[240px] overflow-y-auto">
        {#each results as r (r.id)}
          {@const isActive = phase === 'moving' && r.status === 'moving'}
          <div
            class="flex items-center gap-2.5 rounded-sm"
            style:padding="7px 10px"
            style:background={isActive ? `${acc}10` : 'transparent'}
          >
            <span class="shrink-0" style:width="14px" style:height="14px">
              {#if r.status === 'done'}
                <Check size={14} class="text-ok" />
              {:else if r.status === 'error'}
                <X size={14} class="text-bad" />
              {:else if r.status === 'moving'}
                <span class="block size-[8px] mt-[3px] rounded-full animate-pulse" style:background={acc}></span>
              {/if}
            </span>
            <span class="min-w-0 flex-1 truncate text-[12.5px]" style:color={r.status === 'pending' ? 'var(--color-ink-3)' : 'var(--color-ink-1)'}>
              {r.name}
            </span>
            <span class="shrink-0 font-mono text-[10.5px]" style:color="var(--color-ink-3)">
              {#if r.status === 'skipped'}already here{:else if r.status === 'error'}failed{:else if r.status === 'canceled'}canceled{:else if r.status === 'done'}moved{:else if r.status === 'moving'}moving…{/if}
            </span>
          </div>
          {#if r.status === 'error' && r.message}
            <div class="ml-[24px] -mt-0.5 mb-1 text-[10.5px] leading-snug text-bad/90">{r.message}</div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>

  <!-- footer -->
  <div style:padding="16px 24px 22px" style:border-top="1px solid var(--color-line-1)" style:background="rgba(0,0,0,0.18)">
    {#if phase === 'error'}
      <div
        class="mb-3 flex items-start gap-2 rounded-sm"
        style:padding="9px 12px"
        style:border="1px solid color-mix(in srgb, var(--color-bad) 28%, transparent)"
        style:background="color-mix(in srgb, var(--color-bad) 8%, transparent)"
      >
        <X size={14} class="mt-px shrink-0 text-bad" />
        <span class="flex-1" style:font-size="12px" style:color="var(--color-ink-1)" style:line-height="1.4">
          <strong class="font-semibold text-bad">Couldn’t move.</strong>
          {errorMsg}
        </span>
      </div>
    {:else if phase === 'moving' || phase === 'done'}
      <div class="mb-3">
        <div class="mb-1.5 flex items-center justify-between" style:font-size="12px" style:color="var(--color-ink-2)">
          <span>
            {#if phase === 'done'}
              Moved {summary.moved} · Skipped {summary.skipped}{summary.failed ? ` · Failed ${summary.failed}` : ''}{summary.cancels ? ` · Canceled ${summary.cancels}` : ''}
            {:else}
              Moving {Math.min(currentIndex + 1, queueLen)} of {queueLen}…
            {/if}
          </span>
          <span class="font-mono">{pct}%</span>
        </div>
        <div class="h-1.5 w-full overflow-hidden rounded-full" style:background="var(--color-bg-3)">
          <div
            class="h-full rounded-full transition-[width] duration-200"
            style:width="{pct}%"
            style:background={acc}
          ></div>
        </div>
      </div>
    {/if}

    <div class="flex items-center gap-2.5">
      <div class="flex-1"></div>
      <button
        type="button"
        onclick={cancelOrClose}
        class="inline-flex items-center justify-center whitespace-nowrap rounded-sm font-medium transition-colors duration-100"
        style:height="34px"
        style:padding-inline="14px"
        style:font-size="13px"
        style:color="var(--color-ink-2)"
        style:border="1px solid var(--color-line-1)"
        style:cursor="pointer"
        style:background={hover['cancel'] ? 'rgb(255 255 255 / 0.06)' : 'transparent'}
        onmouseenter={() => (hover['cancel'] = true)}
        onmouseleave={() => (hover['cancel'] = false)}
      >
        {phase === 'moving' ? 'Cancel move' : 'Close'}
      </button>
      <button
        type="button"
        onclick={confirm}
        disabled={locked || phase === 'done' || !selected || folders.length === 0}
        class="inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-sm font-medium transition-colors duration-100 disabled:opacity-50"
        style:height="34px"
        style:min-width="160px"
        style:padding-inline="14px"
        style:font-size="13px"
        style:color="#0b0c0e"
        style:border="1px solid transparent"
        style:cursor={locked || phase === 'done' || !selected ? 'default' : 'pointer'}
        style:background={hover['confirm'] && !locked && selected ? shadeHex(acc, -10) : acc}
        onmouseenter={() => (hover['confirm'] = true)}
        onmouseleave={() => (hover['confirm'] = false)}
      >
        {#if phase !== 'moving'}<FolderInput size={14} />{/if}
        {phase === 'moving' ? 'Moving…' : phase === 'done' ? 'Done' : `Move ${games.length} here`}
      </button>
    </div>
  </div>
</ModalShell>
