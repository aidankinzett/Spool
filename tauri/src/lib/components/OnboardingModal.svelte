<script lang="ts">
  /**
   * First-run onboarding — a four-step flow presented as a modal over the
   * (empty) library on a fresh install:
   *
   *   Welcome → Cover art → Cloud saves → Game Mode companion (Linux + Decky)
   *
   * The Decky step only appears on Linux when Decky Loader is present, mirroring
   * the design's `decky_present` gate. The cloud step connects for real inline
   * (OAuth authorize / WebDAV credentials / custom rclone remote), reusing the
   * same backend commands as Settings → Saves. Cloud and Decky are skippable.
   *
   * Presentation is a modal rather than a separate window so it behaves
   * identically in the desktop and Game Mode (touch) layouts — the same reason
   * `CloudConflictModal` and the splash share this overlay shape. On finish it
   * persists `onboarding_completed` and calls `onfinish`, where the parent
   * dissolves the scrim and shows the "all set" toast.
   *
   * Ported from the Spool Onboarding design (Space Grotesk / Geist / JetBrains
   * Mono, graphite surface, brand spool accent).
   */
  import { onMount } from 'svelte';
  import { registerModal, unregisterModal, modalZIndex } from '$lib/modalStack.svelte';
  import {
    Check,
    ChevronLeft,
    Cloud,
    Disc3,
    Download,
    ExternalLink,
    Gamepad2,
    Grid2x2,
    KeyRound,
    Shield,
    Sparkles,
  } from '@lucide/svelte';
  import { api, type DeckyPluginInfo } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import type { ConfigData } from '$lib/types';
  import Btn from '$lib/components/Btn.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import TextField from '$lib/components/TextField.svelte';
  import Pill from '$lib/components/Pill.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import SpoolMark from '$lib/components/SpoolMark.svelte';
  import { gamepadScope } from '$lib/gamepad';

  const ACCENT = '#d7c9a0'; // brand spool — onboarding has no per-game cover

  let { onfinish }: { onfinish: () => void } = $props();

  const _modalId = Symbol();
  const zIndex = $derived(modalZIndex(_modalId));

  // ── Loaded state ────────────────────────────────────────────────────────
  let config = $state<ConfigData | null>(null);
  let isLinux = $state(false);
  let deckyPlugin = $state<DeckyPluginInfo | null>(null);

  // ── Stepper ─────────────────────────────────────────────────────────────
  let idx = $state(0);
  let finishing = $state(false);

  // Decky step only when Linux + Decky Loader is actually present (matches the
  // design's decky_present gate; degrades to a 3-step flow otherwise).
  const deckyPresent = $derived(
    isLinux && !!deckyPlugin?.supported && !!deckyPlugin?.deckyPresent,
  );
  type StepId = 'welcome' | 'covers' | 'cloud' | 'decky';
  const steps = $derived<StepId[]>([
    'welcome',
    'covers',
    'cloud',
    ...(deckyPresent ? (['decky'] as StepId[]) : []),
  ]);
  const stepId = $derived(steps[Math.min(idx, steps.length - 1)]);
  const isLast = $derived(idx === steps.length - 1);
  const skippable = $derived(stepId === 'cloud' || stepId === 'decky');
  const primaryLabel = $derived(
    stepId === 'welcome' ? 'Get started' : isLast ? 'Finish' : 'Continue',
  );

  // ── Cloud step ──────────────────────────────────────────────────────────
  const OAUTH_PROVIDERS = ['google-drive', 'onedrive', 'dropbox', 'box'];
  const CLOUD_PROVIDERS = [
    { v: 'google-drive', l: 'Google Drive', note: 'Sign in with Google', kind: 'oauth' },
    { v: 'onedrive', l: 'OneDrive', note: 'Sign in with Microsoft', kind: 'oauth' },
    { v: 'dropbox', l: 'Dropbox', note: 'Sign in with Dropbox', kind: 'oauth' },
    { v: 'webdav', l: 'WebDAV', note: 'Nextcloud, ownCloud…', kind: 'webdav' },
    { v: 'custom', l: 'Custom', note: 'Existing rclone remote', kind: 'custom' },
  ] as const;

  let cloudConnecting = $state(false);
  let cloudConnected = $state(false);
  let webdavPassword = $state('');

  const chosenProvider = $derived(
    CLOUD_PROVIDERS.find((p) => p.v === config?.cloud_provider) ?? null,
  );

  // ── Decky step ──────────────────────────────────────────────────────────
  let deckyInstalling = $state(false);
  const deckyInstalled = $derived(!!deckyPlugin?.installed);

  onMount(() => {
    registerModal(_modalId);
    return () => unregisterModal(_modalId);
  });

  onMount(async () => {
    try {
      config = await api.getConfig();
      isLinux = (await api.appPlatform()) === 'linux';
      if (isLinux) {
        try {
          deckyPlugin = await api.deckyPluginStatus();
        } catch (e) {
          console.error('[onboarding] deckyPluginStatus failed:', e);
        }
      }
      if (config && OAUTH_PROVIDERS.includes(config.cloud_provider)) {
        cloudConnected = await api.checkCloudRemoteExists(config.cloud_provider);
      }
    } catch (e) {
      console.error('[onboarding] setup failed:', e);
    }
  });

  async function persist(): Promise<boolean> {
    if (!config) return false;
    try {
      config = await api.updateConfig($state.snapshot(config));
      return true;
    } catch (e) {
      console.error('[onboarding] persist failed:', e);
      return false;
    }
  }

  // ── Cloud connect paths ───────────────────────────────────────────────────
  function pickProvider(v: string) {
    if (!config) return;
    config.cloud_provider = v;
    cloudConnected = false;
    persist();
  }

  function changeProvider() {
    if (!config) return;
    config.cloud_provider = '';
    cloudConnected = false;
    cloudConnecting = false;
    persist();
  }

  async function connectOAuth() {
    if (!config || cloudConnecting) return;
    cloudConnecting = true;
    try {
      await api.connectCloudOAuth(config.cloud_provider);
      cloudConnected = true;
      toasts.show({ kind: 'ok', label: 'CLOUD', title: 'Connected', sub: 'Authenticated — your saves will sync.' });
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'CLOUD · OAUTH', title: "Couldn't connect", sub: String(e) });
    } finally {
      cloudConnecting = false;
    }
  }

  async function connectWebdav() {
    if (!config || cloudConnecting) return;
    cloudConnecting = true;
    try {
      await persist(); // ensure provider=webdav + base path are on disk first
      await api.setCloudWebdav(config.cloud_webdav_url.trim(), config.cloud_webdav_username.trim(), webdavPassword, 'other');
      webdavPassword = '';
      config = await api.getConfig();
      cloudConnected = true;
      toasts.show({ kind: 'ok', label: 'CLOUD', title: 'WebDAV connected', sub: 'Saves will sync to this remote.' });
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'CLOUD · WEBDAV', title: "Couldn't connect", sub: String(e) });
    } finally {
      cloudConnecting = false;
    }
  }

  /**
   * On leaving the cloud step, drop a provider that was picked but never
   * actually connected (or a Custom remote left blank) so we don't persist a
   * half-configured remote — onboarding's cloud step is optional.
   */
  async function finalizeCloud() {
    if (!config) return;
    const p = config.cloud_provider;
    if (!p) return;
    const oauthOrDav = OAUTH_PROVIDERS.includes(p) || p === 'webdav';
    const customBlank = p === 'custom' && !config.cloud_remote.trim();
    if ((oauthOrDav && !cloudConnected) || customBlank) {
      config.cloud_provider = '';
      await persist();
    }
  }

  // ── Decky install ─────────────────────────────────────────────────────────
  async function installDecky() {
    if (deckyInstalling) return;
    deckyInstalling = true;
    try {
      const outcome = await api.installDeckyPlugin();
      deckyPlugin = await api.deckyPluginStatus();
      if (outcome.loaderRestarted) {
        toasts.show({ kind: 'ok', label: 'DECKY', title: 'Backup plugin installed', sub: 'Decky was restarted — the Spool Backup plugin is now active.' });
      } else {
        toasts.show({ kind: 'warn', label: 'DECKY', title: 'Backup plugin installed', sub: "The plugin was copied but Decky didn't restart — reboot or restart Decky to load it." });
      }
    } catch (e) {
      toasts.show({ kind: 'bad', label: 'DECKY', title: "Couldn't install plugin", sub: String(e) });
    } finally {
      deckyInstalling = false;
    }
  }

  // ── Navigation ────────────────────────────────────────────────────────────
  // Both "Continue/Finish" and "Skip for now" call next(), and on the cloud step
  // it awaits finalizeCloud() before advancing. Without a guard, a fast double-
  // activation (mouse double-click, or an A-button double-press in Game Mode)
  // re-enters next() before the first resolves and advances the stepper twice,
  // skipping a step. (#288)
  let navigating = $state(false);
  async function next() {
    if (navigating) return;
    navigating = true;
    try {
      if (stepId === 'cloud') await finalizeCloud();
      if (isLast) {
        await complete();
      } else {
        idx += 1;
      }
    } finally {
      navigating = false;
    }
  }

  function back() {
    idx = Math.max(0, idx - 1);
  }

  async function complete() {
    if (!config || finishing) return;
    finishing = true;
    config.onboarding_completed = true;
    await persist();
    onfinish();
  }
</script>

<!-- ── value-prop row (welcome + decky lists) ─────────────────────────────── -->
{#snippet valueRow(kind: 'reel' | 'grid' | 'cloud' | 'shield' | 'download', title: string, desc: string)}
  <div class="flex items-start gap-[14px]">
    <span
      class="mt-px flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-md border border-line-2 bg-white/[0.04]"
      style:color={ACCENT}
    >
      {#if kind === 'reel'}<Disc3 size={17} />
      {:else if kind === 'grid'}<Grid2x2 size={16} />
      {:else if kind === 'cloud'}<Cloud size={16} />
      {:else if kind === 'shield'}<Shield size={16} />
      {:else}<Download size={16} />{/if}
    </span>
    <div class="min-w-0 flex-1">
      <div class="text-[14px] font-semibold text-ink-0">{title}</div>
      <div class="mt-0.5 text-[12.5px] leading-[1.5] text-ink-2">{desc}</div>
    </div>
  </div>
{/snippet}

<!-- ── step heading ───────────────────────────────────────────────────────── -->
{#snippet stepHead(title: string, sub: string)}
  <div class="mb-4">
    <h2 class="font-display m-0 mb-[5px] text-[22px] font-bold tracking-[-0.02em]">{title}</h2>
    <p class="m-0 text-[12.5px] leading-[1.5] text-ink-2">{sub}</p>
  </div>
{/snippet}

<div
  class="ob-scrim fixed inset-0 flex items-center justify-center"
  style:z-index={zIndex}
  style:padding="24px"
  style:background="rgba(8,8,10,0.5)"
  style:backdrop-filter="blur(6px) saturate(0.9) brightness(0.72)"
  style:-webkit-backdrop-filter="blur(6px) saturate(0.9) brightness(0.72)"
>
  <div
    class="ob-modal flex flex-col overflow-hidden bg-bg-1 text-ink-0"
    role="dialog"
    aria-modal="true"
    aria-label="Set up Spool"
    use:gamepadScope={{ onBack: back }}
    style:--gp-focus={ACCENT}
    style:width="560px"
    style:max-width="calc(100vw - 48px)"
    style:height="548px"
    style:max-height="calc(100vh - 48px)"
    style:border="1px solid var(--color-line-2)"
    style:border-radius="8px"
    style:box-shadow="0 30px 90px rgba(0,0,0,0.6)"
  >
    {#if !config}
      <div class="flex flex-1 items-center justify-center">
        <MonoLabel size={10}>Loading…</MonoLabel>
      </div>
    {:else}
      <!-- progress -->
      <div class="flex items-center justify-between" style:padding="16px 28px 0">
        <MonoLabel size={9.5}><span class="whitespace-nowrap">Setup · Step {idx + 1} / {steps.length}</span></MonoLabel>
        <div class="flex gap-1">
          {#each steps as st, i (st)}
            <span
              class="h-[3px] w-[22px] rounded-[2px] transition-colors duration-150"
              style:background={i <= idx ? ACCENT : 'var(--color-line-2)'}
            ></span>
          {/each}
        </div>
      </div>

      <!-- body -->
      <div class="min-h-0 flex-1 overflow-y-auto" style:padding="22px 28px">

        {#if stepId === 'welcome'}
          <div class="flex h-full flex-col">
            <div class="mb-[18px] flex items-center gap-3">
              <div
                class="flex h-[46px] w-[46px] items-center justify-center rounded-md border border-line-2"
                style:background="var(--color-spool-deep, #1a1612)"
              >
                <SpoolMark size={26} color="var(--color-ink-0)" tape={ACCENT} />
              </div>
              <div>
                <h2 class="font-display m-0 text-[25px] font-bold tracking-[-0.02em]">Welcome to Spool</h2>
                <p class="m-0 mt-[3px] text-[13px] text-ink-2">A cover-art game library that keeps your saves safe.</p>
              </div>
            </div>
            <div class="flex flex-col gap-4">
              {@render valueRow('reel', 'Saves, handled', 'Spool backs up your saves before and after every session — automatically, via the bundled ludusavi.')}
              {@render valueRow('grid', 'A cover-art shelf', 'Games arrive with cover, hero, and logo art fetched for you as you add them.')}
              {@render valueRow('cloud', 'Sync anywhere', 'Optionally mirror your saves to the cloud so they follow you between devices.')}
            </div>
          </div>

        {:else if stepId === 'covers'}
          <div>
            {@render stepHead('Cover artwork', "Spool fetches cover, hero, and logo art when you add a game — from Steam's official artwork first, with SteamGridDB as a fallback.")}
            <div class="overflow-hidden rounded-md border border-line-2 bg-bg-2">
              <div class="flex items-center gap-[14px]" style:padding="14px 16px">
                <div class="flex-1">
                  <div class="text-[13px] font-semibold">Use SteamGridDB</div>
                  <div class="mt-0.5 text-[11.5px] text-ink-2">
                    {config.steamgriddb_enabled ? 'Art fetches on every game import.' : 'Covers will use generated placeholders.'}
                  </div>
                </div>
                <Toggle bind:checked={config.steamgriddb_enabled} onchange={persist} aria-label="Use SteamGridDB" />
              </div>
              {#if config.steamgriddb_enabled}
                <div class="border-t border-dashed border-line-1 bg-bg-0" style:padding="0 16px 16px">
                  <div class="text-[11.5px] text-ink-2" style:margin="12px 0 8px">Paste your API key — it's only ever sent to SteamGridDB.</div>
                  <div class="flex gap-2">
                    <TextField bind:value={config.steamgriddb_api_key} placeholder="API key" mono masked full oncommit={persist} />
                    <a href="https://www.steamgriddb.com/profile/preferences/api" target="_blank" rel="noopener noreferrer">
                      <Btn variant="ghost">
                        {#snippet icon()}<KeyRound size={14} />{/snippet}
                        Get a key
                      </Btn>
                    </a>
                  </div>
                </div>
              {/if}
            </div>
            {#if !config.steamgriddb_enabled}
              <p class="mt-3 text-[11.5px] text-ink-3">You can turn this on anytime in Settings → Cover artwork.</p>
            {/if}
          </div>

        {:else if stepId === 'cloud'}
          <div>
            {@render stepHead('Cloud saves', 'Connect a cloud remote now so your saves follow you between devices. Optional — you can skip and add it later.')}

            {#if !chosenProvider}
              <div class="grid grid-cols-2 gap-2">
                {#each CLOUD_PROVIDERS as prov (prov.v)}
                  <button
                    type="button"
                    onclick={() => pickProvider(prov.v)}
                    class="flex cursor-pointer flex-col items-start gap-[3px] rounded-md border border-line-2 bg-bg-2 text-left transition-colors hover:border-line-3"
                    style:padding="12px 14px"
                  >
                    <span class="text-[13px] font-semibold text-ink-0">{prov.l}</span>
                    <span class="font-mono text-[9.5px] uppercase tracking-[0.06em] text-ink-3">{prov.note}</span>
                  </button>
                {/each}
              </div>
            {:else}
              <!-- chosen provider bar -->
              <div
                class="mb-[14px] flex items-center gap-[10px] rounded-md bg-bg-2"
                style:padding="10px 14px"
                style:border="1px solid {cloudConnected ? 'color-mix(in srgb, var(--color-ok) 33%, transparent)' : 'var(--color-line-2)'}"
              >
                <div class="min-w-0 flex-1">
                  <div class="text-[13px] font-semibold">{chosenProvider.l}</div>
                  <div class="font-mono text-[9.5px] uppercase tracking-[0.06em] text-ink-3">{chosenProvider.note}</div>
                </div>
                {#if cloudConnected}<Pill kind="ok">Connected</Pill>{/if}
                <Btn variant="ghost" onclick={changeProvider}>Change</Btn>
              </div>

              <div class="flex flex-col gap-3">
                {#if chosenProvider.kind === 'oauth'}
                  {#if cloudConnected}
                    <div
                      class="flex items-center gap-[10px] rounded-md"
                      style:padding="11px 14px"
                      style:background="color-mix(in srgb, var(--color-ok) 6%, transparent)"
                      style:border="1px solid color-mix(in srgb, var(--color-ok) 25%, transparent)"
                    >
                      <span class="flex text-ok"><Check size={15} /></span>
                      <span class="flex-1 text-[12.5px] text-ink-1">Authenticated — your account is connected.</span>
                      <button type="button" onclick={() => (cloudConnected = false)} class="cursor-pointer border-none bg-transparent text-[11.5px] text-ink-3 hover:text-ink-1">Disconnect</button>
                    </div>
                  {:else}
                    <div>
                      <Btn variant="primary" accent={ACCENT} full onclick={connectOAuth}>
                        {#snippet icon()}<ExternalLink size={14} />{/snippet}
                        {cloudConnecting ? 'Authorizing…' : `Authorize with ${chosenProvider.l}`}
                      </Btn>
                      <p class="mt-2 text-[11px] leading-[1.45] text-ink-3">Opens your browser to sign in — your password never touches Spool. rclone stores the token.</p>
                    </div>
                  {/if}
                {:else if chosenProvider.kind === 'webdav'}
                  <div>
                    <div class="mb-1.5 text-[12px] font-medium text-ink-1">Server URL</div>
                    <TextField bind:value={config.cloud_webdav_url} placeholder="https://host/webdav" mono full />
                    <div class="mt-1 text-[11px] leading-[1.45] text-ink-3">e.g. https://nextcloud.example.com/remote.php/dav/files/me</div>
                  </div>
                  <div class="grid grid-cols-2 gap-[10px]">
                    <div>
                      <div class="mb-1.5 text-[12px] font-medium text-ink-1">Username</div>
                      <TextField bind:value={config.cloud_webdav_username} placeholder="username" mono full />
                    </div>
                    <div>
                      <div class="mb-1.5 text-[12px] font-medium text-ink-1">Password</div>
                      <TextField bind:value={webdavPassword} placeholder="password" mono masked full />
                    </div>
                  </div>
                  <div class="flex items-center gap-[10px]">
                    <Btn variant="primary" accent={ACCENT} onclick={connectWebdav} disabled={cloudConnecting || !config.cloud_webdav_url || !config.cloud_webdav_username}>
                      {#snippet icon()}<Check size={14} />{/snippet}
                      {cloudConnecting ? 'Connecting…' : cloudConnected ? 'Reconnect' : 'Connect'}
                    </Btn>
                    {#if cloudConnected}
                      <span class="flex items-center gap-1.5 text-[11.5px] text-ok"><Check size={14} /> Reached the server</span>
                    {/if}
                  </div>
                {:else}
                  <div>
                    <div class="mb-1.5 text-[12px] font-medium text-ink-1">rclone remote name</div>
                    <TextField bind:value={config.cloud_remote} placeholder="my-remote" mono oncommit={persist} />
                    <div class="mt-1 text-[11px] leading-[1.45] text-ink-3">The name of a remote you've already configured with rclone.</div>
                  </div>
                {/if}

                <div>
                  <div class="mb-1.5 text-[12px] font-medium text-ink-1">Folder on the remote</div>
                  <TextField bind:value={config.cloud_base_path} placeholder="Spool" mono oncommit={persist} />
                  <div class="mt-1 text-[11px] leading-[1.45] text-ink-3">Saves go to &lt;folder&gt;/ludusavi-backup; Spool's cross-device state to &lt;folder&gt;/_spool.</div>
                </div>
              </div>
            {/if}
          </div>

        {:else if stepId === 'decky'}
          <div>
            {@render stepHead('Game Mode companion', 'A Decky Loader plugin that brings Spool into SteamOS Game Mode — no Desktop Mode round-trips.')}
            <div class="mb-4 flex flex-col gap-3">
              {@render valueRow('shield', 'Backup on forced quit', 'Saves even when you Quick Access → Exit Game, from outside the game\'s process tree.')}
              {@render valueRow('grid', 'Browse your library in Game Mode', 'Your cover-art shelf, right in the Quick Access panel.')}
              {@render valueRow('download', 'LAN downloads in Game Mode', 'Pull installs from peers on your network without leaving the game.')}
            </div>
            <div
              class="flex items-center gap-3 rounded-md bg-bg-2"
              style:padding="12px 14px"
              style:border="1px solid {deckyInstalled ? 'color-mix(in srgb, var(--color-ok) 33%, transparent)' : 'var(--color-line-2)'}"
            >
              <span class="flex" style:color={deckyInstalled ? 'var(--color-ok)' : 'var(--color-ink-3)'}>
                {#if deckyInstalled}<Check size={16} />{:else}<Gamepad2 size={16} />{/if}
              </span>
              <span class="flex-1 text-[12.5px] text-ink-1">
                {deckyInstalled ? 'Installed — Decky restarted, plugin active.' : 'Installing asks for your password and restarts Decky.'}
              </span>
              {#if !deckyInstalled}
                <Btn variant="primary" accent={ACCENT} onclick={installDecky} disabled={deckyInstalling}>
                  {#snippet icon()}<Sparkles size={14} />{/snippet}
                  {deckyInstalling ? 'Installing…' : 'Install plugin'}
                </Btn>
              {/if}
            </div>
          </div>
        {/if}

      </div>

      <!-- footer -->
      <div class="flex items-center gap-2 border-t border-line-1" style:padding="14px 28px" style:background="rgba(0,0,0,0.18)">
        {#if idx > 0}
          <Btn variant="ghost" onclick={back}>
            {#snippet icon()}<ChevronLeft size={14} />{/snippet}
            Back
          </Btn>
        {:else}
          <span class="font-mono whitespace-nowrap text-[9.5px] uppercase tracking-[0.1em] text-ink-3">Spool · First run</span>
        {/if}
        <div class="flex-1"></div>
        {#if skippable}<Btn variant="ghost" onclick={next} disabled={navigating}>Skip for now</Btn>{/if}
        <Btn variant="primary" accent={ACCENT} onclick={next} disabled={navigating}>{primaryLabel}</Btn>
      </div>
    {/if}
  </div>
</div>

<style>
  .ob-scrim {
    animation: ob-fade 180ms ease;
  }
  .ob-modal {
    animation: ob-pop 220ms ease;
  }
  @keyframes ob-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes ob-pop {
    from {
      opacity: 0;
      transform: translateY(10px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>
