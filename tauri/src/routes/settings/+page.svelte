<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Check,
    Folder,
    KeyRound,
    Layers,
    Library,
    Plus,
    RefreshCcw,
    Sparkles,
    Trash2,
    Wifi,
  } from '@lucide/svelte';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { appLocalDataDir } from '@tauri-apps/api/path';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import type { ConfigData, LanPeer, ProtonVersion, SyncStatus } from '$lib/types';
  import WindowChrome from '$lib/components/WindowChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import Pill from '$lib/components/Pill.svelte';
  import TextField from '$lib/components/TextField.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import SettingsCard from '$lib/components/SettingsCard.svelte';
  import SettingsRow from '$lib/components/SettingsRow.svelte';

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

  let torboxPinging = $state(false);
  let torboxLastPing = $state<{ ok: boolean; message: string } | null>(null);
  let newSourceUrl = $state('');
  let addingSource = $state(false);

  // Proton / Compatibility (Linux only).
  let isLinux = $state(false);
  let protonVersions = $state<ProtonVersion[]>([]);

  onMount(async () => {
    try {
      config = await api.getConfig();
      syncStatus = await api.currentSyncStatus();
      peers = await api.listLanPeers();
      isLinux = (await api.appPlatform()) === 'linux';
      if (isLinux) protonVersions = await api.listProtonVersions();
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

  async function persist() {
    if (!config) return;
    try {
      config = await api.updateConfig($state.snapshot(config));
    } catch (e) {
      error = String(e);
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

  async function autoDetectUmu() {
    if (!config) return;
    const found = await api.detectUmuRun();
    if (found) config.umu_run_path = found;
    config = await api.getConfig();
  }

  async function browseUmu() {
    const picked = await openDialog({ title: 'Locate umu-run', multiple: false });
    if (typeof picked === 'string' && config) {
      config.umu_run_path = picked;
      await persist();
    }
  }

  async function browseLanInstallDir() {
    const picked = await openDialog({ title: 'Pick the LAN install folder', directory: true, multiple: false });
    if (typeof picked === 'string' && config) {
      config.lan_install_dir = picked;
      await persist();
    }
  }

  async function browseDownloadDir() {
    const picked = await openDialog({ title: 'Pick the TorBox download folder', directory: true, multiple: false });
    if (typeof picked === 'string' && config) {
      config.download_dir = picked;
      await persist();
    }
  }

  async function addSourceFeed() {
    if (!config || !newSourceUrl.trim()) return;
    addingSource = true;
    try {
      const list = await api.hydraAddSource(newSourceUrl.trim());
      config.download_sources = list;
      newSourceUrl = '';
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'SOURCE', title: "Couldn't add feed", sub: String(e) });
    } finally {
      addingSource = false;
    }
  }

  async function removeSourceFeed(url: string) {
    if (!config) return;
    try {
      const list = await api.hydraRemoveSource(url);
      config.download_sources = list;
    } catch (e) {
      console.error('[settings] remove source failed:', e);
    }
  }

  async function testTorBoxConnection() {
    if (!config) return;
    torboxPinging = true;
    torboxLastPing = null;
    try {
      await api.torboxPing();
      torboxLastPing = { ok: true, message: 'API key works — TorBox is reachable.' };
    } catch (e) {
      torboxLastPing = { ok: false, message: String(e) };
    } finally {
      torboxPinging = false;
    }
  }

  function onLanPortCommit() {
    if (!config) return;
    if (!Number.isFinite(config.lan_share_port) || config.lan_share_port < 1024) config.lan_share_port = 47632;
    if (config.lan_share_port > 65535) config.lan_share_port = 65535;
    persist();
  }

  function scrollToSection(id: string) {
    activeSection = id;
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  const NAV_GROUPS = $derived([
    {
      id: 'library',
      title: 'Library',
      items: [
        { id: 'ludusavi', title: 'Ludusavi', sub: 'Save backup engine' },
        ...(isLinux
          ? [{ id: 'compat', title: 'Compatibility', sub: 'Proton / umu-run' }]
          : []),
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
    {
      id: 'sources',
      title: 'Sources & Downloads',
      items: [
        { id: 'hydra', title: 'Source feeds', sub: 'Hydra JSON URLs' },
        { id: 'torbox', title: 'TorBox', sub: 'Debrid download provider' },
      ],
    },
  ]);
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <WindowChrome sub="SETTINGS" />

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
              {#if group.id === 'library'}
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

                </SettingsCard>
              </div>

              <!-- Compatibility (Proton) — Linux only -->
              {#if isLinux}
                <div id="compat">
                  <SettingsCard title="Compatibility (Proton)" helper="Run Windows games on Linux via Proton. umu-run manages the runtime; each game gets its own prefix.">
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
                            ? `Capped at ${config.lan_download_max_mbps} MB/s across all parallel files.`
                            : 'Unlimited — transfers use whatever bandwidth they can get.'}>
                          {#snippet control()}
                            <div class="flex items-center gap-1.5">
                              <input
                                type="number"
                                min="0"
                                step="0.5"
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
                              <span class="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-3">MB/s</span>
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

                        <SettingsRow label="API key" helper="Generated when you register an account on the server.">
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
                            <p class="mb-3 text-[11.5px] leading-[1.45] text-ink-2">
                              Enter the admin secret you set in the server's compose file, plus a username for this device.
                              The server returns an API key that gets pasted in automatically.
                            </p>
                            <div class="flex flex-col gap-2">
                              <div class="flex items-center gap-2">
                                <span class="font-mono w-[120px] shrink-0 text-[10.5px] uppercase tracking-[0.08em] text-ink-2">Admin secret</span>
                                <TextField bind:value={registerAdminSecret} placeholder="ADMIN_SECRET from docker-compose.yml" mono masked full />
                              </div>
                              <div class="flex items-center gap-2">
                                <span class="font-mono w-[120px] shrink-0 text-[10.5px] uppercase tracking-[0.08em] text-ink-2">Username</span>
                                <TextField bind:value={registerUsername} placeholder="my-pc" mono full />
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

          <!-- ════════════════ SOURCES & DOWNLOADS GROUP ════════════════ -->
          <section class="mb-9">
            <div class="mb-3.5 border-b border-line-1 pb-2.5">
              <h2 class="font-display text-[20px] font-semibold tracking-[-0.01em] text-ink-0">Sources & Downloads</h2>
              <div class="mt-[3px] text-[12px] text-ink-2">Where to find new games, and how to fetch them.</div>
            </div>
            <div class="flex flex-col gap-4">

              <!-- Source feeds section -->
              <div id="hydra">
                <SettingsCard title="Source feeds" helper="Hydra-compatible JSON feeds. The Browse Games window aggregates everything listed here.">
                  <div class="px-[18px] py-3">
                    <!-- Add new feed -->
                    <div class="flex gap-2">
                      <TextField
                        bind:value={newSourceUrl}
                        placeholder="https://example.com/source.json"
                        mono
                        full
                      />
                      <Btn
                        onclick={addSourceFeed}
                        disabled={addingSource || !newSourceUrl.trim()}
                      >
                        {#snippet icon()}<Plus size={14} />{/snippet}
                        {addingSource ? 'Adding…' : 'Add'}
                      </Btn>
                    </div>

                    <!-- Feed list -->
                    {#if config.download_sources.length > 0}
                      <div class="mt-2.5 overflow-hidden rounded-sm border border-line-1 bg-bg-0">
                        {#each config.download_sources as url, i (url)}
                          <div
                            class="flex items-center gap-2.5 px-3 py-2"
                            class:border-b={i < config.download_sources.length - 1}
                            class:border-dashed={i < config.download_sources.length - 1}
                            class:border-line-1={i < config.download_sources.length - 1}
                          >
                            <span class="size-1.5 shrink-0 rounded-full bg-ok"></span>
                            <code class="font-mono min-w-0 flex-1 truncate text-[11px] text-ink-1" title={url}>{url}</code>
                            <button
                              onclick={() => removeSourceFeed(url)}
                              title="Remove feed"
                              class="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-ink-3 transition-colors hover:bg-white/10 hover:text-bad"
                            >
                              <Trash2 size={12} />
                            </button>
                          </div>
                        {/each}
                      </div>
                    {:else}
                      <div class="mt-2.5 rounded-sm border border-dashed border-line-1 px-3 py-4 text-center">
                        <p class="font-mono text-[11px] uppercase tracking-[0.08em] text-ink-3">No feeds configured</p>
                      </div>
                    {/if}

                    <div class="mt-2.5 flex items-center justify-between text-[11px] text-ink-3">
                      <span>{config.download_sources.length} feed{config.download_sources.length === 1 ? '' : 's'}</span>
                      <span class="text-ink-2">Refreshed on every Browse Games open</span>
                    </div>
                  </div>
                </SettingsCard>
              </div>

              <!-- TorBox section -->
              <div id="torbox">
                <SettingsCard title="TorBox" helper="Cloud debrid service. Spool fetches files via your TorBox account when you click Download.">
                  <div class="border-b border-dashed border-line-1">
                    <div class="flex items-center gap-[14px] px-[18px] py-[14px]">
                      <div class="flex-1">
                        <div class="flex items-center gap-2 text-[13px] font-medium text-ink-0">
                          Enable TorBox
                          <Pill kind={config.torbox_enabled ? 'ok' : 'off'}>
                            {config.torbox_enabled ? 'Linked' : 'OFF'}
                          </Pill>
                        </div>
                        <div class="mt-[3px] text-[11.5px] text-ink-2">
                          {config.torbox_enabled
                            ? 'Debrid downloads active — magnet URIs go through TorBox.'
                            : 'Off — Browse Games downloads fall back to direct links only.'}
                        </div>
                      </div>
                      <Toggle bind:checked={config!.torbox_enabled} onchange={persist} />
                    </div>

                    {#if config.torbox_enabled}
                      <div class="bg-bg-0 pb-1">
                        <SettingsRow
                          label="API key"
                          helper={torboxLastPing ? torboxLastPing.message : 'Generate at torbox.app — Account → API.'}
                          status={torboxLastPing ? (torboxLastPing.ok ? 'ok' : 'warn') : undefined}
                        >
                          {#snippet extras()}
                            <TextField
                              bind:value={config!.torbox_api_key}
                              placeholder="Paste TorBox key…"
                              mono
                              masked
                              full
                              oncommit={() => { torboxLastPing = null; persist(); }}
                            />
                            <Btn
                              variant="ghost"
                              onclick={testTorBoxConnection}
                              disabled={torboxPinging || !config!.torbox_api_key}
                            >
                              {#snippet icon()}<Check size={14} />{/snippet}
                              {torboxPinging ? 'Testing…' : 'Test'}
                            </Btn>
                          {/snippet}
                        </SettingsRow>

                        <SettingsRow label="Download to" helper="Where TorBox-fetched games land before they're installed.">
                          {#snippet extras()}
                            <TextField
                              bind:value={config!.download_dir}
                              placeholder="(default · ~/Downloads)"
                              mono
                              full
                              oncommit={persist}
                            />
                            <Btn variant="ghost" onclick={browseDownloadDir}>
                              {#snippet icon()}<Folder size={14} />{/snippet}
                              Browse
                            </Btn>
                          {/snippet}
                        </SettingsRow>
                      </div>
                    {/if}
                  </div>
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
                      await openPath(dir);
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
