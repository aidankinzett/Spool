<script lang="ts">
  /**
   * Edit game dialog — opens as a child WebviewWindow from the detail
   * pane's Edit button. Three tabs (Identity / Install / Launch) covering
   * the fields we have in `GameEntry` today. Other tabs from the design
   * (Saves, Sharing, runner / env vars / window mode under Launch) will
   * land when the underlying schema does.
   *
   * Flow:
   *   - on mount, read `?id=` from URL, load the entry via list_games,
   *     snapshot it into the form state
   *   - tab switcher with the game's accent colour underlining the active
   *     tab + Save button background, same per-game tint as the detail
   *   - "Remove from library" button in the footer is destructive; falls
   *     back to a confirm prompt before calling remove_game
   *   - Save → call update_game with the merged entry → library:changed
   *     fires automatically → window closes
   *   - Cancel → close without saving (form state is discarded)
   */
  import { onMount } from 'svelte';
  import { Download, Folder, RefreshCw, Trash2 } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { api, assetUrl } from '$lib/api';
  import { fmtCatalog, absDateTime } from '$lib/format';
  import { toasts } from '$lib/toasts.svelte';
  import type { GameEntry, ProtonVersion } from '$lib/types';
  import WindowChrome from '$lib/components/WindowChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import CatalogId from '$lib/components/CatalogId.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import TextField from '$lib/components/TextField.svelte';
  import Toggle from '$lib/components/Toggle.svelte';

  type Tab = 'identity' | 'install' | 'launch' | 'sharing';

  let original = $state<GameEntry | null>(null);
  let form = $state<GameEntry | null>(null);
  let tab = $state<Tab>('identity');
  let saving = $state(false);
  let error = $state<string | null>(null);

  // Proton launch (Linux only). Populated on mount.
  let isLinux = $state(false);
  let protonVersions = $state<ProtonVersion[]>([]);
  let depsVerbs = $state('vcrun2022');
  let depsInstalling = $state(false);

  const BRAND_SPOOL = '#d7c9a0';
  const accent = $derived(form?.accent_color ?? BRAND_SPOOL);
  const cover = $derived(assetUrl(form?.cover_image_path));
  const dirty = $derived.by(() => {
    if (!form || !original) return false;
    // Cheap shallow compare on the editable fields.
    return (
      form.game_name !== original.game_name ||
      form.exe_path !== original.exe_path ||
      (form.game_folder_path ?? '') !== (original.game_folder_path ?? '') ||
      form.run_as_admin !== original.run_as_admin ||
      form.lan_shared !== original.lan_shared ||
      form.use_proton !== original.use_proton ||
      (form.proton_version_path ?? '') !== (original.proton_version_path ?? '') ||
      (form.wine_prefix_path ?? '') !== (original.wine_prefix_path ?? '') ||
      (form.launch_args ?? '') !== (original.launch_args ?? '')
    );
  });

  // Sharing tab gating: a game can only be flagged for LAN sharing if it
  // has a real install folder for the server to stream from.
  const hasFolder = $derived(
    !!form && (form.game_folder_path ?? '').length > 0,
  );

  // Tracks whether Windows itself has the exe flagged for elevation via
  // AppCompatFlags. When true, the per-entry Run-As-Admin toggle is
  // informational only — the launch will elevate regardless of the
  // entry's setting. Re-checked whenever exe_path changes.
  let registryRunAsAdmin = $state(false);
  $effect(() => {
    const exe = form?.exe_path ?? '';
    if (!exe) {
      registryRunAsAdmin = false;
      return;
    }
    api
      .getRunAsAdminInRegistry(exe)
      .then((v) => {
        registryRunAsAdmin = v;
      })
      .catch((e) => console.error('[edit] registry probe failed:', e));
  });

  onMount(async () => {
    try {
      const params = new URLSearchParams(window.location.search);
      const id = params.get('id');
      if (!id) {
        error = 'No game id in URL — close and reopen from the detail page.';
        return;
      }
      const all = await api.listGames();
      const found = all.find((g) => g.id === id);
      if (!found) {
        error = `Game ${id} not found in library.`;
        return;
      }
      original = found;
      form = { ...found };
      // Coerce nullable launch fields to '' so inputs/selects bind cleanly;
      // converted back to null on save. dirty-compare uses `?? ''` so this
      // doesn't register as a change.
      form.proton_version_path ??= '';
      form.wine_prefix_path ??= '';
      form.launch_args ??= '';

      // Linux-only Proton settings.
      try {
        isLinux = (await api.appPlatform()) === 'linux';
        if (isLinux) protonVersions = await api.listProtonVersions();
      } catch (e) {
        console.error('[edit] proton init failed:', e);
      }
    } catch (e) {
      error = String(e);
    }
  });

  async function browseExe() {
    if (!form) return;
    const picked = await openDialog({
      title: 'Pick the game executable',
      multiple: false,
      filters: [
        { name: 'Executable', extensions: ['exe', ''] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (typeof picked === 'string') {
      form.exe_path = picked;
    }
  }

  async function browseFolder() {
    if (!form) return;
    const picked = await openDialog({
      title: 'Pick the install folder',
      directory: true,
      multiple: false,
    });
    if (typeof picked === 'string') {
      form.game_folder_path = picked;
    }
  }

  async function installDeps() {
    if (!form || depsInstalling) return;
    const verbs = depsVerbs.trim();
    if (!verbs) return;
    depsInstalling = true;
    try {
      await api.installProtonDeps(form.id, verbs);
      toasts.show({
        kind: 'ok',
        label: 'PROTON',
        title: 'Dependencies installed',
        sub: verbs,
        catalog: fmtCatalog(form.catalog_number),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'PROTON · DEPS',
        title: "Couldn't install dependencies",
        sub: String(e),
      });
    } finally {
      depsInstalling = false;
    }
  }

  async function browsePrefix() {
    if (!form) return;
    const picked = await openDialog({
      title: 'Pick the Wine prefix folder',
      directory: true,
      multiple: false,
    });
    if (typeof picked === 'string') {
      form.wine_prefix_path = picked;
    }
  }

  async function refetchCover() {
    if (!form) return;
    try {
      await api.fetchCover(form.id);
      toasts.show({
        kind: 'info',
        label: 'COVER',
        title: 'Cover refreshed',
        sub: 'Pulled the latest from SteamGridDB.',
        catalog: fmtCatalog(form.catalog_number),
      });
      // Pull the entry again so we see the new path + accent immediately.
      const all = await api.listGames();
      const next = all.find((g) => g.id === form!.id);
      if (next) form = { ...next };
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'COVER · FAILED',
        title: "Couldn't refresh cover",
        sub: String(e),
      });
    }
  }

  async function save() {
    if (!form) return;
    saving = true;
    try {
      const payload = $state.snapshot(form);
      // Empty optional launch fields persist as null, not "".
      payload.proton_version_path = payload.proton_version_path || null;
      payload.wine_prefix_path = payload.wine_prefix_path || null;
      payload.launch_args = payload.launch_args || null;
      await api.updateGame(payload);
      await getCurrentWindow().close();
    } catch (e) {
      error = String(e);
      toasts.show({
        kind: 'bad',
        label: 'EDIT · FAILED',
        title: "Couldn't save changes",
        sub: String(e),
      });
    } finally {
      saving = false;
    }
  }

  async function cancel() {
    await getCurrentWindow().close();
  }

  async function removeGame() {
    if (!form) return;
    if (!confirm(`Remove "${form.game_name}" from your library?`)) return;
    try {
      await api.removeGame(form.id);
      await getCurrentWindow().close();
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'REMOVE · FAILED',
        title: "Couldn't remove",
        sub: String(e),
      });
    }
  }

  const tabs: { id: Tab; label: string }[] = [
    { id: 'identity', label: 'Identity' },
    { id: 'install', label: 'Install' },
    { id: 'launch', label: 'Launch' },
    { id: 'sharing', label: 'Sharing' },
  ];
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <WindowChrome sub="EDIT · ENTRY" {accent} />

  {#if error && !form}
    <main class="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
      <p class="text-bad">{error}</p>
      <Btn variant="ghost" onclick={cancel}>Close</Btn>
    </main>
  {:else if !form}
    <main class="flex flex-1 items-center justify-center">
      <p class="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-3">Loading…</p>
    </main>
  {:else}
    <main class="flex flex-1 flex-col overflow-hidden">
      <!-- Header: cover thumb + catalog + name -->
      <header class="flex items-center gap-3.5 border-b border-line-1 px-5 py-4">
        <div
          class="size-[50px_70px] h-[70px] w-[50px] shrink-0 overflow-hidden rounded-sm border border-line-1 bg-bg-2"
        >
          {#if cover}
            <img src={cover} alt={form.game_name} class="h-full w-full object-cover" />
          {:else}
            <div
              class="flex h-full w-full items-center justify-center"
              style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
            ></div>
          {/if}
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <CatalogId id={fmtCatalog(form.catalog_number)} {accent} />
            <MonoLabel size={10}>
              <span style:color={accent}>EDITING</span>
            </MonoLabel>
          </div>
          <div
            class="font-display mt-1 truncate text-[18px] font-semibold"
            style:letter-spacing="-0.012em"
            title={form.game_name}
          >
            {form.game_name}
          </div>
        </div>
      </header>

      <!-- Tab bar -->
      <div class="flex gap-0 border-b border-line-1 bg-bg-1 px-5">
        {#each tabs as t (t.id)}
          {@const active = tab === t.id}
          <button
            type="button"
            onclick={() => (tab = t.id)}
            class="cursor-pointer border-b-2 px-3.5 py-2.5 text-[12.5px] transition-colors"
            style:border-color={active ? accent : 'transparent'}
            style:color={active ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
            style:font-weight={active ? 500 : 400}
          >
            {t.label}
          </button>
        {/each}
      </div>

      <!-- Tab content -->
      <div class="flex-1 overflow-y-auto px-5 py-4">
        {#snippet field(label: string, helper: string, control: import('svelte').Snippet)}
          <div
            class="grid items-start gap-4 border-b border-dashed border-line-1 py-2.5"
            style:grid-template-columns="160px 1fr"
          >
            <div class="pt-1.5">
              <div class="text-[12.5px] font-medium text-ink-0">{label}</div>
              {#if helper}
                <div class="mt-0.5 text-[11px] leading-snug text-ink-2">{helper}</div>
              {/if}
            </div>
            <div>{@render control()}</div>
          </div>
        {/snippet}

        {#if tab === 'identity'}
          {@render field(
            'Title',
            "What shows in the library and on the detail page.",
            identityTitle,
          )}
          {@render field(
            'Cover art',
            'Refetch from SteamGridDB to update both the image and the accent colour.',
            identityCover,
          )}

          {#snippet identityTitle()}
            <TextField bind:value={form!.game_name} full />
          {/snippet}
          {#snippet identityCover()}
            <div class="flex flex-wrap gap-1.5">
              <Btn variant="ghost" onclick={refetchCover}>
                {#snippet icon()}<RefreshCw size={14} />{/snippet}
                Refetch from SteamGridDB
              </Btn>
            </div>
          {/snippet}
        {:else if tab === 'install'}
          {@render field('Install folder', 'Where the game lives on disk.', installFolder)}
          {@render field('Executable', 'The file Spool launches.', installExe)}
          {@render field('Added on', 'When this entry first appeared in your library.', installAdded)}

          {#snippet installFolder()}
            <div class="flex gap-1.5">
              <TextField
                bind:value={form!.game_folder_path as unknown as string}
                placeholder="(unset)"
                mono
                full
              />
              <Btn variant="ghost" onclick={browseFolder}>
                {#snippet icon()}<Folder size={14} />{/snippet}
                Browse
              </Btn>
            </div>
          {/snippet}
          {#snippet installExe()}
            <div class="flex gap-1.5">
              <TextField bind:value={form!.exe_path} mono full />
              <Btn variant="ghost" onclick={browseExe}>
                {#snippet icon()}<Folder size={14} />{/snippet}
                Browse
              </Btn>
            </div>
          {/snippet}
          {#snippet installAdded()}
            <span class="font-mono text-[11.5px] text-ink-2">
              {absDateTime(form!.added_at)}
            </span>
          {/snippet}
        {:else if tab === 'launch'}
          {@render field(
            'Run as administrator',
            registryRunAsAdmin
              ? 'Already enabled by Windows for this exe — Spool will elevate either way.'
              : 'Required by some games (mostly older / DRM-laden). Off by default. Triggers a UAC prompt at launch.',
            launchRunAs,
          )}
          {#if isLinux}
            {@render field(
              'Run with Proton',
              'Launch this Windows game through Proton (umu-run) on Linux. Required to run .exe games here.',
              protonToggle,
            )}
            {#if form!.use_proton}
              {@render field(
                'Proton version',
                'Which Proton build to launch with. Auto picks the newest installed.',
                protonSelect,
              )}
              {@render field(
                'Wine prefix',
                'Override the per-game prefix folder. Leave blank for the Spool default.',
                prefixField,
              )}
              {@render field(
                'Install dependencies',
                'Install Windows runtimes into this prefix via winetricks (space-separated, e.g. vcrun2022 dotnet48). Needs UMU/GE Proton.',
                depsRow,
              )}
            {/if}
          {/if}
          {@render field(
            'Launch arguments',
            'Extra command-line arguments passed after the executable.',
            argsField,
          )}

          {#snippet launchRunAs()}
            <div class="flex flex-col items-end gap-1.5">
              <Toggle bind:checked={form!.run_as_admin} aria-label="Run as administrator" />
              {#if registryRunAsAdmin}
                <span
                  class="font-mono inline-flex items-center gap-1 rounded-[3px] px-1.5 py-0.5 text-[9.5px] uppercase tracking-[0.08em]"
                  style:background="rgba(126,198,255,0.10)"
                  style:color="var(--color-info)"
                  title="Windows AppCompatFlags layers has RUNASADMIN set for this exe"
                >
                  Registry
                </span>
              {/if}
            </div>
          {/snippet}
          {#snippet protonToggle()}
            <Toggle bind:checked={form!.use_proton} aria-label="Run with Proton" />
          {/snippet}
          {#snippet protonSelect()}
            <select
              bind:value={form!.proton_version_path}
              style="color-scheme: dark"
              class="font-mono rounded-[4px] border border-line-1 bg-bg-2 px-2 py-1 text-[11.5px] text-ink-0"
            >
              <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="">Auto (newest installed)</option>
              {#each protonVersions as p (p.path)}
                <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value={p.path}>{p.name}</option>
              {/each}
            </select>
          {/snippet}
          {#snippet prefixField()}
            <div class="flex gap-1.5">
              <TextField
                bind:value={
                  () => form!.wine_prefix_path ?? '',
                  (v) => (form!.wine_prefix_path = v)
                }
                mono
                full
                placeholder="Spool default"
              />
              <Btn variant="ghost" onclick={browsePrefix}>
                {#snippet icon()}<Folder size={14} />{/snippet}
                Browse
              </Btn>
            </div>
          {/snippet}
          {#snippet argsField()}
            <TextField
              bind:value={
                () => form!.launch_args ?? '',
                (v) => (form!.launch_args = v)
              }
              mono
              full
              placeholder="--windowed -nolauncher"
            />
          {/snippet}
          {#snippet depsRow()}
            <div class="flex gap-1.5">
              <TextField bind:value={depsVerbs} mono full placeholder="vcrun2022" />
              <Btn variant="ghost" onclick={installDeps} disabled={depsInstalling}>
                {#snippet icon()}<Download size={14} />{/snippet}
                {depsInstalling ? 'Installing…' : 'Install'}
              </Btn>
            </div>
          {/snippet}
        {:else if tab === 'sharing'}
          {@render field(
            'Share over LAN',
            hasFolder
              ? "When on, other Spool devices on your network can install this game from you."
              : "Set an install folder on the Install tab first — there's nothing to stream without it.",
            sharingToggle,
          )}
          {@render field(
            'Visible to peers',
            'Peers see the title, catalog number, developer, and install size. They never see local file paths.',
            sharingVisibility,
          )}

          {#snippet sharingToggle()}
            <div class="flex flex-col gap-1.5">
              <Toggle
                bind:checked={form!.lan_shared}
                disabled={!hasFolder}
                aria-label="Share this game over LAN"
              />
              {#if !hasFolder && form!.lan_shared}
                <span class="font-mono text-[10px] uppercase tracking-[0.12em] text-warn">
                  Folder required
                </span>
              {/if}
            </div>
          {/snippet}
          {#snippet sharingVisibility()}
            <span class="text-[11.5px] text-ink-2">
              Title · Catalog # · Developer · Publisher · Genres · Install size · Save metadata
            </span>
          {/snippet}
        {/if}
      </div>

      <!-- Footer -->
      <footer class="flex items-center gap-2 border-t border-line-1 bg-black/20 px-5 py-3">
        <Btn variant="danger" onclick={removeGame}>
          {#snippet icon()}<Trash2 size={14} />{/snippet}
          Remove from library
        </Btn>
        <div class="flex-1"></div>
        <Btn variant="ghost" onclick={cancel}>Cancel</Btn>
        <button
          type="button"
          onclick={save}
          disabled={!dirty || saving}
          class="font-sans inline-flex h-8 min-w-[120px] items-center justify-center gap-1.5 rounded-sm border-none px-3 text-[13px] font-medium transition-opacity"
          class:cursor-pointer={dirty && !saving}
          class:cursor-not-allowed={!dirty || saving}
          class:opacity-50={!dirty || saving}
          style:background={accent}
          style:color="#0b0c0e"
        >
          {saving ? 'Saving…' : 'Save changes'}
        </button>
      </footer>
    </main>
  {/if}
</div>
