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
  import { ArrowDownToLine, ArrowUpFromLine, Folder, HardDriveDownload, Package, Pencil, Play, Trash2 } from '@lucide/svelte';
  import { openView } from '$lib/nav';
  import { api, assetUrl } from '$lib/api';
  import { fmtCatalog, folderForGame, parentDir } from '$lib/format';
  import { toasts } from '$lib/toasts.svelte';
  import { confirmDialog } from '$lib/confirm.svelte';
  import { confirmSteamRestart } from '$lib/steamRestart';
  import { removeGameDialog } from '$lib/removeGame.svelte';
  import { gamepadScope } from '$lib/gamepad';
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
  let isWindows = $state(false);
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
  onMount(async () => {
    document.addEventListener('mousedown', handleOutside, true);
    document.addEventListener('keydown', handleKey, true);
    isWindows = (await api.appPlatform()) === 'windows';
  });
  onDestroy(() => {
    document.removeEventListener('mousedown', handleOutside, true);
    document.removeEventListener('keydown', handleKey, true);
  });

  // ── Action handlers — same logic as GameDetail's toolbar buttons ───────


  async function play() {
    onclose();
    try {
      await api.launchGame(game.id);
    } catch (e) {
      const msg = String(e);
      if (!/cloud sync conflict/i.test(msg)) {
        toasts.show({
          kind: 'bad',
          label: 'LAUNCH · FAILED',
          title: "Couldn't launch game",
          sub: msg,
          catalog: fmtCatalog(game.catalog_number),
        });
      }
    }
  }

  async function openFolder() {
    onclose();
    const f = folderForGame(game);
    if (f) await api.openPath(f);
  }

  async function generateArmouryLauncher() {
    onclose();
    try {
      const path = await api.generateArmouryLauncher(game.id);
      // Pull the dir off the end of the path string so the "Open
      // folder" CTA reveals the .exe in Explorer without needing
      // a separate IPC.
      const dir = parentDir(path) ?? path;
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
            api.openPath(dir).catch((e) => console.error('[launcher] open folder failed:', e));
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
      // cloud_synced is true when no upload was needed/attempted (no remote) or
      // the upload reached the cloud; it's only false when a configured remote's
      // upload failed or hit a conflict. Mirror the saves:backup event toast.
      toasts.show(
        r.cloud_synced
          ? {
              kind: 'ok',
              label: 'LUDUSAVI',
              title: 'Saves backed up & synced',
              sub: `${game.game_name} · ${mb} MB · cloud updated`,
              catalog: fmtCatalog(game.catalog_number),
            }
          : {
              kind: 'warn',
              label: 'LUDUSAVI',
              title: 'Backed up locally',
              sub: `${game.game_name} · ${mb} MB · cloud sync pending`,
              catalog: fmtCatalog(game.catalog_number),
            },
      );
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
      !(await confirmDialog({
        label: 'LUDUSAVI · RESTORE',
        title: 'Restore saves from backup?',
        body: `This overwrites your current local saves for "${game.game_name}" with the most recent backup.`,
        confirmLabel: 'Restore saves',
        accent,
        catalog: fmtCatalog(game.catalog_number),
      }))
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
    if (!(await confirmSteamRestart())) return;
    try {
      const r = await api.addToSteam(game.id);
      const extras = r.extras_placed.length ? ` · ${r.extras_placed.join(', ')} art placed` : '';
      const sub = r.steam_restarted
        ? `Restarting Steam — "${game.game_name}" will appear in your library${extras}.`
        : `Restart Steam to see "${game.game_name}" in your library${extras}.`;
      toasts.show({
        kind: 'ok',
        label: 'STEAM',
        title: 'Added to Steam',
        sub,
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

  async function removeFromSteam() {
    onclose();
    if (!(await confirmSteamRestart('Removing from Steam'))) return;
    try {
      const removed = await api.removeFromSteam(game.id);
      toasts.show({
        kind: 'ok',
        label: 'STEAM',
        title: removed ? 'Removed from Steam' : 'Already removed',
        sub: removed
          ? `"${game.game_name}" was removed from your Steam library.`
          : `No Steam shortcut was found for "${game.game_name}".`,
        catalog: fmtCatalog(game.catalog_number),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'STEAM · FAILED',
        title: "Couldn't remove from Steam",
        sub: String(e),
      });
    }
  }

  function openEdit() {
    onclose();
    openView('edit', { id: game.id });
  }

  // Open the three-option remove chooser (remove from disk / from library /
  // from disk and library), hosted globally by RemoveGameHost.
  function remove() {
    onclose();
    removeGameDialog.request(game);
  }

  // Re-add an uninstalled game via the Add flow (which reuses this entry).
  function reinstall() {
    onclose();
    openView('add', { reinstall: game.id });
  }
</script>

<div
  bind:this={menuEl}
  role="menu"
  use:gamepadScope={{ onBack: onclose }}
  class="fixed z-50 w-[260px] overflow-hidden rounded-md border border-line-2 bg-bg-1 py-1.5 text-ink-0"
  style:left="{pos.x}px"
  style:top="{pos.y}px"
  style:--gp-focus={accent}
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
      data-danger={danger ? 'true' : undefined}
      class="menu-item group flex h-7 w-full items-center gap-2.5 px-3 text-left text-[12px] text-ink-1 transition-colors disabled:cursor-not-allowed disabled:opacity-40"
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
      !game.exe_path || !game.installed,
    )}
    {@render item(
      'Open install folder',
      folderIcon,
      openFolder,
      !folderForGame(game),
    )}
    {#if game.steam_app_id != null}
      {@render item('Remove from Steam', steamIcon, removeFromSteam, false)}
    {:else}
      {@render item('Add to Steam', steamIcon, addToSteam, !game.exe_path || !game.installed)}
    {/if}
    {#if isWindows}
      {@render item('Generate Armoury Crate launcher', armouryIcon, generateArmouryLauncher, !game.exe_path || !game.installed)}
    {/if}
  </div>

  <div class="border-t border-dashed border-line-1 py-1">
    {@render item('Back up saves now', backupIcon, manualBackup, !game.installed)}
    {@render item('Restore saves…', restoreIcon, manualRestore, !game.installed)}
  </div>

  {#snippet playIcon()}<Play size={13} fill="currentColor" />{/snippet}
  {#snippet folderIcon()}<Folder size={13} />{/snippet}
  {#snippet steamIcon()}<Play size={13} />{/snippet}
  {#snippet armouryIcon()}<Package size={13} />{/snippet}
  {#snippet backupIcon()}<ArrowUpFromLine size={13} />{/snippet}
  {#snippet restoreIcon()}<ArrowDownToLine size={13} />{/snippet}
  {#snippet pencilIcon()}<Pencil size={13} />{/snippet}
  {#snippet trashIcon()}<Trash2 size={13} />{/snippet}
  {#snippet reinstallIcon()}<HardDriveDownload size={13} />{/snippet}

  <div class="border-t border-dashed border-line-1 py-1">
    {@render item('Edit…', pencilIcon, openEdit)}
    {#if !game.installed}
      {@render item('Reinstall…', reinstallIcon, reinstall)}
    {/if}
    {@render item('Remove…', trashIcon, remove, false, true)}
  </div>
</div>

<style>
  /* Hover/focus highlight, driven by CSS instead of per-item JS. The accent
     tint reuses the menu's `--gp-focus` custom property (set on the root), so
     it also lights up on gamepad/keyboard focus, not just mouse hover. (#286) */
  .menu-item:not(:disabled):hover,
  .menu-item:not(:disabled):focus-visible {
    background: color-mix(in srgb, var(--gp-focus, var(--color-spool)) 10%, transparent);
    color: var(--color-ink-0);
  }
  .menu-item[data-danger]:not(:disabled):hover,
  .menu-item[data-danger]:not(:disabled):focus-visible {
    background: rgb(255 122 122 / 0.14);
    color: #ffa6a6;
  }
</style>
