<script lang="ts">
  /**
   * Settings — single-page layout, live save on commit.
   *
   * Scope v1: just the Library group (ludusavi, SteamGridDB cover art,
   * device name). Sharing/Sync and Sources/Downloads groups are modelled
   * in the backend ConfigData (so config.json round-trips with the C#
   * app), but no UI surfaces them yet — they ship with their respective
   * features in v2.
   */
  import { onMount } from 'svelte';
  import { ChevronLeft, Folder, KeyRound, Sparkles } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import type { ConfigData, SyncStatus } from '$lib/types';
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
  let ludusaviOk = $derived(
    config !== null && config.ludusavi_path.length > 0,
  );

  // ── Sync server state — current reachability + register form ─────
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

  onMount(async () => {
    try {
      config = await api.getConfig();
      syncStatus = await api.currentSyncStatus();
    } catch (e) {
      error = String(e);
    }
  });

  /** Tell the backend to immediately re-poll /health so the badge
   *  updates without waiting for the next 30s tick. Called after
   *  the user edits the URL / API key fields. */
  async function refreshSync() {
    try {
      syncStatus = await api.refreshSyncStatus();
    } catch (e) {
      console.error('[settings] refreshSyncStatus failed:', e);
    }
  }

  /** Persist + immediately re-probe so the user sees status update. */
  async function persistAndRefresh() {
    await persist();
    await refreshSync();
  }

  async function submitRegister() {
    if (!config) return;
    const url = config.sync_server_url.trim();
    if (!url) {
      toasts.show({
        kind: 'warn',
        label: 'SYNC',
        title: 'Server URL required',
        sub: 'Set the URL above before registering.',
      });
      return;
    }
    if (!registerAdminSecret.trim() || !registerUsername.trim()) {
      toasts.show({
        kind: 'warn',
        label: 'SYNC',
        title: 'Missing fields',
        sub: 'Admin secret and username are both required.',
      });
      return;
    }
    registerSubmitting = true;
    try {
      const apiKey = await api.syncRegisterAccount(
        url,
        registerAdminSecret.trim(),
        registerUsername.trim(),
      );
      config.sync_server_api_key = apiKey;
      config.sync_server_enabled = true;
      await persistAndRefresh();
      registerAdminSecret = '';
      registerUsername = '';
      registerOpen = false;
      toasts.show({
        kind: 'ok',
        label: 'SYNC',
        title: 'Registered',
        sub: 'API key filled in. Sync server is now configured.',
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'SYNC · REGISTER',
        title: "Couldn't register",
        sub: String(e),
      });
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
    if (found) {
      config.ludusavi_path = found;
    }
    // The Rust side has already persisted if found; pull fresh state to be safe.
    config = await api.getConfig();
  }

  async function browseLudusavi() {
    const picked = await openDialog({
      title: 'Locate ludusavi executable',
      multiple: false,
      filters: [
        { name: 'Executable', extensions: ['exe', ''] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (typeof picked === 'string' && config) {
      config.ludusavi_path = picked;
      await persist();
    }
  }

  async function browseLanInstallDir() {
    const picked = await openDialog({
      title: 'Pick the LAN install folder',
      directory: true,
      multiple: false,
    });
    if (typeof picked === 'string' && config) {
      config.lan_install_dir = picked;
      await persist();
    }
  }

  /**
   * Validates the LAN port input. Empty / invalid → fall back to 47632
   * (the default). The HTTP server's bind logic already falls back to
   * ephemeral on collision so the only real failure mode here is a
   * non-numeric string — which the type="number" input mostly prevents.
   */
  function onLanPortCommit() {
    if (!config) return;
    if (!Number.isFinite(config.lan_share_port) || config.lan_share_port < 1024) {
      config.lan_share_port = 47632;
    }
    if (config.lan_share_port > 65535) {
      config.lan_share_port = 65535;
    }
    persist();
  }
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <WindowChrome sub="SETTINGS" />

  <main class="flex flex-1 flex-col overflow-hidden">
    <!-- Toolbar: back link + section eyebrow -->
    <header class="flex items-center gap-3 border-b border-line-1 px-6 py-3">
      <a
        href="/"
        class="inline-flex items-center gap-1.5 text-[12.5px] font-medium text-ink-2 transition-colors hover:text-ink-0"
      >
        <ChevronLeft size={14} />
        Library
      </a>
      <span class="text-ink-3">·</span>
      <MonoLabel size={11}>Settings</MonoLabel>
    </header>

    <!-- Body -->
    <div class="flex-1 overflow-auto px-6 py-6">
      {#if error}
        <div class="rounded-md border border-bad/40 bg-bad/10 p-4 text-sm text-bad">
          <div class="mb-1 font-medium">Failed to load settings</div>
          <code class="font-mono text-[11px] opacity-80">{error}</code>
        </div>
      {:else if !config}
        <p class="font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">Loading…</p>
      {:else}
        <div class="mx-auto flex max-w-2xl flex-col gap-5">
          <SettingsCard title="Library">
            <!-- Ludusavi -->
            <SettingsRow title="Ludusavi" subtitle="Save backup engine">
              {#snippet control()}
                {#if ludusaviOk}
                  <Pill kind="ok">Found</Pill>
                {:else}
                  <Pill kind="warn">Not set</Pill>
                {/if}
              {/snippet}
              {#snippet extras()}
                <TextField
                  bind:value={config!.ludusavi_path}
                  placeholder="C:\path\to\ludusavi.exe"
                  mono
                  full
                  oncommit={persist}
                />
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

            <!-- SteamGridDB -->
            <SettingsRow
              title="Cover artwork"
              subtitle="Fetch art from SteamGridDB when adding games"
            >
              {#snippet control()}
                <Toggle
                  bind:checked={config!.steamgriddb_enabled}
                  onchange={persist}
                  aria-label="SteamGridDB enabled"
                />
              {/snippet}
              {#snippet extras()}
                {#if config!.steamgriddb_enabled}
                  <TextField
                    bind:value={config!.steamgriddb_api_key}
                    placeholder="API key"
                    mono
                    masked
                    full
                    oncommit={persist}
                  />
                {/if}
              {/snippet}
            </SettingsRow>

            <!-- Device name -->
            <SettingsRow title="Device name" subtitle="How this machine identifies itself">
              {#snippet control()}
                <TextField
                  bind:value={config!.device_name}
                  placeholder="Workshop · Desktop"
                  oncommit={persist}
                />
              {/snippet}
            </SettingsRow>
          </SettingsCard>

          <SettingsCard title="Sync server">
            <!-- Enable toggle -->
            <SettingsRow
              title="Cloud save sync"
              subtitle="Acquire a play-state lock so two devices can't play the same game at once, and record save backup / restore events for cross-device sync."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={config!.sync_server_enabled}
                  onchange={persistAndRefresh}
                  aria-label="Sync server enabled"
                />
              {/snippet}
            </SettingsRow>

            {#if config.sync_server_enabled}
              <!-- URL field -->
              <SettingsRow
                title="Server URL"
                subtitle={syncStatus.reachability === 'online'
                  ? `Online${syncStatus.server_version ? ` · v${syncStatus.server_version}` : ''}`
                  : syncStatus.reachability === 'offline'
                    ? `Unreachable · ${syncStatus.error ?? 'no response'}`
                    : 'Not yet configured.'}
              >
                {#snippet control()}
                  {#if syncStatus.reachability === 'online'}
                    <Pill kind="ok">Online</Pill>
                  {:else if syncStatus.reachability === 'offline'}
                    <Pill kind="warn">Offline</Pill>
                  {:else}
                    <Pill kind="warn">Set URL</Pill>
                  {/if}
                {/snippet}
                {#snippet extras()}
                  <TextField
                    bind:value={config!.sync_server_url}
                    placeholder="http://raspberrypi.local:47633"
                    mono
                    full
                    oncommit={persistAndRefresh}
                  />
                {/snippet}
              </SettingsRow>

              <!-- API key field -->
              <SettingsRow
                title="API key"
                subtitle="Generated when you register an account on the server. The Register button below fills this in for you."
              >
                {#snippet extras()}
                  <TextField
                    bind:value={config!.sync_server_api_key}
                    placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                    mono
                    masked
                    full
                    oncommit={persistAndRefresh}
                  />
                  <Btn
                    variant="ghost"
                    onclick={() => (registerOpen = !registerOpen)}
                  >
                    {#snippet icon()}<KeyRound size={14} />{/snippet}
                    {registerOpen ? 'Cancel' : 'Register…'}
                  </Btn>
                {/snippet}
              </SettingsRow>

              {#if registerOpen}
                <!-- Inline register form. Slides in under the API key row. -->
                <div
                  class="border-l-2 border-spool/40 bg-bg-2/40 px-4 py-3"
                  style:margin-left="-4px"
                >
                  <div
                    class="font-mono mb-2 text-[10px] uppercase tracking-[0.1em] text-spool"
                  >
                    Register new account
                  </div>
                  <p class="mb-3 text-[11.5px] leading-[1.45] text-ink-2">
                    Enter the admin secret you set in the server's compose file,
                    plus a username for this device. The server returns an API
                    key that gets pasted into the field above automatically.
                  </p>
                  <div class="flex flex-col gap-2">
                    <div class="flex items-center gap-2">
                      <span
                        class="font-mono w-[120px] text-[10.5px] uppercase tracking-[0.08em] text-ink-2"
                      >
                        Admin secret
                      </span>
                      <TextField
                        bind:value={registerAdminSecret}
                        placeholder="ADMIN_SECRET from docker-compose.yml"
                        mono
                        masked
                        full
                      />
                    </div>
                    <div class="flex items-center gap-2">
                      <span
                        class="font-mono w-[120px] text-[10.5px] uppercase tracking-[0.08em] text-ink-2"
                      >
                        Username
                      </span>
                      <TextField
                        bind:value={registerUsername}
                        placeholder="my-pc"
                        mono
                        full
                      />
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
            {/if}
          </SettingsCard>

          <SettingsCard title="Sharing">
            <!-- LAN toggle -->
            <SettingsRow
              title="Share over LAN"
              subtitle="Let other Spool devices on your network see this device and the games you flag for sharing."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={config!.lan_share_enabled}
                  onchange={persist}
                  aria-label="LAN sharing enabled"
                />
              {/snippet}
            </SettingsRow>

            {#if config.lan_share_enabled}
              <!-- Port -->
              <SettingsRow
                title="HTTP port"
                subtitle="The transfer server's preferred port. Falls back to a random port if taken."
              >
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

              <!-- Install dir -->
              <SettingsRow
                title="Install folder"
                subtitle="Where games downloaded from peers land. Default: %LOCALAPPDATA%\Spool\lan-games."
              >
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

              <!-- Bandwidth throttle -->
              <SettingsRow
                title="Download speed limit"
                subtitle={config!.lan_download_max_mbps > 0
                  ? `Capped at ${config!.lan_download_max_mbps} MB/s across all parallel files.`
                  : "Unlimited — LAN transfers will use whatever bandwidth they can get."}
              >
                {#snippet control()}
                  <div class="flex items-center gap-1.5">
                    <input
                      type="number"
                      min="0"
                      step="0.5"
                      bind:value={config!.lan_download_max_mbps}
                      onblur={() => {
                        if (!config) return;
                        if (
                          !Number.isFinite(config.lan_download_max_mbps) ||
                          config.lan_download_max_mbps < 0
                        ) {
                          config.lan_download_max_mbps = 0;
                        }
                        persist();
                      }}
                      class="font-mono h-7 w-20 rounded-sm border border-line-1 bg-bg-2 px-2 text-right text-[12px] text-ink-0 outline-none focus:border-line-3"
                    />
                    <span
                      class="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-3"
                    >
                      MB/s
                    </span>
                  </div>
                {/snippet}
              </SettingsRow>
            {/if}

            <SettingsRow
              title="Per-game opt-in"
              subtitle="Open a game's Edit dialog → Sharing tab to flag it for LAN sharing. Off by default."
            >
              {#snippet control()}
                <span class="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-3">
                  Per game
                </span>
              {/snippet}
            </SettingsRow>
          </SettingsCard>

          <p class="px-1 text-[11px] text-ink-3">
            Changes save automatically. Settings live at
            <code class="font-mono text-ink-2">%LOCALAPPDATA%\Spool\config.json</code>.
          </p>
        </div>
      {/if}
    </div>
  </main>
</div>
