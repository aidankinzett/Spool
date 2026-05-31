<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Check,
    Folder,
    KeyRound,
    Layers,
    Library,
    MonitorSmartphone,
    Plus,
    RefreshCcw,
    Sparkles,
    Trash2,
    Wifi,
  } from '@lucide/svelte';
  import { goto } from '$app/navigation';
  import { appLocalDataDir } from '@tauri-apps/api/path';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { emit } from '@tauri-apps/api/event';
  import { api, type DeckyPluginInfo } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import type { ConfigData, DepStatus, LanPeer, ProtonVersion, SyncStatus } from '$lib/types';
  import AppChrome from '$lib/components/AppChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import Pill from '$lib/components/Pill.svelte';
  import TextField from '$lib/components/TextField.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import SettingsCard from '$lib/components/SettingsCard.svelte';
  import SettingsRow from '$lib/components/SettingsRow.svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import { uiMode } from '$lib/uiMode.svelte';

  let config = $state<ConfigData | null>(null);
  let error = $state<string | null>(null);
  let activeSection = $state('ludusavi');
  let peers = $state<LanPeer[]>([]);

  let syncStatus = $state<SyncStatus>({
    reachability: 'unconfigured',
    server_version: null,
    error: null,
    last_ok_ago_secs: null,
  });
  let registerOpen = $state(false);
  let registerAdminSecret = $state('');
  let registerUsername = $state('');
  let registerSubmitting = $state(false);

  // Cloud saves — WebDAV. The password is never persisted to config (ludusavi
  // obscures it into rclone.conf); it only lives here long enough to connect.
  let webdavPassword = $state('');
  let webdavConnecting = $state(false);
  let serverStorageConnecting = $state(false);

  // Proton / Compatibility (Linux only).
  let isLinux = $state(false);
  let protonVersions = $state<ProtonVersion[]>([]);
  let deps = $state<DepStatus[]>([]);
  let depsLoading = $state(false);

  // Decky plugin (SteamOS forced-close backup safety net) — Linux only.
  let deckyPlugin = $state<DeckyPluginInfo | null>(null);
  let deckyInstalling = $state(false);

  // Add Spool to Steam — Linux only.
  let addingSpoolToSteam = $state(false);

  onMount(async () => {
    try {
      config = await api.getConfig();
      syncStatus = await api.currentSyncStatus();
      peers = await api.listLanPeers();
      isLinux = (await api.appPlatform()) === 'linux';
      if (isLinux) {
        protonVersions = await api.listProtonVersions();
        deps = await api.checkDependencies();
        try {
          deckyPlugin = await api.deckyPluginStatus();
        } catch (e) {
          console.error('[settings] deckyPluginStatus failed:', e);
        }
      }
    } catch (e) {
      error = String(e);
    }
  });

  async function refreshSync() {
    try {
      syncStatus = await api.refreshSyncStatus();
    } catch (e) {
      console.error('[settings] refreshSyncStatus failed:', e);
    }
  }

  async function persistAndRefresh() {
    await persist();
    await refreshSync();
  }

  async function submitRegister() {
    if (!config) return;
    const url = config.sync_server_url.trim();
    if (!url) {
      toasts.show({ kind: 'warn', label: 'SYNC', title: 'Server URL required', sub: 'Set the URL above before registering.' });
      return;
    }
    if (!registerAdminSecret.trim() || !registerUsername.trim()) {
      toasts.show({ kind: 'warn', label: 'SYNC', title: 'Missing fields', sub: 'Admin secret and username are both required.' });
      return;
    }
    registerSubmitting = true;
    try {
      const apiKey = await api.syncRegisterAccount(url, registerAdminSecret.trim(), registerUsername.trim());
      config.sync_server_api_key = apiKey;
      config.sync_server_enabled = true;
      await persistAndRefresh();
      registerAdminSecret = '';
      registerUsername = '';
      registerOpen = false;
      toasts.show({ kind: 'ok', label: 'SYNC', title: 'Registered', sub: 'API key filled in. Sync server is now configured.' });
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'SYNC · REGISTER', title: "Couldn't register", sub: String(e) });
    } finally {
      registerSubmitting = false;
    }
  }

  async function persist(): Promise<boolean> {
    if (!config) return false;
    try {
      config = await api.updateConfig($state.snapshot(config));
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }

  async function setUiMode(mode: ConfigData['ui_mode']) {
    if (!config) return;
    config.ui_mode = mode;
    const ok = await persist();
    if (!ok) return;
    await uiMode.init(mode); // applies <html data-mode> live in this window
    // Tell every other open window (the main library) to re-resolve the mode
    // so the switch takes effect everywhere without a restart.
    await emit('config:ui-mode-changed');

    // Keep this window coherent with the new windowing model. Desktop opens
    // each view as its own decorated child window; touch routes everything
    // inside the main window. When the model flips under us the current
    // window's chrome no longer matches — without this the Settings window
    // loses its close button in touch mode (the back chevron's history.back()
    // is a dead-end in a freshly-spawned window) and traps the user.
    const win = getCurrentWindow();
    if (win.label !== 'main') {
      // Spawned desktop child window. Touch mode has no separate Settings
      // window, so hand control back to the main library window.
      if (uiMode.resolved === 'touch') {
        toasts.show({
          kind: 'ok',
          label: 'DISPLAY',
          title: 'Touch mode on',
          sub: 'Switched the library to touch layout.',
        });
        await win.close();
      }
    } else if (uiMode.resolved === 'desktop') {
      // We're the main window showing the in-app Settings route (touch nav).
      // Desktop chrome's close button targets the whole window, so step back
      // to the library where that chrome belongs.
      await goto('/');
    }
  }

  async function autoDetect() {
    if (!config) return;
    const found = await api.detectLudusavi();
    if (found) config.ludusavi_path = found;
    config = await api.getConfig();
  }

  async function browseLudusavi() {
    const picked = await openDialog({
      title: 'Locate ludusavi executable',
      multiple: false,
      filters: [{ name: 'Executable', extensions: ['exe', ''] }, { name: 'All files', extensions: ['*'] }],
    });
    if (typeof picked === 'string' && config) {
      config.ludusavi_path = picked;
      await persist();
    }
  }

  async function refreshDeps() {
    depsLoading = true;
    try { deps = await api.checkDependencies(); } finally { depsLoading = false; }
  }

  async function installDeckyPlugin() {
    deckyInstalling = true;
    try {
      await api.installDeckyPlugin();
      deckyPlugin = await api.deckyPluginStatus();
      toasts.show({
        kind: 'ok',
        label: 'DECKY',
        title: 'Backup plugin installed',
        sub: 'Decky was restarted — the Spool Backup plugin is now active.',
      });
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'DECKY', title: "Couldn't install plugin", sub: String(e) });
    } finally {
      deckyInstalling = false;
    }
  }

  async function addSpoolToSteam() {
    addingSpoolToSteam = true;
    try {
      await api.addSpoolToSteam();
      toasts.show({
        kind: 'ok',
        label: 'STEAM',
        title: 'Spool added to Steam',
        sub: 'Restart Steam to see the Spool shortcut in your library.',
      });
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'STEAM', title: "Couldn't add to Steam", sub: String(e) });
    } finally {
      addingSpoolToSteam = false;
    }
  }

  async function autoDetectUmu() {
    if (!config) return;
    const found = await api.detectUmuRun();
    if (found) config.umu_run_path = found;
    config = await api.getConfig();
    await refreshDeps();
  }

  async function browseUmu() {
    const picked = await openDialog({ title: 'Locate umu-run', multiple: false });
    if (typeof picked === 'string' && config) {
      config.umu_run_path = picked;
      await persist();
    }
  }

  async function browseRclone() {
    const picked = await openDialog({ title: 'Locate rclone', multiple: false });
    if (typeof picked === 'string' && config) {
      config.rclone_path = picked;
      await persist();
    }
  }

  async function connectWebdav() {
    if (!config) return;
    webdavConnecting = true;
    try {
      await api.setCloudWebdav(
        config.cloud_webdav_url.trim(),
        config.cloud_webdav_username.trim(),
        webdavPassword,
        'other',
      );
      webdavPassword = '';
      config = await api.getConfig();
      toasts.show({ kind: 'ok', label: 'CLOUD', title: 'WebDAV connected', sub: 'Saves will sync to this remote.' });
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'CLOUD · WEBDAV', title: "Couldn't connect", sub: String(e) });
    } finally {
      webdavConnecting = false;
    }
  }

  async function useServerStorage() {
    serverStorageConnecting = true;
    try {
      await api.useServerSaveStorage();
      config = await api.getConfig();
      toasts.show({ kind: 'ok', label: 'CLOUD', title: 'Save storage connected', sub: 'Saves will sync to your Spool server.' });
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'CLOUD · SERVER', title: "Couldn't connect storage", sub: String(e) });
    } finally {
      serverStorageConnecting = false;
    }
  }

  async function disconnectServerStorage() {
    if (!config) return;
    serverStorageConnecting = true;
    try {
      config.cloud_provider = '';
      config.cloud_webdav_url = '';
      config.cloud_webdav_username = '';
      await persist();
      toasts.show({ kind: 'ok', label: 'CLOUD', title: 'Disconnected', sub: 'Save storage turned off.' });
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'CLOUD · SERVER', title: "Couldn't disconnect", sub: String(e) });
    } finally {
      serverStorageConnecting = false;
    }
  }

  async function browseLanInstallDir() {
    const picked = await openDialog({ title: 'Pick the LAN install folder', directory: true, multiple: false });
    if (typeof picked === 'string' && config) {
      config.lan_install_dir = picked;
      await persist();
    }
  }

  function onLanPortCommit() {
    if (!config) return;
    if (!Number.isFinite(config.lan_share_port) || config.lan_share_port < 1024) config.lan_share_port = 47632;
    if (config.lan_share_port > 65535) config.lan_share_port = 65535;
    persist();
  }

  function onRetentionCommit() {
    if (!config) return;
    // Mirror the backend clamp (1–10) so the field reflects what's stored.
    if (!Number.isFinite(config.save_retention_full)) config.save_retention_full = 3;
    config.save_retention_full = Math.min(10, Math.max(1, Math.round(config.save_retention_full)));
    persist();
  }

  function scrollToSection(id: string) {
    activeSection = id;
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  const NAV_GROUPS = $derived([
    {
      id: 'display',
      title: 'Display',
      items: [
        { id: 'display', title: 'Display & touch', sub: 'Density & handheld mode' },
      ],
    },
    {
      id: 'library',
      title: 'Library',
      items: [
        { id: 'ludusavi', title: 'Ludusavi', sub: 'Save backup engine' },
        ...(isLinux
          ? [
              { id: 'compat', title: 'Compatibility', sub: 'Proton / umu-run' },
              { id: 'steam', title: 'Steam', sub: 'Add Spool to Steam library' },
              { id: 'decky', title: 'Steam Deck', sub: 'Backup safety net plugin' },
            ]
          : []),
        { id: 'cloud-saves', title: 'Cloud saves', sub: 'rclone remote' },
        { id: 'artwork', title: 'Cover artwork', sub: 'SteamGridDB' },
      ],
    },
    {
      id: 'sharing',
      title: 'Sharing & Sync',
      items: [
        { id: 'lan', title: 'LAN sharing', sub: 'Transfers between devices' },
        { id: 'sync', title: 'Sync server', sub: 'Session locks' },
        { id: 'device', title: 'This device', sub: 'Shown to peers' },
      ],
    },
  ]);
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <AppChrome sub="SETTINGS" onback={() => history.back()} />

  {#if error}
    <div class="flex-1 flex items-center justify-center p-8">
      <div class="rounded-md border border-bad/40 bg-bad/10 p-4 text-sm text-bad max-w-lg">
        <div class="mb-1 font-medium">Failed to load settings</div>
        <code class="font-mono text-[11px] opacity-80">{error}</code>
      </div>
    </div>
  {:else if !config}
    <div class="flex-1 flex items-center justify-center">
      <p class="font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">Loading…</p>
    </div>
  {:else}
    <!-- Two-pane: sidebar + scroll body -->
    <div class="flex flex-1 min-h-0" style="display:grid; grid-template-columns: 260px 1fr">

      <!-- ── Sidebar nav ── -->
      <nav class="overflow-y-auto border-r border-line-1 bg-bg-1 p-[20px_14px]">
        {#each NAV_GROUPS as group (group.id)}
          <div class="mb-[22px]">
            <!-- Group header -->
            <div class="flex items-center gap-2 px-2 pb-1.5">
              {#if group.id === 'display'}
                <MonitorSmartphone size={13} class="text-ink-2" />
              {:else if group.id === 'library'}
                <Library size={13} class="text-ink-2" />
              {:else if group.id === 'sharing'}
                <Wifi size={13} class="text-ink-2" />
              {:else}
                <Layers size={13} class="text-ink-2" />
              {/if}
              <MonoLabel size={10}>{group.title}</MonoLabel>
            </div>

            <!-- Nav items -->
            {#each group.items as item (item.id)}
              {@const isActive = activeSection === item.id}
              <button
                onclick={() => scrollToSection(item.id)}
                class="flex w-full flex-col items-start gap-px px-2 py-[6px] text-left transition-colors"
                style:background={isActive ? 'var(--color-bg-3)' : 'transparent'}
                style:border-left={isActive ? '2px solid var(--color-spool)' : '2px solid transparent'}
                style:color={isActive ? 'var(--color-ink-0)' : 'var(--color-ink-1)'}
              >
                <span class="text-[12.5px] font-medium">{item.title}</span>
                <span class="text-[10.5px] text-ink-3">{item.sub}</span>
              </button>
            {/each}
          </div>
        {/each}

        <!-- Footer -->
        <div class="mt-6 flex flex-col gap-1 border-t border-dashed border-line-1 pt-[10px] px-2">
          <MonoLabel size={9}>Spool · Settings</MonoLabel>
          <span class="text-[11px] text-ink-2">Changes save as you go</span>
        </div>
      </nav>

      <!-- ── Scroll body ── -->
      <div class="overflow-y-auto px-10 pb-20 pt-8">
        <div class="mx-auto max-w-[720px]">

          <!-- Page header -->
          <div class="mb-8">
            <MonoLabel size={10}>Spool · Cabinet</MonoLabel>
            <h1 class="mb-1 mt-1.5 font-display text-[32px] font-bold tracking-[-0.022em] text-ink-0">
              Settings
            </h1>
            <p class="max-w-[540px] text-[13px] leading-[1.55] text-ink-2">
              Set up Ludusavi, share games on your LAN, and connect external sources.
              Changes save as you go.
            </p>
          </div>

          <!-- ════════════════ DISPLAY GROUP ════════════════ -->
          <section class="mb-9">
            <div class="mb-3.5 border-b border-line-1 pb-2.5">
              <h2 class="font-display text-[20px] font-semibold tracking-[-0.01em] text-ink-0">Display</h2>
              <div class="mt-[3px] text-[12px] text-ink-2">How big the controls are and whether Spool runs in handheld mode.</div>
            </div>
            <div class="flex flex-col gap-4">
              <div id="display">
                <SettingsCard title="Display & touch" helper="Auto detects a touchscreen and grows targets for handhelds. Override it for a Deck/Ally docked to a monitor.">
                  <SettingsRow
                    label="Touch mode"
                    helper={`Larger buttons, taller rows, tap-friendly spacing. Currently rendering: ${uiMode.resolved}.`}
                  >
                    {#snippet extras()}
                      <Segmented
                        value={config!.ui_mode}
                        onchange={(v) => setUiMode(v as ConfigData['ui_mode'])}
                        options={[
                          { value: 'auto', label: 'Auto' },
                          { value: 'desktop', label: 'Desktop' },
                          { value: 'touch', label: 'Touch' },
                        ]}
                      />
                    {/snippet}
                  </SettingsRow>
                </SettingsCard>
              </div>
            </div>
          </section>

          <!-- ════════════════ LIBRARY GROUP ════════════════ -->
          <section class="mb-9">
            <div class="mb-3.5 border-b border-line-1 pb-2.5">
              <h2 class="font-display text-[20px] font-semibold tracking-[-0.01em] text-ink-0">Library</h2>
              <div class="mt-[3px] text-[12px] text-ink-2">Where saves live and how the shelf looks.</div>
            </div>
            <div class="flex flex-col gap-4">

              <!-- Ludusavi section -->
              <div id="ludusavi">
                <SettingsCard title="Ludusavi" helper="Spool delegates save backup and restore to ludusavi. We won't touch a game without it.">
                  <SettingsRow
                    label="Executable"
                    helper={config.ludusavi_path ? 'Detected — binary reachable' : 'Browse to ludusavi.exe or use auto-detect'}
                    status={config.ludusavi_path ? 'ok' : 'warn'}
                  >
                    {#snippet extras()}
                      <TextField bind:value={config!.ludusavi_path} placeholder="C:\path\to\ludusavi.exe" mono full oncommit={persist} />
                      <Btn variant="ghost" onclick={autoDetect}>
                        {#snippet icon()}<Sparkles size={14} />{/snippet}
                        Auto-detect
                      </Btn>
                      <Btn variant="ghost" onclick={browseLudusavi}>
                        {#snippet icon()}<Folder size={14} />{/snippet}
                        Browse
                      </Btn>
                    {/snippet}
                  </SettingsRow>

                  <SettingsRow
                    label="Save revisions to keep"
                    helper="How many save backups to retain per game. More gives more rollback points (restore an earlier save from a game's detail panel), at the cost of more disk and cloud upload. 1–10."
                  >
                    {#snippet control()}
                      <input
                        type="number"
                        min="1"
                        max="10"
                        bind:value={config!.save_retention_full}
                        onblur={onRetentionCommit}
                        class="font-mono h-7 w-24 rounded-sm border border-line-1 bg-bg-2 px-2 text-right text-[12px] text-ink-0 outline-none focus:border-line-3"
                      />
                    {/snippet}
                  </SettingsRow>

                </SettingsCard>
              </div>

              <!-- Compatibility (Proton) — Linux only -->
              {#if isLinux}
                <div id="compat">
                  <SettingsCard title="Compatibility (Proton)" helper="Run Windows games on Linux via Proton. umu-run manages the runtime; each game gets its own prefix.">

                    <!-- Dependency doctor -->
                    {#if deps.length > 0}
                      <div class="border-b border-dashed border-line-1 px-[18px] py-[14px]">
                        <div class="mb-2 flex items-center justify-between">
                          <span class="text-[12px] font-medium text-ink-1">Dependencies</span>
                          <button
                            onclick={refreshDeps}
                            disabled={depsLoading}
                            class="flex items-center gap-1 font-mono text-[10px] uppercase tracking-[0.1em] text-ink-3 hover:text-ink-1 disabled:opacity-40"
                          >
                            <RefreshCcw size={10} class={depsLoading ? 'animate-spin' : ''} />
                            Refresh
                          </button>
                        </div>
                        <div class="flex flex-col gap-2">
                          {#each deps as dep (dep.name)}
                            <div class="flex flex-col gap-0.5">
                              <div class="flex items-center gap-2">
                                <span
                                  class="size-[6px] flex-shrink-0 rounded-full"
                                  style:background={dep.found ? 'var(--color-ok)' : 'var(--color-warn)'}
                                ></span>
                                <span class="font-mono text-[12px] text-ink-0">{dep.name}</span>
                                {#if dep.found}
                                  <span
                                    class="rounded-[3px] px-1.5 py-px font-mono text-[9.5px] uppercase tracking-[0.08em]"
                                    style:background={
                                      dep.source === 'bundled' ? 'rgba(126,198,255,0.12)' :
                                      dep.source === 'config'  ? 'rgba(215,201,160,0.12)' :
                                                                  'rgba(120,220,160,0.12)'
                                    }
                                    style:color={
                                      dep.source === 'bundled' ? 'var(--color-info)' :
                                      dep.source === 'config'  ? 'var(--color-spool)' :
                                                                  'var(--color-ok)'
                                    }
                                  >{dep.source}</span>
                                {/if}
                              </div>
                              {#if dep.found}
                                <p class="ml-4 font-mono text-[10.5px] text-ink-3 break-all">{dep.path}</p>
                              {:else}
                                <div class="ml-4 flex flex-col gap-0.5">
                                  <p class="text-[11px] text-warn">Not found</p>
                                  {#if dep.install_hint}
                                    <code class="rounded-[3px] bg-black/30 px-2 py-1 font-mono text-[10.5px] text-ink-2 select-all">{dep.install_hint}</code>
                                  {/if}
                                </div>
                              {/if}
                            </div>
                          {/each}
                        </div>
                      </div>
                    {/if}

                    <SettingsRow
                      label="umu-run"
                      helper={config.umu_run_path ? 'Detected — launcher reachable' : 'Install umu-launcher or browse to umu-run'}
                      status={config.umu_run_path ? 'ok' : 'warn'}
                    >
                      {#snippet extras()}
                        <TextField bind:value={config!.umu_run_path} placeholder="/usr/bin/umu-run" mono full oncommit={persist} />
                        <Btn variant="ghost" onclick={autoDetectUmu}>
                          {#snippet icon()}<Sparkles size={14} />{/snippet}
                          Auto-detect
                        </Btn>
                        <Btn variant="ghost" onclick={browseUmu}>
                          {#snippet icon()}<Folder size={14} />{/snippet}
                          Browse
                        </Btn>
                      {/snippet}
                    </SettingsRow>
                    <SettingsRow label="Default Proton" helper="Used when a game doesn't pick its own version. Auto chooses the newest installed.">
                      {#snippet extras()}
                        <select
                          bind:value={config!.default_proton_path}
                          onchange={persist}
                          style="color-scheme: dark"
                          class="font-mono rounded-[4px] border border-line-1 bg-bg-2 px-2 py-1 text-[11.5px] text-ink-0"
                        >
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="">Auto (newest installed)</option>
                          {#each protonVersions as p (p.path)}
                            <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value={p.path}>{p.name}</option>
                          {/each}
                        </select>
                      {/snippet}
                    </SettingsRow>
                  </SettingsCard>
                </div>
              {/if}

              <!-- Steam — add Spool to the Steam library -->
              {#if isLinux}
                <div id="steam">
                  <SettingsCard
                    title="Steam"
                    helper="Add Spool to your Steam library so you can launch it from Gaming Mode on SteamOS and Steam Deck."
                  >
                    <SettingsRow
                      label="Add Spool to Steam"
                      helper="Creates a non-Steam shortcut in your Steam library. Restart Steam after adding."
                    >
                      {#snippet extras()}
                        <Btn
                          variant="primary"
                          onclick={addSpoolToSteam}
                          disabled={addingSpoolToSteam}
                        >
                          {#snippet icon()}<Plus size={14} />{/snippet}
                          {addingSpoolToSteam ? 'Adding…' : 'Add to Steam'}
                        </Btn>
                      {/snippet}
                    </SettingsRow>
                  </SettingsCard>
                </div>
              {/if}

              <!-- Steam Deck — forced-close backup plugin (Decky) -->
              {#if isLinux}
                {@const dp = deckyPlugin}
                {@const needsUpdate = !!dp?.installed && dp.installedVersion !== dp.bundledVersion}
                <div id="decky">
                  <SettingsCard
                    title="Steam Deck backup plugin"
                    helper="In Game Mode, closing a game with Quick Access → Exit Game can kill Spool before it backs up. This optional Decky Loader plugin runs the backup as a safety net from outside the game's process tree."
                  >
                    <SettingsRow
                      label="Spool Backup plugin"
                      status={dp?.installed ? (needsUpdate ? 'warn' : 'ok') : 'warn'}
                      helper={!dp
                        ? 'Checking…'
                        : !dp.deckyPresent
                          ? "Decky Loader isn't detected (~/homebrew). Install Decky first, then come back."
                          : dp.installed
                            ? needsUpdate
                              ? `Installed v${dp.installedVersion ?? '?'} · bundled v${dp.bundledVersion}. An update is available.`
                              : `Installed v${dp.installedVersion ?? dp.bundledVersion} — up to date.`
                            : `Not installed. Bundled v${dp.bundledVersion}. Installing asks for your password and restarts Decky — do this from Desktop Mode.`}
                    >
                      {#snippet extras()}
                        <Pill kind={dp?.installed ? (needsUpdate ? 'warn' : 'ok') : 'off'}>
                          {dp?.installed ? (needsUpdate ? 'Update' : 'Installed') : 'Not installed'}
                        </Pill>
                        <Btn
                          variant="primary"
                          onclick={installDeckyPlugin}
                          disabled={deckyInstalling || !dp}
                        >
                          {#snippet icon()}<Sparkles size={14} />{/snippet}
                          {deckyInstalling
                            ? 'Installing…'
                            : dp?.installed
                              ? needsUpdate
                                ? 'Update plugin'
                                : 'Reinstall'
                              : 'Install plugin'}
                        </Btn>
                      {/snippet}
                    </SettingsRow>
                  </SettingsCard>
                </div>
              {/if}

              <!-- Cloud saves (rclone) -->
              <div id="cloud-saves">
                <SettingsCard title="Cloud saves (rclone)" helper="Configure a cloud remote here, then use 'Open Ludusavi settings' to run rclone config / authenticate.">
                  
                  {#if config.cloud_provider === 'spool-server'}
                    <!-- Connected to the self-hosted Spool server (turnkey path). -->
                    <SettingsRow
                      label="Save storage"
                      status="ok"
                      helper="Saves sync to your Spool server's built-in storage."
                    >
                      {#snippet extras()}
                        <Btn variant="ghost" onclick={disconnectServerStorage} disabled={serverStorageConnecting}>
                          {#snippet icon()}<Trash2 size={14} />{/snippet}
                          {serverStorageConnecting ? 'Working…' : 'Disconnect'}
                        </Btn>
                      {/snippet}
                    </SettingsRow>
                    <SettingsRow label="Server" helper="WebDAV endpoint provided by your Spool server">
                      {#snippet control()}
                        <TextField value={config!.cloud_webdav_url} mono full readonly />
                      {/snippet}
                    </SettingsRow>
                    <SettingsRow label="Account">
                      {#snippet control()}
                        <TextField value={config!.cloud_webdav_username} mono readonly />
                      {/snippet}
                    </SettingsRow>
                  {:else}
                    {#if !config.cloud_provider || (config.cloud_provider === 'custom' && !config.cloud_remote)}
                      <div class="mx-[18px] mb-3.5 rounded-sm border border-dashed border-warn/40 bg-warn/5 p-3 text-[11.5px] text-ink-2">
                        Cloud sync is not configured — saves are backed up locally only.
                      </div>
                    {/if}

                    {#if config.sync_server_enabled && config.sync_server_url && config.sync_server_api_key}
                      <SettingsRow
                        label="Self-hosted storage"
                        helper="Sync saves to your Spool server's built-in WebDAV store — no extra setup."
                      >
                        {#snippet extras()}
                          <Btn variant="primary" onclick={useServerStorage} disabled={serverStorageConnecting}>
                            {#snippet icon()}<Sparkles size={14} />{/snippet}
                            {serverStorageConnecting ? 'Connecting…' : 'Use my Spool server for save storage'}
                          </Btn>
                        {/snippet}
                      </SettingsRow>
                    {/if}

                    <SettingsRow label="Provider" helper="Choose a cloud storage provider or Custom for a custom rclone remote name">
                      {#snippet extras()}
                        <select
                          bind:value={config!.cloud_provider}
                          onchange={persist}
                          style="color-scheme: dark"
                          class="rounded-[4px] border border-line-1 bg-bg-2 px-2 py-1 text-[11.5px] text-ink-0"
                        >
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="">Disabled</option>
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="custom">Custom (rclone remote)</option>
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="google-drive">Google Drive</option>
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="onedrive">OneDrive</option>
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="dropbox">Dropbox</option>
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="box">Box</option>
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="ftp">FTP</option>
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="smb">SMB</option>
                          <option style="background: var(--color-bg-2); color: var(--color-ink-0)" value="webdav">WebDAV</option>
                        </select>
                      {/snippet}
                    </SettingsRow>

                    {#if config.cloud_provider === 'custom'}
                      <SettingsRow label="Remote" helper="rclone remote name (e.g. bazzite, gdrive, b2)">
                        {#snippet control()}
                          <TextField bind:value={config!.cloud_remote} placeholder="bazzite" mono oncommit={persist} />
                        {/snippet}
                      </SettingsRow>
                    {/if}

                    {#if config.cloud_provider === 'webdav'}
                      <SettingsRow label="WebDAV URL" helper="e.g. https://nextcloud.example.com/remote.php/dav/files/me">
                        {#snippet control()}
                          <TextField bind:value={config!.cloud_webdav_url} placeholder="https://host/webdav" mono full />
                        {/snippet}
                      </SettingsRow>
                      <SettingsRow label="Username">
                        {#snippet control()}
                          <TextField bind:value={config!.cloud_webdav_username} placeholder="username" mono />
                        {/snippet}
                      </SettingsRow>
                      <SettingsRow label="Password" helper="Stored obscured by rclone, never saved in Spool's config">
                        {#snippet extras()}
                          <TextField bind:value={webdavPassword} masked placeholder="password" mono full />
                          <Btn
                            variant="primary"
                            onclick={connectWebdav}
                            disabled={webdavConnecting || !config!.cloud_webdav_url || !config!.cloud_webdav_username}
                          >
                            {#snippet icon()}<Check size={14} />{/snippet}
                            {webdavConnecting ? 'Connecting…' : 'Connect'}
                          </Btn>
                        {/snippet}
                      </SettingsRow>
                    {/if}

                    <SettingsRow label="Remote path" helper="Subpath on the remote where saves will be synced">
                      {#snippet control()}
                        <TextField bind:value={config!.cloud_path} placeholder="Spool/ludusavi-backup" mono oncommit={persist} />
                      {/snippet}
                    </SettingsRow>

                    <SettingsRow label="rclone binary" helper="Path to rclone executable (leave blank to let ludusavi find it)">
                      {#snippet extras()}
                        <TextField bind:value={config!.rclone_path} placeholder="rclone" mono full oncommit={persist} />
                        <Btn variant="ghost" onclick={browseRclone}>
                          {#snippet icon()}<Folder size={14} />{/snippet}
                          Browse
                        </Btn>
                      {/snippet}
                    </SettingsRow>

                    <SettingsRow label="rclone arguments" helper="Additional arguments passed to rclone calls">
                      {#snippet control()}
                        <TextField bind:value={config!.rclone_args} placeholder="--fast-list --ignore-checksum" mono oncommit={persist} />
                      {/snippet}
                    </SettingsRow>
                  {/if}

                  <div class="flex justify-end px-[18px] py-[10px] bg-bg-0">
                    <Btn variant="ghost" onclick={() => api.openLudusaviGui().catch(err => toasts.show({ kind: 'bad', label: 'LUDUSAVI', title: 'Could not open settings', sub: String(err) }))}>
                      {#snippet icon()}<Layers size={14} />{/snippet}
                      Open Ludusavi settings
                    </Btn>
                  </div>

                </SettingsCard>
              </div>

              <!-- Cover artwork section -->
              <div id="artwork">
                <SettingsCard title="Cover artwork" helper="Cover, hero, and logo art is fetched from SteamGridDB when you add a game.">
                  <!-- SteamGridDB toggle row -->
                  <div class="border-b border-dashed border-line-1">
                    <div class="flex items-center gap-[14px] px-[18px] py-[14px]">
                      <div class="flex-1">
                        <div class="flex items-center gap-2 text-[13px] font-medium text-ink-0">
                          Use SteamGridDB
                          <Pill kind={config.steamgriddb_enabled ? 'ok' : 'off'}>
                            {config.steamgriddb_enabled ? 'ON' : 'OFF'}
                          </Pill>
                        </div>
                        <div class="mt-[3px] text-[11.5px] text-ink-2">
                          {config.steamgriddb_enabled
                            ? 'Authenticated — art fetches on every game import.'
                            : 'Disabled — covers use generated placeholders.'}
                        </div>
                      </div>
                      <Toggle bind:checked={config!.steamgriddb_enabled} onchange={persist} />
                    </div>
                    {#if config.steamgriddb_enabled}
                      <div class="bg-bg-0 pb-1">
                        <SettingsRow label="API key" helper="Required for art fetches. Your key is never sent to anyone but SteamGridDB.">
                          {#snippet extras()}
                            <TextField bind:value={config!.steamgriddb_api_key} placeholder="API key" mono masked full oncommit={persist} />
                            <a
                              href="https://www.steamgriddb.com/profile/preferences/api"
                              target="_blank"
                              rel="noopener noreferrer"
                            >
                              <Btn variant="ghost">
                                {#snippet icon()}<KeyRound size={14} />{/snippet}
                                Get a key
                              </Btn>
                            </a>
                          {/snippet}
                        </SettingsRow>


                      </div>
                    {/if}
                  </div>
                </SettingsCard>
              </div>

            </div>
          </section>

          <!-- ════════════════ SHARING & SYNC GROUP ════════════════ -->
          <section class="mb-9">
            <div class="mb-3.5 border-b border-line-1 pb-2.5">
              <h2 class="font-display text-[20px] font-semibold tracking-[-0.01em] text-ink-0">Sharing & Sync</h2>
              <div class="mt-[3px] text-[12px] text-ink-2">Between your devices, and across your home network.</div>
            </div>
            <div class="flex flex-col gap-4">

              <!-- LAN sharing section -->
              <div id="lan">
                <SettingsCard title="LAN sharing" helper="Discovers other Spool instances on your local network and shares game installs over HTTP.">
                  <!-- LAN toggle -->
                  <div class="border-b border-dashed border-line-1">
                    <div class="flex items-center gap-[14px] px-[18px] py-[14px]">
                      <div class="flex-1">
                        <div class="flex items-center gap-2 text-[13px] font-medium text-ink-0">
                          Share installs over LAN
                          <Pill kind={config.lan_share_enabled ? 'ok' : 'off'}>
                            {config.lan_share_enabled ? 'ON' : 'OFF'}
                          </Pill>
                        </div>
                        <div class="mt-[3px] text-[11.5px] text-ink-2">
                          {config.lan_share_enabled
                            ? `Listening on :${config.lan_share_port} · ${peers.length} peer${peers.length === 1 ? '' : 's'} visible`
                            : 'Off — your installs stay private.'}
                        </div>
                      </div>
                      <Toggle bind:checked={config!.lan_share_enabled} onchange={persist} />
                    </div>

                    {#if config.lan_share_enabled}
                      <div class="bg-bg-0 pb-1">
                        <SettingsRow label="Port" helper="TCP port peers connect to. Falls back to a random port if taken.">
                          {#snippet control()}
                            <input
                              type="number"
                              min="1024"
                              max="65535"
                              bind:value={config!.lan_share_port}
                              onblur={onLanPortCommit}
                              class="font-mono h-7 w-24 rounded-sm border border-line-1 bg-bg-2 px-2 text-right text-[12px] text-ink-0 outline-none focus:border-line-3"
                            />
                          {/snippet}
                        </SettingsRow>

                        <SettingsRow label="Default install dir" helper="Where downloads from peers land.">
                          {#snippet extras()}
                            <TextField
                              bind:value={config!.lan_install_dir}
                              placeholder="(default · lan-games inside Spool app data)"
                              mono
                              full
                              oncommit={persist}
                            />
                            <Btn variant="ghost" onclick={browseLanInstallDir}>
                              {#snippet icon()}<Folder size={14} />{/snippet}
                              Browse
                            </Btn>
                          {/snippet}
                        </SettingsRow>

                        <SettingsRow label="Download speed limit"
                          helper={config.lan_download_max_mbps > 0
                            ? `Capped at ${config.lan_download_max_mbps} Mbps across all parallel files.`
                            : 'Unlimited — transfers use whatever bandwidth they can get.'}>
                          {#snippet control()}
                            <div class="flex items-center gap-1.5">
                              <input
                                type="number"
                                min="0"
                                step="1"
                                bind:value={config!.lan_download_max_mbps}
                                onblur={() => {
                                  if (!config) return;
                                  if (!Number.isFinite(config.lan_download_max_mbps) || config.lan_download_max_mbps < 0) {
                                    config.lan_download_max_mbps = 0;
                                  }
                                  persist();
                                }}
                                class="font-mono h-7 w-20 rounded-sm border border-line-1 bg-bg-2 px-2 text-right text-[12px] text-ink-0 outline-none focus:border-line-3"
                              />
                              <span class="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-3">Mbps</span>
                            </div>
                          {/snippet}
                        </SettingsRow>

                        <!-- Peer list preview -->
                        {#if peers.length > 0}
                          <div class="mx-[18px] mb-3.5 overflow-hidden rounded-sm border border-dashed border-line-2 bg-bg-0">
                            <div class="flex items-center justify-between border-b border-dashed border-line-1 px-3 py-2">
                              <MonoLabel size={9.5}>Discovered peers</MonoLabel>
                              <span class="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-3">
                                UDP · BROADCAST · {config.lan_share_port}
                              </span>
                            </div>
                            {#each peers as peer, i (peer.device_id)}
                              <div
                                class="grid items-center gap-3 px-3 py-2"
                                style="grid-template-columns: 14px 1fr auto auto auto"
                                class:border-b={i < peers.length - 1}
                                class:border-dashed={i < peers.length - 1}
                                class:border-line-1={i < peers.length - 1}
                              >
                                <span
                                  class="size-[7px] rounded-full"
                                  style:background="var(--color-ok)"
                                  style:box-shadow="0 0 8px rgba(126,226,164,0.4)"
                                ></span>
                                <div class="flex min-w-0 flex-col gap-px">
                                  <span class="truncate text-[12px] text-ink-0">{peer.device_name}</span>
                                  <span class="font-mono text-[9.5px] uppercase tracking-[0.06em] text-ink-3">
                                    {peer.game_count} games
                                  </span>
                                </div>
                                <span class="font-mono text-[10px] text-ink-2">{peer.addr}</span>
                                <span class="font-mono text-[10px] text-ink-3">
                                  {peer.last_seen_ago_secs < 5 ? 'now' : `${peer.last_seen_ago_secs}s ago`}
                                </span>
                                <Pill kind="info" soft>peer</Pill>
                              </div>
                            {/each}
                          </div>
                        {/if}
                      </div>
                    {/if}
                  </div>
                </SettingsCard>
              </div>

              <!-- Sync server section -->
              <div id="sync">
                <SettingsCard title="Sync server" helper="A small HTTP service that holds a per-game lock so two devices don't fight over saves.">
                  <div class="border-b border-dashed border-line-1">
                    <div class="flex items-center gap-[14px] px-[18px] py-[14px]">
                      <div class="flex-1">
                        <div class="flex items-center gap-2 text-[13px] font-medium text-ink-0">
                          Use a sync server
                          <Pill kind={config.sync_server_enabled ? (syncStatus.reachability === 'online' ? 'ok' : 'warn') : 'off'}>
                            {config.sync_server_enabled ? (syncStatus.reachability === 'online' ? 'Online' : 'Offline') : 'OFF'}
                          </Pill>
                        </div>
                        <div class="mt-[3px] text-[11.5px] text-ink-2">
                          {#if config.sync_server_enabled && syncStatus.reachability === 'online'}
                            {config.sync_server_url}{syncStatus.server_version ? ` · v${syncStatus.server_version}` : ''}
                          {:else if config.sync_server_enabled && syncStatus.reachability === 'offline'}
                            Unreachable · {syncStatus.error ?? 'no response'}
                          {:else if config.sync_server_enabled}
                            Configure a server URL below.
                          {:else}
                            Off — you'll only get local backups.
                          {/if}
                        </div>
                      </div>
                      <Toggle bind:checked={config!.sync_server_enabled} onchange={persistAndRefresh} />
                    </div>

                    {#if config.sync_server_enabled}
                      <div class="bg-bg-0 pb-1">
                        <SettingsRow label="Server URL"
                          helper={syncStatus.reachability === 'online'
                            ? `Online${syncStatus.server_version ? ` · v${syncStatus.server_version}` : ''}`
                            : syncStatus.reachability === 'offline'
                              ? `Unreachable · ${syncStatus.error ?? 'no response'}`
                              : 'Not yet configured.'}
                          status={syncStatus.reachability === 'online' ? 'ok' : syncStatus.reachability === 'offline' ? 'warn' : undefined}
                        >
                          {#snippet extras()}
                            <TextField
                              bind:value={config!.sync_server_url}
                              placeholder="http://raspberrypi.local:47633"
                              mono
                              full
                              oncommit={persistAndRefresh}
                            />
                            <Btn variant="ghost" onclick={refreshSync}>
                              {#snippet icon()}<RefreshCcw size={14} />{/snippet}
                              Check
                            </Btn>
                          {/snippet}
                        </SettingsRow>

                        <SettingsRow label="API key" helper="Generated once when you register. On your other devices, paste this same key — don't register again.">
                          {#snippet extras()}
                            <TextField
                              bind:value={config!.sync_server_api_key}
                              placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                              mono
                              masked
                              full
                              oncommit={persistAndRefresh}
                            />
                            <Btn variant="ghost" onclick={() => (registerOpen = !registerOpen)}>
                              {#snippet icon()}<KeyRound size={14} />{/snippet}
                              {registerOpen ? 'Cancel' : 'Register…'}
                            </Btn>
                          {/snippet}
                        </SettingsRow>

                        {#if registerOpen}
                          <div class="border-l-2 border-spool/40 bg-bg-2/40 mx-[18px] mb-3 px-4 py-3">
                            <div class="font-mono mb-2 text-[10px] uppercase tracking-[0.1em] text-spool">Register new account</div>
                            <div class="mb-3 rounded-sm border border-dashed border-spool/40 bg-spool/5 p-2.5 text-[11px] leading-[1.45] text-ink-2">
                              <span class="font-medium text-ink-1">Do this once.</span> Your account is shared across all your devices. On your other PCs, paste this same API key instead of registering again — that's what makes saves sync and play-locking work together.
                            </div>
                            <p class="mb-3 text-[11.5px] leading-[1.45] text-ink-2">
                              Enter the admin secret you set in the server's compose file, and a name for your account.
                              The server returns an API key that gets pasted in automatically — reuse that same key on your other devices.
                            </p>
                            <div class="flex flex-col gap-2">
                              <div class="flex items-center gap-2">
                                <span class="font-mono w-[120px] shrink-0 text-[10.5px] uppercase tracking-[0.08em] text-ink-2">Admin secret</span>
                                <TextField bind:value={registerAdminSecret} placeholder="ADMIN_SECRET from docker-compose.yml" mono masked full />
                              </div>
                              <div class="flex items-center gap-2">
                                <span class="font-mono w-[120px] shrink-0 text-[10.5px] uppercase tracking-[0.08em] text-ink-2">Account name</span>
                                <TextField bind:value={registerUsername} placeholder="me" mono full />
                              </div>
                              <div class="mt-1 flex justify-end">
                                <Btn onclick={submitRegister} disabled={registerSubmitting}>
                                  {#snippet icon()}<KeyRound size={14} />{/snippet}
                                  {registerSubmitting ? 'Registering…' : 'Register'}
                                </Btn>
                              </div>
                            </div>
                          </div>
                        {/if}
                      </div>
                    {/if}
                  </div>
                </SettingsCard>
              </div>

              <!-- This device section -->
              <div id="device">
                <SettingsCard title="This device" helper="The label other Spool devices see in their peer list.">
                  <SettingsRow label="Device name">
                    {#snippet control()}
                      <TextField bind:value={config!.device_name} placeholder="Workshop · Desktop" oncommit={persist} />
                    {/snippet}
                  </SettingsRow>
                </SettingsCard>
              </div>

            </div>
          </section>

          <!-- ════════════════ ADVANCED GROUP ════════════════ -->
          <section class="mb-9">
            <div class="mb-3.5 border-b border-line-1 pb-2.5">
              <h2 class="font-display text-[20px] font-semibold tracking-[-0.01em] text-ink-0">Advanced</h2>
              <div class="mt-[3px] text-[12px] text-ink-2">Maintenance and escape hatches.</div>
            </div>
            <SettingsCard title="Reset" helper="Lose all settings and start over. The library and your saves stay.">
              <div class="flex items-center gap-2 px-[18px] py-4 flex-wrap">
                <Btn
                  variant="ghost"
                  onclick={async () => {
                    try {
                      const dir = await appLocalDataDir();
                      await api.openPath(dir);
                    } catch (e) {
                      toasts.show({ kind: 'bad', label: 'SETTINGS', title: "Couldn't open config folder", sub: String(e) });
                    }
                  }}
                >
                  {#snippet icon()}<Folder size={14} />{/snippet}
                  Open config folder
                </Btn>
                <Btn
                  variant="ghost"
                  onclick={async () => {
                    if (!config) return;
                    try {
                      await navigator.clipboard.writeText(JSON.stringify($state.snapshot(config), null, 2));
                      toasts.show({ kind: 'ok', label: 'SETTINGS', title: 'Copied', sub: 'Config JSON copied to clipboard.' });
                    } catch (e) {
                      toasts.show({ kind: 'bad', label: 'SETTINGS', title: "Couldn't copy", sub: String(e) });
                    }
                  }}
                >
                  Copy diagnostics
                </Btn>
              </div>
            </SettingsCard>
          </section>

          <!-- Footer note -->
          <p class="pb-4 text-[11px] text-ink-3">
            Settings live at
            <code class="font-mono text-ink-2">%LOCALAPPDATA%\Spool\config.json</code> ·
            Changes save automatically.
          </p>

        </div>
      </div>
    </div>
  {/if}
</div>
