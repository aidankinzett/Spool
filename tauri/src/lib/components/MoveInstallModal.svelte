<script lang="ts">
  /**
   * Move-install chooser — relocate a game's install folder to another library
   * folder (drive). Lists the configured library folders as destinations, each
   * with live free space; folders too small for the install, or the one the game
   * already lives in, are disabled.
   *
   * Self-driven over a small state machine (choose → moving → done / error). The
   * move itself streams `move:progress` events from the backend, which drive the
   * progress bar; Cancel during a move asks the backend to abort (it cleans up
   * its staging dir and leaves the original untouched). Mirrors the design
   * language of RemoveGameModal / CloudConflictModal.
   */
  import { onDestroy } from 'svelte';
  import { FolderInput, HardDrive, Check, X } from '@lucide/svelte';
  import { listen } from '@tauri-apps/api/event';
  import { api } from '$lib/api';
  import { fmtSize } from '$lib/format';
  import { shadeHex } from '$lib/tokens';
  import { isCurrentRoot as isCurrentRootOf, neededBytes } from '$lib/pathMatch';
  import type { GameEntry, LibraryFolder, MoveProgress } from '$lib/types';
  import ModalShell from '$lib/components/ModalShell.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    game,
    folders,
    onClose,
    onDone,
    importMode = false,
    renameTo = '',
    showDontAskAgain = false,
    onDontAskAgain,
  }: {
    /** The game whose install folder is being moved. */
    game: GameEntry;
    /** Configured library folders (destinations) from app config. */
    folders: LibraryFolder[];
    /** Dismiss without moving (Cancel / Escape / close / scrim). */
    onClose: () => void;
    /** Run after a successful move (e.g. close the edit window). */
    onDone?: () => void;
    /** Whether the modal is in post-add import mode. */
    importMode?: boolean;
    /** The name of the game to rename the folder to (backend sanitizes). */
    renameTo?: string;
    /** Whether to show the "Don't ask again" action. */
    showDontAskAgain?: boolean;
    /** Callback when the user clicks "Don't ask again". */
    onDontAskAgain?: () => void | Promise<void>;
  } = $props();

  const acc = $derived(game.accent_color ?? BRAND_SPOOL);
  const sizeBytes = $derived(Math.round((game.install_size_mb || 0) * 1048576));
  const currentFolder = $derived(game.game_folder_path ?? '');

  type FolderRow = {
    path: string;
    label: string | null;
    free: number;
    tooSmall: boolean;
    isCurrent: boolean;
  };
  let rows = $state<FolderRow[]>([]);
  let selected = $state<string | null>(null);
  let hover = $state<Record<string, boolean>>({});

  // The install lives at `<root>/<game folder>`, so a library folder is the
  // current location when it's the parent of the game's folder. Path-folding and
  // the free-space headroom rule live in `$lib/pathMatch` (shared with the batch
  // move + the Settings library view).
  function isCurrentRoot(root: string): boolean {
    return isCurrentRootOf(root, currentFolder);
  }

  async function loadRows() {
    // Probe every folder's free space in parallel rather than one await at a time.
    const out = await Promise.all(
      folders.map(async (f): Promise<FolderRow> => {
        let free: number;
        try {
          free = await api.folderFreeSpace(f.path);
        } catch {
          free = 0;
        }
        return {
          path: f.path,
          label: f.label,
          free,
          tooSmall: free > 0 && sizeBytes > 0 && free < neededBytes(sizeBytes),
          isCurrent: importMode ? false : isCurrentRoot(f.path),
        };
      }),
    );
    rows = out;
    // Keep the user's selection if it's still usable; otherwise pre-select the
    // first usable destination.
    const stillValid = out.some((r) => r.path === selected && !r.isCurrent && !r.tooSmall);
    if (!stillValid) {
      selected = out.find((r) => !r.isCurrent && !r.tooSmall)?.path ?? null;
    }
  }

  type Phase = 'choose' | 'moving' | 'done' | 'error';
  let phase = $state<Phase>('choose');
  let progress = $state<MoveProgress | null>(null);
  let errorMsg = $state('');
  let canceled = $state(false);
  let unlisten: (() => void) | null = null;
  let closeTimer: ReturnType<typeof setTimeout> | null = null;

  // Refresh the destination rows only while choosing — never mid-move, so a
  // `library:changed`-triggered folders refresh can't reset the selection or
  // rows out from under an in-flight transfer.
  $effect(() => {
    if (phase === 'choose') void loadRows();
  });

  const locked = $derived(phase === 'moving');
  // 100 on 'done' regardless of the event stream — the fast-path rename never
  // learns a byte total, so the ratio alone would read 0% at completion.
  const pct = $derived(
    phase === 'done'
      ? 100
      : progress && progress.total_bytes > 0
        ? Math.min(100, Math.round((progress.copied_bytes / progress.total_bytes) * 100))
        : 0,
  );

  function pick(path: string) {
    if (locked) return;
    const row = rows.find((r) => r.path === path);
    if (!row || row.isCurrent || row.tooSmall) return;
    selected = path;
  }

  async function confirm() {
    if (!selected || locked) return;
    phase = 'moving';
    progress = null;
    errorMsg = '';
    canceled = false;
    try {
      // Registered inside the try so a listen() failure routes through the catch
      // below rather than leaving the modal stuck on 'moving'.
      unlisten = await listen<MoveProgress>('move:progress', (e) => {
        if (e.payload.game_id === game.id) progress = e.payload;
      });
      await api.moveGameInstall(game.id, selected, importMode && renameTo ? renameTo : undefined);
      phase = 'done';
      onDone?.();
      // Brief beat on the "done" state so the bar reads 100% before close.
      closeTimer = setTimeout(() => onClose(), 700);
    } catch (e) {
      // A user-requested cancel rejects the move promise with the backend's
      // Canceled error ("install cancelled") — close cleanly rather than
      // presenting it as a failure (the source is left intact). Any other
      // rejection after a cancel click is a real failure that raced the
      // cancel; surface it instead of swallowing it.
      if (canceled && /install cancelled/i.test(String(e))) {
        onClose();
      } else {
        errorMsg = String(e);
        phase = 'error';
      }
    } finally {
      unlisten?.();
      unlisten = null;
    }
  }

  async function cancelOrClose() {
    if (phase === 'moving') {
      canceled = true;
      await api.cancelMove(game.id);
    } else {
      onClose();
    }
  }

  onDestroy(() => {
    unlisten?.();
    // Clear the delayed close so it can't fire after the modal is gone and close
    // a freshly opened one.
    if (closeTimer) clearTimeout(closeTimer);
  });

  function rowSubtitle(r: FolderRow): string {
    if (r.isCurrent) return 'Current location';
    if (r.tooSmall) return `Not enough space — ${fmtSize(r.free / 1048576)} free`;
    return `${fmtSize(r.free / 1048576)} free`;
  }
</script>

{#snippet folderCard(r: FolderRow)}
  {@const active = selected === r.path}
  {@const disabled = r.isCurrent || r.tooSmall || locked}
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
    style:opacity={r.isCurrent || r.tooSmall ? 0.55 : 1}
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
      <div class="truncate font-mono text-[12.5px] text-ink-0">
        {importMode && renameTo ? `${r.label || r.path}/${renameTo}` : (r.label || r.path)}
      </div>
      <div class="text-[11.5px]" style:color={r.tooSmall ? 'var(--color-bad)' : 'var(--color-ink-2)'}>
        {rowSubtitle(r)}
      </div>
    </div>
    {#if active}<Check size={15} style="color: {acc}" />{/if}
  </button>
{/snippet}

<ModalShell
  breadcrumb="MOVE · INSTALL"
  breadcrumbColor="var(--color-ink-2)"
  accent={game.accent_color}
  width="540px"
  closeDisabled={locked}
  {onClose}
  ariaLabelledBy="mv-modal-title"
>
  <!-- hero -->
  <div class="flex items-start gap-[18px]" style:padding="20px 24px 18px" style:border-bottom="1px solid var(--color-line-1)">
    <div class="min-w-0 flex-1">
      <h1
        id="mv-modal-title"
        class="font-display"
        style:margin="0"
        style:font-size="24px"
        style:font-weight="700"
        style:letter-spacing="-0.02em"
        style:line-height="1.05"
      >
        {importMode ? 'Import game' : 'Move install'}
      </h1>
      {#if importMode}
        <div style:margin-top="8px" style:font-size="13px" style:color="var(--color-ink-1)" style:font-weight="500" style:line-height="1.4">
          This game is stored outside your library folders. Move it in and rename the folder to <strong>{game.game_name}</strong>?
        </div>
      {:else}
        <div style:margin-top="6px" style:font-size="13.5px" style:color="var(--color-ink-1)" style:font-weight="500">
          {game.game_name}
        </div>
        <div
          class="mt-2 flex items-center gap-1.5 truncate font-mono"
          style:font-size="10.5px"
          style:color="var(--color-ink-3)"
          title={currentFolder}
        >
          <HardDrive size={11} class="shrink-0" />
          <span class="truncate">{currentFolder || 'No install folder'}</span>
          <span class="shrink-0" style:color="var(--color-ink-2)">· {fmtSize(game.install_size_mb)}</span>
        </div>
      {/if}
    </div>
  </div>

  <!-- body -->
  <div class="flex flex-col" style:padding="18px 24px 16px" style:gap="10px">
    {#if folders.length === 0}
      <p style:font-size="13px" style:color="var(--color-ink-2)" style:line-height="1.5">
        No library folders are set up yet. Add one per drive in
        <strong class="font-semibold text-ink-1">Settings → Library folders</strong>, then move the game there.
      </p>
    {:else}
      {#each rows as r (r.path)}
        {@render folderCard(r)}
      {/each}
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
          <span>{phase === 'done' ? 'Done' : progress?.status === 'finalizing' ? 'Finalising…' : 'Copying…'}</span>
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
      <div class="flex-1 min-w-0">
        {#if importMode && showDontAskAgain}
          <button
            type="button"
            onclick={async () => {
              await onDontAskAgain?.();
              onClose();
            }}
            class="text-[12.5px] font-medium transition-colors duration-100 text-ink-2 hover:text-ink-1 hover:underline"
            style:background="transparent"
            style:border="none"
            style:cursor="pointer"
            style:padding="0"
          >
            Don't ask again
          </button>
        {/if}
      </div>
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
        {phase === 'moving' ? (importMode ? 'Importing…' : 'Moving…') : phase === 'done' ? (importMode ? 'Imported' : 'Moved') : (importMode ? 'Import' : 'Move here')}
      </button>
    </div>
  </div>
</ModalShell>
