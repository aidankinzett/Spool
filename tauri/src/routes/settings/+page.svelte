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
  import { ChevronLeft, Folder, Sparkles } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import type { ConfigData } from '$lib/types';
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

  onMount(async () => {
    try {
      config = await api.getConfig();
    } catch (e) {
      error = String(e);
    }
  });

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

          <p class="px-1 text-[11px] text-ink-3">
            Changes save automatically. Settings live at
            <code class="font-mono text-ink-2">%LOCALAPPDATA%\Spool\config.json</code>.
          </p>
        </div>
      {/if}
    </div>
  </main>
</div>
