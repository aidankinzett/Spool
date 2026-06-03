<script lang="ts">
  /**
   * Install Game — guided repack installer (Linux).
   *
   * Flow:
   *   1. Configure — pick the repack's setup.exe and confirm a game name
   *      (seeds the host install folder under ~/.local/share/Spool/games).
   *   2. Installing — runs setup.exe through Proton/umu with that folder mounted
   *      as a Wine drive. The user installs into the shown drive letter; this
   *      stage blocks until the installer window closes.
   *   3. Detect — pick the installed game .exe, identify it through ludusavi
   *      (same as Add Game), then add it to the library. The install-time prefix
   *      is attached so the game launches where it was installed.
   */
  import { onMount } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { Folder, HardDrive, Loader2, Package, FileWarning } from '@lucide/svelte';
  import { api } from '$lib/api';
  import type { SearchCandidate, RepackInstallResult, ProtonVersion } from '$lib/types';
  import AppChrome from '$lib/components/AppChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import TextField from '$lib/components/TextField.svelte';
  import CandidateRow from '$lib/components/CandidateRow.svelte';

  type Stage = 'config' | 'installing' | 'detect';

  // ── State ──────────────────────────────────────────────────────────────
  let stage = $state<Stage>('config');
  let setupExe = $state<string | null>(null);
  let gameName = $state('');
  let error = $state<string | null>(null);

  // Proton version picker — loaded at mount, pre-selects first GE-Proton found.
  let protonVersions = $state<ProtonVersion[]>([]);
  let selectedProton = $state('');
  let hasGeProton = $derived(
    protonVersions.some((v) => v.name.toLowerCase().includes('ge-proton')),
  );

  // Set once the installer finishes — carries the host install dir + the prefix
  // the game was installed into (forwarded to add_game).
  let install = $state<RepackInstallResult | null>(null);
  // Populated early via the install:drive-ready event so the user sees the
  // drive letter before the installer command returns.
  let earlyDriveLetter = $state<string | null>(null);

  // Detect stage — mirrors the Add Game flow.
  let exePath = $state<string | null>(null);
  let identifying = $state(false);
  let candidates = $state<SearchCandidate[]>([]);
  let picked = $state<SearchCandidate | null>(null);

  function baseName(path: string): string {
    const parts = path.split(/[\\/]/);
    return parts.at(-1) ?? path;
  }
  function parentDir(path: string): string | null {
    const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    return idx > 0 ? path.slice(0, idx) : null;
  }

  /** Guess a game name from the setup.exe's parent folder, dropping the usual
   *  repack tags like "[FitGirl Repack]" or "(v1.2)". */
  function guessName(setupPath: string): string {
    const folder = parentDir(setupPath);
    const raw = folder ? baseName(folder) : baseName(setupPath).replace(/\.[^.]+$/, '');
    return raw
      .replace(/[[(].*?[\])]/g, '')
      .replace(/\s+/g, ' ')
      .trim();
  }

  function candKey(c: SearchCandidate): string {
    return `${c.name}::${c.steam_id ?? ''}::${c.gog_id ?? ''}::${c.manifest_install_dir ?? ''}`;
  }

  // ── Mount: load Proton versions + pick the installer immediately ───────
  onMount(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      unlisten = await listen<string>('install:drive-ready', (e) => {
        earlyDriveLetter = e.payload;
      });
      protonVersions = await api.listProtonVersions();
      const ge = protonVersions.find((v) => v.name.toLowerCase().includes('ge-proton'));
      if (ge) selectedProton = ge.path;
      await pickSetup(true);
    })();
    return () => unlisten?.();
  });

  async function pickSetup(closeOnCancel = false) {
    const result = await openDialog({
      title: "Pick the repack's installer (setup.exe)",
      multiple: false,
      filters: [
        { name: 'Installer', extensions: ['exe'] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (typeof result !== 'string') {
      if (closeOnCancel && !setupExe) await getCurrentWindow().close();
      return;
    }
    setupExe = result;
    if (!gameName.trim()) gameName = guessName(result);
  }

  async function startInstall() {
    if (!setupExe || !gameName.trim()) return;
    error = null;
    stage = 'installing';
    try {
      install = await api.runRepackInstaller(setupExe, gameName.trim(), undefined, selectedProton || undefined);
      // Installer finished — pick the installed game exe.
      stage = 'detect';
      await pickGameExe();
    } catch (e) {
      error = String(e);
      stage = 'config';
    }
  }

  async function pickGameExe() {
    const result = await openDialog({
      title: 'Pick the installed game executable',
      multiple: false,
      defaultPath: install?.install_dir ?? undefined,
      filters: [
        { name: 'Executable', extensions: ['exe', ''] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (typeof result !== 'string') return;
    exePath = result;
    await identify();
  }

  async function identify() {
    if (!exePath) return;
    error = null;
    identifying = true;
    picked = null;
    try {
      candidates = await api.searchByExe(exePath);
      picked = candidates[0] ?? null;
    } catch (e) {
      error = String(e);
      candidates = [];
    } finally {
      identifying = false;
    }
  }

  // ── Add to library ─────────────────────────────────────────────────────
  async function addWithCandidate() {
    if (!exePath || !picked || !install) return;
    try {
      await api.addGame({
        game_name: picked.name,
        exe_path: exePath,
        steam_id: picked.steam_id,
        gog_id: picked.gog_id,
        lutris_slug: picked.lutris_slug,
        manifest_install_dir: picked.manifest_install_dir,
        save_paths: picked.save_paths,
        game_folder_path: install.install_dir,
        wine_prefix_path: install.prefix_path,
        proton_version_path: install.proton_path ?? undefined,
      });
      await getCurrentWindow().close();
    } catch (e) {
      error = String(e);
    }
  }

  async function addWithoutTracking() {
    if (!exePath || !install) return;
    try {
      const fallback = baseName(exePath).replace(/\.[^.]+$/, '') || gameName.trim() || 'Untitled Game';
      await api.addGame({
        game_name: picked?.name ?? fallback,
        exe_path: exePath,
        save_paths: [],
        game_folder_path: install.install_dir,
        wine_prefix_path: install.prefix_path,
        proton_version_path: install.proton_path ?? undefined,
      });
      await getCurrentWindow().close();
    } catch (e) {
      error = String(e);
    }
  }

  async function cancel() {
    await getCurrentWindow().close();
  }
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <AppChrome sub="INSTALL GAME" onback={() => history.back()} />

  <main class="flex flex-1 flex-col overflow-hidden">
    <!-- Header -->
    <header class="px-6 pb-3 pt-5">
      <MonoLabel size={10}>Spool · install repack</MonoLabel>
      <h1 class="font-display mt-1.5 mb-1 text-[22px] font-bold leading-tight" style:letter-spacing="-0.02em">
        Install a game
      </h1>
      <p class="m-0 max-w-[540px] text-[12px] leading-relaxed text-ink-2">
        Run a repack's <code class="font-mono">setup.exe</code> through Proton. Spool makes a clean
        install folder, mounts it into a Wine prefix as a drive, then adds the finished game to your
        library.
      </p>
    </header>

    <!-- Error banner -->
    {#if error}
      <div class="mx-6 mb-1 rounded-sm border border-bad/40 bg-bad/10 px-3 py-2 text-[12px] text-bad">
        <code class="font-mono text-[11px]">{error}</code>
      </div>
    {/if}

    <!-- Body -->
    <div class="flex flex-1 flex-col gap-3 overflow-hidden px-6 pb-2 pt-2">
      {#if stage === 'config'}
        <!-- Installer strip -->
        <div class="relative flex items-center gap-3.5 overflow-hidden rounded-md border border-line-1 bg-bg-1 px-3.5 py-3">
          <div class="absolute inset-y-0 left-0 w-[3px]" style:background="var(--color-spool)"></div>
          <div class="flex size-[38px] shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-1">
            <Package size={18} strokeWidth={1.4} />
          </div>
          <div class="min-w-0 flex-1">
            <div class="font-mono truncate text-[13px] font-medium">
              {setupExe ? baseName(setupExe) : 'No installer selected'}
            </div>
            <div class="font-mono mt-1 min-w-0 truncate text-[10.5px] tracking-[0.04em] text-ink-3">
              {setupExe ?? 'Pick the setup.exe from the repack you downloaded.'}
            </div>
          </div>
          <Btn variant="ghost" onclick={() => pickSetup(false)}>
            {#snippet icon()}<Folder size={14} />{/snippet}
            {setupExe ? 'Change' : 'Pick'}
          </Btn>
        </div>

        <!-- Game name -->
        <div class="rounded-md border border-line-1 bg-bg-1 px-3.5 py-3">
          <MonoLabel size={10}>Game name</MonoLabel>
          <p class="m-0 mt-1 mb-2 text-[11.5px] leading-relaxed text-ink-3">
            Used for the install folder name. You can refine the library name after install.
          </p>
          <TextField bind:value={gameName} placeholder="e.g. Elden Ring" full />
        </div>

        <!-- Proton version -->
        <div class="rounded-md border border-line-1 bg-bg-1 px-3.5 py-3">
          <MonoLabel size={10}>Proton version</MonoLabel>
          <p class="m-0 mt-1 mb-2 text-[11.5px] leading-relaxed text-ink-3">
            GE-Proton is recommended for repacks — it handles DLLs and codecs that stock Proton rejects.
          </p>
          <select
            bind:value={selectedProton}
            style="color-scheme: dark"
            class="font-mono rounded-[4px] border border-line-1 bg-bg-2 px-2 py-1 text-[11.5px] text-ink-0"
          >
            <option value="">Auto (umu default)</option>
            {#each protonVersions as p (p.path)}
              <option value={p.path}>{p.name}</option>
            {/each}
          </select>
        </div>

        <!-- GE-Proton not found warning -->
        {#if protonVersions.length > 0 && !hasGeProton}
          <div class="rounded-sm border border-warn/40 bg-warn/10 px-3.5 py-2.5 text-[12px] text-warn">
            <span class="font-semibold">GE-Proton not found.</span> It's recommended for repacks.
            Install it via <span class="font-mono">ProtonUp-Qt</span>, or copy a GE-Proton build into
            <span class="font-mono">~/.local/share/Steam/compatibilitytools.d/</span> and restart Spool.
          </div>
        {/if}
      {:else if stage === 'installing'}
        <div class="flex flex-1 flex-col items-center justify-center gap-4 py-8 text-center">
          <Loader2 size={40} class="animate-spin text-spool" />
          <div>
            <div class="font-display text-[18px] font-semibold" style:letter-spacing="-0.01em">
              Installer running through Proton…
            </div>
            <p class="m-0 mt-2 max-w-[460px] text-[12.5px] leading-relaxed text-ink-2">
              When the installer asks where to install, choose the
              <span class="inline-flex items-center gap-1 font-mono font-semibold text-spool">
                <HardDrive size={13} />{install?.drive_letter ?? earlyDriveLetter ?? '…'}
              </span>
              drive. This step finishes when you close the installer.
            </p>
          </div>
        </div>
      {:else if stage === 'detect'}
        <!-- Picked exe strip -->
        <div class="relative flex items-center gap-3.5 overflow-hidden rounded-md border border-line-1 bg-bg-1 px-3.5 py-3">
          <div class="absolute inset-y-0 left-0 w-[3px]" style:background="var(--color-spool)"></div>
          <div class="flex size-[38px] shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-1">
            <FileWarning size={18} strokeWidth={1.4} />
          </div>
          <div class="min-w-0 flex-1">
            <div class="font-mono truncate text-[13px] font-medium">
              {exePath ? baseName(exePath) : 'No executable selected'}
            </div>
            <div class="font-mono mt-1 min-w-0 truncate text-[10.5px] tracking-[0.04em] text-ink-3">
              {exePath ?? 'Pick the game exe the installer created.'}
            </div>
          </div>
          <Btn variant="ghost" onclick={pickGameExe}>
            {#snippet icon()}<Folder size={14} />{/snippet}
            {exePath ? 'Change' : 'Pick exe'}
          </Btn>
        </div>

        <div class="flex min-h-0 flex-1 flex-col">
          {#if identifying}
            <div class="flex flex-1 items-center justify-center gap-2.5 py-8 text-ink-2">
              <Loader2 size={18} class="animate-spin text-spool" />
              <span class="text-[13px]">Identifying through ludusavi…</span>
            </div>
          {:else if exePath && candidates.length === 0}
            <div class="flex flex-1 flex-col items-center justify-center gap-2 rounded-sm border border-line-1 bg-bg-1 p-6 text-center">
              <div class="inline-flex size-10 items-center justify-center rounded-full bg-warn/15 text-warn">
                <FileWarning size={18} strokeWidth={1.5} />
              </div>
              <div class="font-display text-[16px] font-semibold">Couldn't identify this game</div>
              <p class="m-0 max-w-[420px] text-[12.5px] leading-relaxed text-ink-2">
                You can still add it without save tracking — Spool will launch it but won't back up saves.
              </p>
            </div>
          {:else if candidates.length > 0}
            <div class="mb-2 mt-1 px-0.5">
              <MonoLabel size={10} class="text-spool">
                Ludusavi · {candidates.length} candidate{candidates.length === 1 ? '' : 's'}
              </MonoLabel>
            </div>
            <div class="min-h-0 flex-1 overflow-y-auto rounded-sm border border-line-1 bg-bg-1">
              {#each candidates as cand (candKey(cand))}
                <div class="border-b border-line-1 last:border-b-0">
                  <CandidateRow
                    {cand}
                    picked={!!picked && candKey(picked) === candKey(cand)}
                    onpick={() => (picked = cand)}
                  />
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Footer -->
    <footer class="flex items-center gap-2 border-t border-line-1 bg-black/20 px-5 py-3">
      <Btn variant="ghost" onclick={cancel}>Cancel</Btn>
      <div class="flex-1"></div>
      {#if stage === 'config'}
        <Btn variant="primary" onclick={startInstall} disabled={!setupExe || !gameName.trim()} class="min-w-[160px] justify-center">
          Run installer
        </Btn>
      {:else if stage === 'detect'}
        <Btn variant="ghost" onclick={addWithoutTracking} disabled={!exePath || identifying}>
          Add without save tracking
        </Btn>
        <Btn variant="primary" onclick={addWithCandidate} disabled={!picked || identifying} class="min-w-[180px] justify-center">
          {picked ? `Add "${picked.name}"` : 'Pick a candidate'}
        </Btn>
      {/if}
    </footer>
  </main>
</div>
