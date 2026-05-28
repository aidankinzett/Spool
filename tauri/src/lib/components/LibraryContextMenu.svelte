<script lang="ts">
  /**
   * Right-click context menu for library sidebar rows.
   *
   * Renders as a fixed-positioned card at the mouse coords (clamped to
   * the viewport). Outside-click and Escape both dismiss. Items mirror
   * the action toolbar in `GameDetail`; items whose backends aren't built
   * yet (Armoury Crate, manual backup/restore, LAN/Sync) are omitted
   * rather than disabled — the design has them, but we'll add as their
   * slices land.
   *
   * Design fidelity: small cover thumb + catalog id header, sectioned
   * items with dashed separators, per-game accent on hover for the
   * non-danger rows, red hover for Remove.
   */
  import { onMount, onDestroy } from 'svelte';
  import { ArrowDownToLine, ArrowUpFromLine, Folder, Package, Pencil, Play, Trash2 } from '@lucide/svelte';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { api, assetUrl } from '$lib/api';
  import { fmtCatalog } from '$lib/format';
  import { toasts } from '$lib/toasts.svelte';
  import type { GameEntry } from '$lib/types';

  let {
    game,
    x,
    y,
    onclose,
  }: {
    game: GameEntry;
    x: number;
    y: number;
    onclose: () => void;
  } = $props();

  let menuEl: HTMLDivElement | undefined = $state();

  const BRAND_SPOOL = '#d7c9a0';
  const accent = $derived(game.accent_color ?? BRAND_SPOOL);
  const cover = $derived(assetUrl(game.cover_image_path));

  // Clamp menu to viewport so it never opens partially off-screen.
  // Measure the rendered menu once mounted; until we have dimensions we
  // place the menu at the raw mouse coords. Derive the final position
  // from the measurements so changes to x/y (e.g. from a re-trigger)
  // propagate.
  let measured = $state({ w: 0, h: 0 });
  $effect(() => {
    if (!menuEl) return;
    const r = menuEl.getBoundingClientRect();
    if (r.width !== measured.w || r.height !== measured.h) {
      measured = { w: r.width, h: r.height };
    }
  });
  const pos = $derived.by(() => {
    if (!measured.w || !measured.h) return { x, y };
    const maxX = window.innerWidth - measured.w - 8;
    const maxY = window.innerHeight - measured.h - 8;
    return {
      x: Math.max(4, Math.min(x, maxX)),
      y: Math.max(4, Math.min(y, maxY)),
    };
  });

  function handleOutside(e: MouseEvent) {
    if (menuEl && !menuEl.contains(e.target as Node)) onclose();
  }
  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
  }
  onMount(() => {
    document.addEventListener('mousedown', handleOutside, true);
    document.addEventListener('keydown', handleKey, true);
  });
  onDestroy(() => {
    document.removeEventListener('mousedown', handleOutside, true);
    document.removeEventListener('keydown', handleKey, true);
  });

  // ── Action handlers — same logic as GameDetail's toolbar buttons ───────
  function folderForGame(g: GameEntry): string | null {
    if (g.game_folder_path) return g.game_folder_path;
    if (!g.exe_path) return null;
    const sep = g.exe_path.includes('\\') ? '\\' : '/';
    const idx = g.exe_path.lastIndexOf(sep);
    return idx > 0 ? g.exe_path.slice(0, idx) : null;
  }

  async function play() {
    onclose();
    try {
      await api.launchGame(game.id);
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'LAUNCH · FAILED',
        title: "Couldn't launch game",
        sub: String(e),
        catalog: fmtCatalog(game.catalog_number),
      });
    }
  }

  async function openFolder() {
    onclose();
    const f = folderForGame(game);
    if (f) await openPath(f);
  }

  async function generateArmouryLauncher() {
    onclose();
    try {
      const path = await api.generateArmouryLauncher(game.id);
      // Pull the dir off the end of the path string so the "Open
      // folder" CTA reveals the .exe in Explorer without needing
      // a separate IPC.
      const sep = path.includes('\\') ? '\\' : '/';
      const idx = path.lastIndexOf(sep);
      const dir = idx > 0 ? path.slice(0, idx) : path;
      toasts.show({
        kind: 'ok',
        label: 'ARMOURY CRATE',
        title: 'Launcher generated',
        sub: `In Armoury Crate: Library → Manage Library → Add → browse to ${path}`,
        catalog: fmtCatalog(game.catalog_number),
        duration: 0,
        cta: {
          label: 'Open folder',
          onClick: () => {
            openPath(dir).catch((e) => console.error('[launcher] open folder failed:', e));
          },
        },
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'ARMOURY CRATE · FAILED',
        title: "Couldn't generate launcher",
        sub: String(e),
        catalog: fmtCatalog(game.catalog_number),
      });
    }
  }

  async function manualBackup() {
    onclose();
    try {
      const r = await api.manualBackup(game.id);
      if (r.game_count === 0) {
        toasts.show({
          kind: 'info',
          label: 'LUDUSAVI',
          title: 'Nothing to back up',
          sub: `${game.game_name} has no save data ludusavi recognises.`,
          catalog: fmtCatalog(game.catalog_number),
        });
        return;
      }
      const mb = (r.bytes_total / (1024 * 1024)).toFixed(1);
      toasts.show({
        kind: 'ok',
        label: 'LUDUSAVI',
        title: 'Saves backed up',
        sub: `${game.game_name} · ${mb} MB`,
        catalog: fmtCatalog(game.catalog_number),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'LUDUSAVI · BACKUP',
        title: "Couldn't back up",
        sub: String(e),
        catalog: fmtCatalog(game.catalog_number),
      });
    }
  }

  async function manualRestore() {
    onclose();
    if (
      !confirm(
        `Restore saves for "${game.game_name}"?\n\nThis overwrites your current local saves with the most recent backup.`,
      )
    ) {
      return;
    }
    try {
      const r = await api.manualRestore(game.id);
      if (r.game_count === 0) {
        toasts.show({
          kind: 'info',
          label: 'LUDUSAVI',
          title: 'No backups found',
          sub: `${game.game_name} has nothing to restore yet.`,
          catalog: fmtCatalog(game.catalog_number),
        });
        return;
      }
      toasts.show({
        kind: 'ok',
        label: 'LUDUSAVI',
        title: 'Saves restored',
        sub: `${game.game_name} is ready to play.`,
        catalog: fmtCatalog(game.catalog_number),
      });
    } catch (e) {
      const msg = String(e);
      const isConflict = /cloud sync conflict/i.test(msg);
      toasts.show({
        kind: isConflict ? 'warn' : 'bad',
        label: isConflict ? 'LUDUSAVI · CONFLICT' : 'LUDUSAVI · RESTORE',
        title: isConflict ? 'Cloud sync conflict' : "Couldn't restore",
        sub: msg,
        catalog: fmtCatalog(game.catalog_number),
        cta: isConflict
          ? {
              label: 'Open Ludusavi',
              onClick: () => {
                api.openLudusaviGui().catch((err) =>
                  console.error('[ludusavi] open failed:', err),
                );
              },
            }
          : undefined,
      });
    }
  }

  async function addToSteam() {
    onclose();
    try {
      const r = await api.addToSteam(game.id);
      const extras = r.extras_placed.length ? ` · ${r.extras_placed.join(', ')} art placed` : '';
      toasts.show({
        kind: 'ok',
        label: 'STEAM',
        title: 'Added to Steam',
        sub: `Restart Steam to see "${game.game_name}" in your library${extras}.`,
        catalog: fmtCatalog(game.catalog_number),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'STEAM · FAILED',
        title: "Couldn't add to Steam",
        sub: String(e),
      });
    }
  }

  function openEdit() {
    onclose();
    WebviewWindow.getByLabel('edit-game').then((win) => {
      if (win) {
        win.setFocus();
        return;
      }
      new WebviewWindow('edit-game', {
        url: `/edit?id=${encodeURIComponent(game.id)}`,
        title: `Edit · ${game.game_name}`,
        width: 720,
        height: 660,
        minWidth: 600,
        minHeight: 480,
        decorations: false,
        resizable: true,
        center: true,
        backgroundColor: '#0b0c0e',
      });
    });
  }

  async function remove() {
    onclose();
    if (!confirm(`Remove "${game.game_name}" from your library?`)) return;
    try {
      await api.removeGame(game.id);
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'REMOVE · FAILED',
        title: "Couldn't remove",
        sub: String(e),
      });
    }
  }
</script>

<div
  bind:this={menuEl}
  role="menu"
  class="fixed z-50 w-[260px] overflow-hidden rounded-md border border-line-2 bg-bg-1 py-1.5 text-ink-0"
  style:left="{pos.x}px"
  style:top="{pos.y}px"
  style:box-shadow="0 18px 48px rgb(0 0 0 / 0.6)"
>
  <!-- Header: cover + name + catalog -->
  <div class="flex items-center gap-2.5 border-b border-dashed border-line-1 px-3.5 py-2 pb-2.5">
    <div class="h-[34px] w-6 shrink-0 overflow-hidden rounded-sm border border-line-1 bg-bg-2">
      {#if cover}
        <img src={cover} alt={game.game_name} class="h-full w-full object-cover" />
      {:else}
        <div
          class="h-full w-full"
          style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
        ></div>
      {/if}
    </div>
    <div class="min-w-0 flex-1">
      <div class="truncate text-[12px] font-medium" title={game.game_name}>{game.game_name}</div>
      <div
        class="font-mono mt-px text-[9px] tracking-[0.06em] text-ink-3"
      >
        {fmtCatalog(game.catalog_number)}
      </div>
    </div>
  </div>

  <!-- Section: launch + open + integrations -->
  {#snippet item(
    label: string,
    icon: import('svelte').Snippet,
    handler: () => void,
    disabled: boolean = false,
    danger: boolean = false,
  )}
    <button
      type="button"
      role="menuitem"
      onclick={handler}
      {disabled}
      class="group flex h-7 w-full items-center gap-2.5 px-3 text-left text-[12px] transition-colors disabled:cursor-not-allowed disabled:opacity-40"
      class:hover:bg-bad-15={danger}
      style:color={danger ? 'var(--color-ink-1)' : 'var(--color-ink-1)'}
      onmouseenter={(e) => {
        if (disabled) return;
        if (danger) {
          (e.currentTarget as HTMLElement).style.background = 'rgb(255 122 122 / 0.14)';
          (e.currentTarget as HTMLElement).style.color = '#ffa6a6';
        } else {
          (e.currentTarget as HTMLElement).style.background = `color-mix(in srgb, ${accent} 10%, transparent)`;
          (e.currentTarget as HTMLElement).style.color = 'var(--color-ink-0)';
        }
      }}
      onmouseleave={(e) => {
        (e.currentTarget as HTMLElement).style.background = 'transparent';
        (e.currentTarget as HTMLElement).style.color = 'var(--color-ink-1)';
      }}
    >
      <span class="flex" style:color={danger ? 'currentColor' : 'var(--color-ink-2)'}>
        {@render icon()}
      </span>
      <span class="flex-1">{label}</span>
    </button>
  {/snippet}

  <div class="py-1">
    {@render item(
      'Play',
      playIcon,
      play,
      !game.exe_path,
    )}
    {@render item(
      'Open install folder',
      folderIcon,
      openFolder,
      !folderForGame(game),
    )}
    {@render item('Add to Steam', steamIcon, addToSteam, !game.exe_path)}
    {@render item('Generate Armoury Crate launcher', armouryIcon, generateArmouryLauncher, !game.exe_path)}
  </div>

  <div class="border-t border-dashed border-line-1 py-1">
    {@render item('Back up saves now', backupIcon, manualBackup)}
    {@render item('Restore saves…', restoreIcon, manualRestore)}
  </div>

  {#snippet playIcon()}<Play size={13} fill="currentColor" />{/snippet}
  {#snippet folderIcon()}<Folder size={13} />{/snippet}
  {#snippet steamIcon()}<Play size={13} />{/snippet}
  {#snippet armouryIcon()}<Package size={13} />{/snippet}
  {#snippet backupIcon()}<ArrowUpFromLine size={13} />{/snippet}
  {#snippet restoreIcon()}<ArrowDownToLine size={13} />{/snippet}
  {#snippet pencilIcon()}<Pencil size={13} />{/snippet}
  {#snippet trashIcon()}<Trash2 size={13} />{/snippet}

  <div class="border-t border-dashed border-line-1 py-1">
    {@render item('Edit…', pencilIcon, openEdit)}
    {@render item('Remove from library…', trashIcon, remove, false, true)}
  </div>
</div>
