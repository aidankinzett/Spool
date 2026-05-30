<script lang="ts">
  /**
   * Right-pane detail view of a selected game.
   *
   * Sections (top → bottom):
   *   - Hero: gradient backdrop, catalog id + side label, big title,
   *     Play button + last-played/playtime, LAN/sync pills
   *   - StatsStrip: 4 columns of mono stats
   *   - ActionToolbar: per-entry actions (Open folder, Steam, …)
   *   - About card: description + genres
   *   - Saves card: backup count / size / last-run
   *   - Details card: developer / publisher / executable / install path
   *
   * Per-game accent colour isn't extracted yet — every game uses the
   * brand `spool` tint until the cover-art-dominant-color slice lands.
   * The structure already passes `accent` through everywhere so wiring
   * it up later is a one-liner.
   */
  import {
    Cloud,
    Copy,
    Folder,
    Pencil,
    Play,
    Sparkles,
    Trash2,
  } from '@lucide/svelte';
  import { openView } from '$lib/nav';
  import { api } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import type { GameEntry, RunPhase } from '$lib/types';
  import {
    absDate,
    absDateTime,
    fmtCatalog,
    fmtPlaytime,
    fmtSize,
    relDate,
  } from '$lib/format';
  import MonoLabel from './MonoLabel.svelte';
  import CatalogId from './CatalogId.svelte';
  import Pill from './Pill.svelte';
  import Btn from './Btn.svelte';
  import DetailCard from './DetailCard.svelte';

  let {
    game,
    runPhase = null,
  }: {
    game: GameEntry;
    /** Current Run-workflow phase for *this* game (null if idle). */
    runPhase?: RunPhase | null;
  } = $props();

  const isRunning = $derived(runPhase != null);
  const playLabel = $derived.by(() => {
    switch (runPhase) {
      case 'restoring':
        return 'Restoring saves…';
      case 'launching':
        return 'Launching…';
      case 'playing':
        return 'Playing';
      case 'backing-up':
        return 'Backing up…';
      default:
        return 'Play';
    }
  });

  async function launch() {
    if (!game.exe_path) return;
    try {
      await api.launchGame(game.id);
    } catch (e) {
      // Error is also broadcast via run:phase → 'error', but capturing here
      // so the in-button label can flip back to "Play" immediately.
      console.error('[runner] launch failed:', e);
    }
  }

  /**
   * Per-game accent colour. Extracted from the cover image when it
   * downloaded (see steamgriddb::extract_vibrant_color); falls back to
   * the brand `spool` colour when None — keeps things consistent for
   * games without cover art and before extraction has run.
   */
  const BRAND_SPOOL = '#d7c9a0';
  const accentHex = $derived(game.accent_color ?? BRAND_SPOOL);
  // CSS-variable form for cases that need a token-style reference; same
  // value either way, just different consumers.
  const accent = $derived(accentHex);

  // Try to derive a folder path for the "Open folder" action: the entry's
  // own game_folder_path if set, else the parent of the exe path.
  function folderForGame(g: GameEntry): string | null {
    if (g.game_folder_path) return g.game_folder_path;
    if (!g.exe_path) return null;
    const sep = g.exe_path.includes('\\') ? '\\' : '/';
    const idx = g.exe_path.lastIndexOf(sep);
    return idx > 0 ? g.exe_path.slice(0, idx) : null;
  }

  async function openFolder() {
    const folder = folderForGame(game);
    if (folder) await api.openPath(folder);
  }

  async function copyToClipboard(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // best-effort; webview may not always grant clipboard
    }
  }

  async function removeGame() {
    if (!confirm(`Remove "${game.game_name}" from your library?`)) return;
    await api.removeGame(game.id);
    // library:changed event will cause the parent page to clear selection.
  }

  let generatingArmoury = $state(false);
  async function generateArmouryLauncher() {
    generatingArmoury = true;
    try {
      const path = await api.generateArmouryLauncher(game.id);
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
    } finally {
      generatingArmoury = false;
    }
  }

  let addingToSteam = $state(false);
  async function addToSteam() {
    addingToSteam = true;
    try {
      const result = await api.addToSteam(game.id);
      const extras = result.extras_placed.length
        ? ` · ${result.extras_placed.join(', ')} art placed`
        : '';
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
    } finally {
      addingToSteam = false;
    }
  }
</script>

<div class="flex min-w-0 flex-1 flex-col overflow-y-auto bg-bg-0">
  <!-- Hero -->
  <div
    class="relative h-[280px] overflow-hidden border-b border-line-1"
    style:background="linear-gradient(135deg, color-mix(in srgb, {accentHex} 22%, var(--color-bg-1)) 0%, var(--color-bg-0) 100%)"
  >
    <!-- tape strip across top -->
    <div
      class="absolute inset-x-0 top-0 h-1"
      style:background="linear-gradient(90deg, {accentHex} 0%, color-mix(in srgb, {accentHex} 60%, transparent) 50%, {accentHex} 100%)"
    ></div>
    <!-- tape-reel halos -->
    <div
      class="absolute right-[-120px] top-[-80px] h-[420px] w-[420px] rounded-full border"
      style:border-color="color-mix(in srgb, {accentHex} 13%, transparent)"
      style:background="radial-gradient(circle at 35% 35%, color-mix(in srgb, {accentHex} 20%, transparent), transparent 55%)"
    ></div>
    <div
      class="absolute right-[-40px] top-[30px] h-[260px] w-[260px] rounded-full border border-dashed"
      style:border-color="color-mix(in srgb, {accentHex} 20%, transparent)"
    ></div>

    <!-- subtle film grain -->
    <div
      class="pointer-events-none absolute inset-0 opacity-40 mix-blend-overlay"
      style:background-image="radial-gradient(rgb(255 255 255 / 0.05) 1px, transparent 1px)"
      style:background-size="3px 3px"
    ></div>

    <!-- bottom fade to bg-0 -->
    <div
      class="pointer-events-none absolute inset-0"
      style:background="linear-gradient(180deg, transparent 40%, var(--color-bg-0) 100%)"
    ></div>

    <!-- Content -->
    <div class="absolute inset-x-7 bottom-[22px] top-[26px] flex flex-col justify-between">
      <div class="flex items-center gap-2.5">
        <CatalogId id={fmtCatalog(game.catalog_number)} accent={accentHex} />
        {#if game.genres.length > 0}
          <MonoLabel size={10}>{game.genres[0].toUpperCase()}</MonoLabel>
        {/if}
      </div>

      <div>
        <h1
          data-testid="game-title"
          class="font-display max-w-[720px] text-[44px] font-bold leading-[1.04] text-ink-0 text-balance"
          style:letter-spacing="-0.025em"
          style:text-shadow="0 2px 16px rgb(0 0 0 / 0.4)"
        >
          {game.game_name}
        </h1>

        <div class="mt-3.5 flex items-center gap-3.5">
          <button
            type="button"
            data-testid="play-button"
            onclick={launch}
            disabled={isRunning || !game.exe_path}
            class="font-sans inline-flex items-center gap-2.5 rounded-md border-none font-semibold transition-opacity"
            style:height="var(--control-h)"
            style:padding-inline="calc(var(--space-unit) * 4)"
            style:font-size="var(--text-base)"
            class:cursor-pointer={!isRunning && !!game.exe_path}
            class:cursor-not-allowed={isRunning || !game.exe_path}
            class:opacity-70={isRunning || !game.exe_path}
            style:background={accentHex}
            style:color="#0b0c0e"
            style:box-shadow="0 6px 20px color-mix(in srgb, {accentHex} 26%, transparent)"
            title={!game.exe_path
              ? 'No executable set'
              : isRunning
                ? playLabel
                : 'Restore saves, launch game, back up on exit'}
          >
            <Play size={16} fill="currentColor" />
            {playLabel}
          </button>

          <div class="flex flex-col gap-px">
            <MonoLabel size={9.5}>
              <span style:color={accentHex}>
                LAST · {game.last_played_at ? relDate(game.last_played_at).toUpperCase() : 'NEVER'}
              </span>
            </MonoLabel>
            <span
              class="font-mono text-[11.5px] tracking-[0.04em] text-ink-2"
            >
              {fmtPlaytime(game.playtime_minutes)} · {game.save_backup_count} backup{game.save_backup_count === 1 ? '' : 's'}
            </span>
          </div>

          <div class="flex-1"></div>

          {#if game.has_cloud_save}
            <Pill kind="info">
              <Cloud size={9} />
              Cloud sync
            </Pill>
          {/if}
        </div>
      </div>
    </div>
  </div>

  <!-- Stats strip -->
  <div class="grid grid-cols-4 border-b border-line-1 px-7 py-4">
    {#snippet stat(label: string, value: string, sub: string, first: boolean = false)}
      <div class="px-[18px] {first ? '' : 'border-l border-dashed border-line-1'}">
        <MonoLabel size={9.5}>{label}</MonoLabel>
        <div
          class="font-display mt-1 text-[20px] font-semibold text-ink-0"
          style:letter-spacing="-0.015em"
        >
          {value}
        </div>
        <div
          class="font-mono mt-0.5 text-[10.5px] tracking-[0.04em] text-ink-2"
        >
          {sub}
        </div>
      </div>
    {/snippet}
    {@render stat(
      'Last played',
      game.last_played_at ? relDate(game.last_played_at) : 'Never',
      game.last_played_at ? absDateTime(game.last_played_at) : '—',
      true,
    )}
    {@render stat(
      'Playtime',
      fmtPlaytime(game.playtime_minutes),
      game.playtime_minutes > 0 ? 'across sessions' : 'no sessions yet',
    )}
    {@render stat(
      'Install size',
      fmtSize(game.install_size_mb),
      game.exe_path ? 'on disk' : '—',
    )}
    {@render stat(
      'Saves',
      game.save_backup_count > 0
        ? `${game.save_backup_count} backup${game.save_backup_count === 1 ? '' : 's'}`
        : '—',
      game.save_backup_count > 0
        ? `${fmtSize(game.save_backup_size_mb)} · ${relDate(game.save_last_backed_up_at)}`
        : 'no backups yet',
    )}
  </div>

  <!-- Action toolbar -->
  <div class="flex items-center gap-1.5 border-b border-line-1 px-7 py-3">
    <Btn variant="ghost" onclick={openFolder} disabled={!folderForGame(game)}>
      {#snippet icon()}<Folder size={14} />{/snippet}
      Open folder
    </Btn>
    <Btn
      variant="ghost"
      onclick={generateArmouryLauncher}
      disabled={!game.exe_path || generatingArmoury}
    >
      {#snippet icon()}<Sparkles size={14} />{/snippet}
      {generatingArmoury ? 'Generating…' : 'Armoury Crate'}
    </Btn>
    <Btn
      variant="ghost"
      onclick={addToSteam}
      disabled={!game.exe_path || addingToSteam}
    >
      {#snippet icon()}<Play size={14} />{/snippet}
      {addingToSteam ? 'Adding…' : 'Add to Steam'}
    </Btn>
    <div class="flex-1"></div>
    <Btn variant="ghost" onclick={() => openView('edit', { id: game.id })}>
      {#snippet icon()}<Pencil size={14} />{/snippet}
      Edit
    </Btn>
    <Btn variant="danger" onclick={removeGame}>
      {#snippet icon()}<Trash2 size={14} />{/snippet}
      Remove
    </Btn>
  </div>

  <!-- Two-column body -->
  <div
    class="grid gap-3.5 px-7 pb-7 pt-5"
    style:grid-template-columns="minmax(0, 1.4fr) minmax(0, 1fr)"
  >
    <div class="flex min-w-0 flex-col gap-3.5">
      <!-- About -->
      <DetailCard title="ABOUT" {accent}>
        {#if game.description || game.genres.length > 0}
          {#if game.description}
            <p class="m-0 text-[13px] leading-relaxed text-ink-1">
              {game.description}
            </p>
          {/if}
          {#if game.genres.length > 0}
            <div class="mt-3 flex flex-wrap gap-1.5">
              {#each game.genres as g (g)}
                <span
                  class="inline-flex items-center rounded-sm border border-line-2 bg-bg-2 px-2 py-px text-[11px] text-ink-1"
                >
                  {g}
                </span>
              {/each}
            </div>
          {/if}
        {:else}
          <p class="m-0 text-[12.5px] text-ink-3">
            No description on file. Spool will populate this when metadata fetching ships.
          </p>
        {/if}
      </DetailCard>

      <!-- Saves -->
      <DetailCard title="SAVE BACKUP · LUDUSAVI" {accent}>
        {#if game.save_backup_count > 0}
          <div class="grid grid-cols-3 gap-[18px]">
            {#snippet stat(label: string, value: string, sub: string)}
              <div>
                <MonoLabel size={9}>{label}</MonoLabel>
                <div
                  class="font-display mt-0.5 text-[18px] font-semibold text-ink-0"
                  style:letter-spacing="-0.01em"
                >
                  {value}
                </div>
                <div
                  class="font-mono mt-0.5 text-[10.5px] tracking-[0.04em] text-ink-3"
                >
                  {sub}
                </div>
              </div>
            {/snippet}
            {@render stat('LAST BACKUP', relDate(game.save_last_backed_up_at), absDateTime(game.save_last_backed_up_at))}
            {@render stat('REVISIONS', `${game.save_backup_count}`, 'across all profiles')}
            {@render stat('TOTAL SIZE', fmtSize(game.save_backup_size_mb), 'compressed')}
          </div>
          <div
            class="mt-3 flex items-center gap-2 rounded-sm border px-3 py-2 text-[11.5px] text-ink-1"
            style:border-color="color-mix(in srgb, var(--color-ok) 20%, transparent)"
            style:background="rgb(126 226 164 / 0.06)"
          >
            <Cloud size={12} class="text-ok" />
            Saves restore before launch and back up on exit automatically.
          </div>
        {:else if game.save_paths.length > 0}
          <div class="flex items-start gap-2.5 text-[12.5px] text-ink-2">
            <Cloud size={14} class="mt-0.5 shrink-0 text-ink-3" />
            <div class="min-w-0">
              <div>No backups yet — Spool will create one the first time you launch.</div>
              <div class="font-mono mt-1.5 text-[10.5px] tracking-[0.02em] text-ink-3">
                Will track:
                <span class="text-ink-2">{game.save_paths[0]}</span>
              </div>
            </div>
          </div>
        {:else}
          <div class="flex items-center gap-2.5 text-[12.5px] text-ink-2">
            <Cloud size={14} class="text-ink-3" />
            No save info from ludusavi — saves won't be backed up automatically.
          </div>
        {/if}
      </DetailCard>
    </div>

    <div class="flex min-w-0 flex-col gap-3.5">
      <DetailCard title="ENTRY · DETAILS" {accent}>
        <div class="flex flex-col">
          {#snippet row(
            label: string,
            value: string,
            mono: boolean = false,
            copy: boolean = false,
            last: boolean = false,
          )}
            <div
              class="grid items-center gap-2.5 py-2"
              class:border-b={!last}
              class:border-dashed={!last}
              class:border-line-1={!last}
              style:grid-template-columns="94px 1fr auto"
            >
              <div
                class="font-mono text-[9.5px] uppercase tracking-[0.1em] text-ink-3"
              >
                {label}
              </div>
              <div
                class="truncate text-ink-0"
                class:font-mono={mono}
                class:text-[11.5px]={mono}
                class:text-[12.5px]={!mono}
                title={value}
              >
                {value}
              </div>
              {#if copy}
                <button
                  type="button"
                  onclick={() => copyToClipboard(value)}
                  class="inline-flex p-1 text-ink-3 transition-colors hover:text-ink-0"
                  title="Copy"
                  aria-label="Copy {label}"
                >
                  <Copy size={12} />
                </button>
              {:else}
                <span></span>
              {/if}
            </div>
          {/snippet}
          {@render row('Developer', game.developer || '—')}
          {@render row('Publisher', game.publisher || '—')}
          {@render row('Released', absDate(game.release_date))}
          {@render row('Added', absDate(game.added_at))}
          {@render row('Executable', game.exe_path || '—', true, !!game.exe_path)}
          {@render row(
            'Install',
            folderForGame(game) ?? '—',
            true,
            !!folderForGame(game),
          )}
          {#if game.steam_id != null}
            {@render row('Steam ID', `${game.steam_id}`, true, true)}
          {/if}
          {@render row('Source', game.install_source, false, false, true)}
        </div>
      </DetailCard>
    </div>
  </div>
</div>
