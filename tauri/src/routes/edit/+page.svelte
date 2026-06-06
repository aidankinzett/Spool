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
  import { SvelteSet } from 'svelte/reactivity';
  import { Check, Download, Folder, FolderX, RefreshCw, Trash2 } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { api, assetUrl } from '$lib/api';
  import { fmtCatalog, absDateTime } from '$lib/format';
  import { toasts } from '$lib/toasts.svelte';
  import { confirmDialog } from '$lib/confirm.svelte';
  import type { GameEntry, ProtonVersion } from '$lib/types';
  import AppChrome from '$lib/components/AppChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import CatalogId from '$lib/components/CatalogId.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import TextField from '$lib/components/TextField.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import Select, { type SelectOption } from '$lib/components/Select.svelte';
  import { gamepadScope } from '$lib/gamepad';

  type Tab = 'identity' | 'install' | 'launch' | 'saves' | 'sharing';

  let original = $state<GameEntry | null>(null);
  let form = $state<GameEntry | null>(null);
  let tab = $state<Tab>('identity');
  let saving = $state(false);
  let error = $state<string | null>(null);

  // Proton launch (Linux only). Populated on mount.
  let isLinux = $state(false);
  let protonVersions = $state<ProtonVersion[]>([]);
  let depsInstalling = $state(false);

  // Saves tab: the in-progress custom save-location template (from the folder
  // picker or typed directly) and a busy flag for the set/clear actions.
  let saveTemplate = $state('');
  let savesBusy = $state(false);

  // Proton picker: Auto + each detected build.
  const protonOptions = $derived<SelectOption[]>([
    { value: '', label: 'Auto (newest installed)' },
    ...protonVersions.map((p) => ({ value: p.path, label: p.name })),
  ]);

  const VERB_PRESETS = [
    { verb: 'vcrun2022', label: 'Visual C++ 2022' },
    { verb: 'dotnet48', label: '.NET 4.8' },
    { verb: 'dotnet6', label: '.NET 6' },
    { verb: 'xna40', label: 'XNA 4.0' },
    { verb: 'physx', label: 'PhysX' },
    { verb: 'd3dcompiler_47', label: 'D3D Compiler 47' },
  ] as const;

  let depsChecked = new SvelteSet<string>();
  let depsCustomEnabled = $state(false);
  let depsCustom = $state('');
  const effectiveDeps = $derived(
    [
      ...depsChecked,
      ...(depsCustomEnabled ? depsCustom.trim().split(/\s+/).filter(Boolean) : []),
    ].join(' '),
  );

  function togglePreset(verb: string, checked: boolean) {
    if (checked) depsChecked.add(verb);
    else depsChecked.delete(verb);
  }

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

  // Whether the configured executable is a Windows `.exe`. On Linux these
  // launch through Proton automatically (no toggle — issue #80); the Proton
  // version / prefix / deps controls only make sense for such games.
  const exeIsWindows = $derived(
    (form?.exe_path ?? '').toLowerCase().endsWith('.exe'),
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
    const verbs = effectiveDeps.trim();
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

  // ── Saves tab ──────────────────────────────────────────────────────────
  // Pick a save folder, then ask the backend to turn it into a portable
  // template (e.g. <winLocalAppData>/MyGame) so it works on every device/OS.
  async function pickSaveFolder() {
    if (!form) return;
    // Open the picker inside the game's Proton prefix (its user profile) so the
    // user just navigates to the save, rather than hunting for the prefix.
    let defaultPath: string | undefined;
    try {
      defaultPath = (await api.savePickerStartDir(form.id)) ?? undefined;
    } catch (e) {
      console.error('[edit] savePickerStartDir failed:', e);
    }
    const picked = await openDialog({
      title: 'Pick the save folder',
      directory: true,
      multiple: false,
      defaultPath,
    });
    if (typeof picked !== 'string') return;
    try {
      saveTemplate = await api.deriveSaveTemplate(form.id, picked);
    } catch (e) {
      console.error('[edit] deriveSaveTemplate failed:', e);
      saveTemplate = picked; // fall back to the literal path
    }
  }

  // Register the custom save location: tracked by ludusavi and replicated to
  // the user's other devices. Updates form.custom_save in place (it's managed
  // out-of-band from the form's Save button).
  async function applyCustomSave() {
    if (!form) return;
    const token = saveTemplate.trim();
    if (!token || savesBusy) return;
    savesBusy = true;
    try {
      await api.setCustomSave(form.id, [token], []);
      form.custom_save = { files: [token], registry: [] };
      saveTemplate = '';
      toasts.show({
        kind: 'ok',
        label: 'SAVES',
        title: 'Save location set',
        sub: `${token} — synced to your devices`,
        catalog: fmtCatalog(form.catalog_number),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'SAVES · FAILED',
        title: "Couldn't set save location",
        sub: String(e),
      });
    } finally {
      savesBusy = false;
    }
  }

  async function clearCustomSave() {
    if (!form || savesBusy) return;
    savesBusy = true;
    try {
      await api.clearCustomSave(form.id);
      form.custom_save = null;
      toasts.show({
        kind: 'info',
        label: 'SAVES',
        title: 'Stopped tracking custom save',
        sub: 'Saves are no longer backed up for this game.',
        catalog: fmtCatalog(form.catalog_number),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'SAVES · FAILED',
        title: "Couldn't stop tracking",
        sub: String(e),
      });
    } finally {
      savesBusy = false;
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
    if (
      !(await confirmDialog({
        label: 'REMOVE · ENTRY',
        title: 'Remove from library?',
        body: `"${form.game_name}" will be forgotten. Your files on disk and save backups are left untouched — you can add it again later.`,
        confirmLabel: 'Remove',
        accent,
        catalog: fmtCatalog(form.catalog_number),
      }))
    )
      return;
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

  async function deleteFromDisk() {
    if (!form) return;
    if (
      !(await confirmDialog({
        label: 'DELETE · DISK',
        title: 'Delete from disk?',
        body:
          `This permanently removes the install folder` +
          `${form.game_folder_path ? ` (${form.game_folder_path})` : ''} and the library entry for "${form.game_name}". This can't be undone.`,
        confirmLabel: 'Delete from disk',
        danger: true,
        catalog: fmtCatalog(form.catalog_number),
      }))
    )
      return;
    try {
      await api.deleteGameFromDisk(form.id);
      await getCurrentWindow().close();
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'DELETE · FAILED',
        title: "Couldn't delete from disk",
        sub: String(e),
      });
    }
  }

  const tabs: { id: Tab; label: string }[] = [
    { id: 'identity', label: 'Identity' },
    { id: 'install', label: 'Install' },
    { id: 'launch', label: 'Launch' },
    { id: 'saves', label: 'Saves' },
    { id: 'sharing', label: 'Sharing' },
  ];

  // Bumpers cycle the edit tabs, like switching tabs on a console.
  function switchTab(dir: -1 | 1) {
    const ids = tabs.map((t) => t.id);
    const i = ids.indexOf(tab);
    tab = ids[(i + dir + ids.length) % ids.length];
  }

  function editButton(btn: string) {
    if (btn === 'LeftTrigger') switchTab(-1);
    else if (btn === 'RightTrigger') switchTab(1);
  }
</script>

<div
  class="flex h-screen flex-col bg-bg-0 text-ink-0"
  use:gamepadScope={{ onBack: () => history.back(), onButton: editButton }}
  style:--gp-focus={accent}
>
  <AppChrome sub="EDIT · ENTRY" {accent} onback={() => history.back()} />

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
            data-gp-autofocus={active ? '' : undefined}
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
          {#if !isLinux}
            {@render field(
              'Run as administrator',
              registryRunAsAdmin
                ? 'Already enabled by Windows for this exe — Spool will elevate either way.'
                : 'Required by some games (mostly older / DRM-laden). Off by default. Triggers a UAC prompt at launch.',
              launchRunAs,
            )}
          {/if}
          {#if isLinux && exeIsWindows}
            {@render field(
              'Proton version',
              'This Windows game launches through Proton automatically on Linux. Choose which Proton build to use; Auto picks the newest installed.',
              protonSelect,
            )}
            {@render field(
              'Wine prefix',
              'Override the per-game prefix folder. Leave blank for the Spool default.',
              prefixField,
            )}
            {@render field(
              'Install dependencies',
              'Install Windows runtime packages into this prefix via winetricks. Needs UMU or GE-Proton.',
              depsRow,
            )}
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
          {#snippet protonSelect()}
            <Select
              bind:value={
                () => form!.proton_version_path ?? '',
                (v) => (form!.proton_version_path = v)
              }
              options={protonOptions}
            />
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
            <div class="flex flex-col gap-2">
              <div class="grid grid-cols-2 gap-1.5">
                {#each VERB_PRESETS as p (p.verb)}
                  <label class="flex cursor-pointer items-start gap-2 rounded-[4px] border border-line-1 bg-bg-1 px-2.5 py-2 hover:bg-bg-2">
                    <input
                      type="checkbox"
                      checked={depsChecked.has(p.verb)}
                      onchange={(e) => togglePreset(p.verb, e.currentTarget.checked)}
                      class="mt-0.5 shrink-0 accent-[var(--color-spool)]"
                    />
                    <div class="min-w-0">
                      <div class="font-mono truncate text-[11px] font-medium text-ink-0">{p.verb}</div>
                      <div class="truncate text-[10px] text-ink-3">{p.label}</div>
                    </div>
                  </label>
                {/each}
              </div>
              <label class="flex cursor-pointer items-center gap-2 rounded-[4px] border border-line-1 bg-bg-1 px-2.5 py-2 hover:bg-bg-2">
                <input
                  type="checkbox"
                  bind:checked={depsCustomEnabled}
                  class="shrink-0 accent-[var(--color-spool)]"
                />
                <span class="font-mono text-[11px] font-medium text-ink-0">custom</span>
                {#if depsCustomEnabled}
                  <input
                    bind:value={depsCustom}
                    placeholder="e.g. dotnet7 d3dcompiler_47"
                    class="font-mono ml-1 min-w-0 flex-1 rounded-[3px] border border-line-2 bg-bg-2 px-2 py-0.5 text-[11px] text-ink-0 outline-none placeholder:text-ink-3 focus:border-line-3"
                  />
                {/if}
              </label>
              <div class="flex items-center justify-between">
                {#if effectiveDeps}
                  <span class="font-mono text-[10px] text-ink-3 truncate">→ {effectiveDeps}</span>
                {:else}
                  <span></span>
                {/if}
                <Btn variant="ghost" onclick={installDeps} disabled={depsInstalling || !effectiveDeps}>
                  {#snippet icon()}<Download size={14} />{/snippet}
                  {depsInstalling ? 'Installing…' : 'Install'}
                </Btn>
              </div>
            </div>
          {/snippet}
        {:else if tab === 'saves'}
          {@render field('Save tracking', '', savesStatus)}
          {#if form.custom_save && form.custom_save.files.length > 0}
            {@render field(
              'Custom save folder',
              'Spool backs this up after each session and syncs it to your other devices.',
              savesCurrent,
            )}
          {:else}
            {@render field(
              'Set a save folder',
              "Use this when ludusavi doesn't recognise the game, or saves it in the wrong place. Pick the folder, or type a ludusavi path template. Applies to all your devices.",
              savesPicker,
            )}
          {/if}

          {#snippet savesStatus()}
            <span class="text-[11.5px] text-ink-2">
              {#if form!.custom_save && form!.custom_save.files.length > 0}
                Custom folder — tracked and synced across your devices.
              {:else if form!.save_paths.length > 0}
                Tracked automatically via the ludusavi manifest.
              {:else}
                Not tracked — ludusavi doesn't recognise this game. Set a folder
                below to back up and sync its saves.
              {/if}
            </span>
          {/snippet}
          {#snippet savesCurrent()}
            <div class="flex flex-col items-start gap-2">
              <div class="font-mono w-full break-all rounded-[4px] border border-line-1 bg-bg-1 px-2.5 py-2 text-[11px] text-ink-1">
                {#each form!.custom_save!.files as f (f)}
                  <div>{f}</div>
                {/each}
              </div>
              <Btn variant="ghost" onclick={clearCustomSave} disabled={savesBusy}>
                {#snippet icon()}<FolderX size={14} />{/snippet}
                Stop tracking
              </Btn>
            </div>
          {/snippet}
          {#snippet savesPicker()}
            <div class="flex flex-col gap-2">
              <div class="flex gap-1.5">
                <TextField
                  bind:value={saveTemplate}
                  mono
                  full
                  placeholder="<winLocalAppData>/MyGame"
                />
                <Btn variant="ghost" onclick={pickSaveFolder}>
                  {#snippet icon()}<Folder size={14} />{/snippet}
                  Browse
                </Btn>
              </div>
              <div class="flex items-center justify-between">
                {#if saveTemplate.trim()}
                  <span class="font-mono truncate text-[10px] text-ink-3">→ {saveTemplate.trim()}</span>
                {:else}
                  <span></span>
                {/if}
                <Btn
                  variant="ghost"
                  onclick={applyCustomSave}
                  disabled={savesBusy || !saveTemplate.trim()}
                >
                  {#snippet icon()}<Check size={14} />{/snippet}
                  {savesBusy ? 'Saving…' : 'Use this location'}
                </Btn>
              </div>
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
        <Btn variant="danger" onclick={deleteFromDisk} disabled={!hasFolder}>
          {#snippet icon()}<FolderX size={14} />{/snippet}
          Delete from disk
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
