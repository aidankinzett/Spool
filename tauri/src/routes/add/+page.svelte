<script lang="ts">
  /**
   * Add Game — single-page flow.
   *
   * On mount, opens the OS file picker. After the user picks an exe:
   *   1. Identifying  — spinner while ludusavi looks up the name
   *   2. Matches      — auto-detected candidates (if found)
   *      or No-Match  — couldn't identify; offer add-without-tracking
   *   3. Search       — user typed in the search box → manual ludusavi search
   *
   * Add buttons in the footer:
   *   - "Add as <name>"           — picks the highlighted candidate
   *   - "Add without save tracking" — uses the exe filename, no manifest data
   *   - Cancel                     — back to library
   */
  import { onMount } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import {
    FileWarning,
    Folder,
    HardDriveDownload,
    Search,
    ExternalLink,
    Wifi,
    Type,
  } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { parentDir } from '$lib/format';
  import type { SearchCandidate } from '$lib/types';
  import AppChrome from '$lib/components/AppChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import CandidateRow from '$lib/components/CandidateRow.svelte';
  import { gamepadScope } from '$lib/gamepad';

  // ── State ──────────────────────────────────────────────────────────────
  let exePath = $state<string | null>(null);
  let identifying = $state(false);
  let candidates = $state<SearchCandidate[]>([]);
  let picked = $state<SearchCandidate | null>(null);
  // Editable game name — the title used when adding *without* save tracking,
  // and the single source of truth for that path. Defaults to the cleaned exe
  // filename; an auto-highlighted candidate (candidates[0]) never touches it, so
  // a wrong ludusavi match can't smuggle its name (and cover art) onto an
  // untracked add, while explicitly clicking a candidate fills it in. The name
  // also drives artwork (Steam store search → Steam CDN, else SteamGridDB by
  // name) when no manifest steam id is available.
  let untrackedName = $state('');
  let searchQuery = $state('');
  let searchMode = $state(false); // true once the user types in the search box
  let error = $state<string | null>(null);
  // Install folder for the game. Auto-detected when a file is picked — from the
  // matched candidate's install root (ludusavi's `installDir`, found among the
  // exe's ancestor folders) when known, otherwise the exe's parent directory.
  // The user can verify or override it before adding. A folder is what makes
  // LAN sharing (on by default) actually streamable.
  let gameFolder = $state<string | null>(null);
  // True once the user picks a folder by hand, so auto-detection stops
  // overwriting their choice when they switch candidates.
  let folderTouched = $state(false);
  // Set when opened from an uninstalled game's "Reinstall…" affordance
  // (`?reinstall=<id>`): the existing entry's id (and name, for the hint).
  // Passed to add_game so that exact library entry is reused — saves, playtime
  // and artwork carry over — instead of a duplicate being created.
  let reinstallTargetId = $state<string | null>(null);
  let reinstallName = $state<string | null>(null);



  /**
   * Auto-fill the install folder from a candidate, unless the user has already
   * set one by hand. Prefers the candidate's detected install root and falls
   * back to the exe's parent directory.
   */
  function applyDetectedFolder(cand: SearchCandidate | null) {
    if (folderTouched || !exePath) return;
    gameFolder = cand?.install_root ?? parentDir(exePath);
  }

  /**
   * Highlight a candidate (explicit user click) and re-derive the install
   * folder from it. Clicking is a deliberate choice, so it also fills the
   * editable name — an auto-highlight (see identify/search) never does.
   */
  function pick(cand: SearchCandidate) {
    picked = cand;
    untrackedName = cand.name;
    applyDetectedFolder(cand);
  }

  /** Last path segment, handling both `\` and `/` separators. */
  function baseName(path: string): string {
    return path.split(/[\\/]/).at(-1) ?? path;
  }

  /** Strip the directory and extension off a path to get a display name. */
  function cleanFileName(path: string): string {
    return baseName(path).replace(/\.[^.]+$/, '') || 'Untitled Game';
  }

  /**
   * Stable identity for a candidate. `SearchCandidate` has no unique id and
   * the multi-query lookup can surface two entries with the same display name,
   * so key on name + store ids to keep the `{#each}` and the picked-highlight
   * distinct.
   */
  function candKey(c: SearchCandidate): string {
    return `${c.name}::${c.steam_id ?? ''}::${c.gog_id ?? ''}::${c.manifest_install_dir ?? ''}`;
  }

  const stage = $derived.by((): 'no-exe' | 'identifying' | 'matches' | 'no-match' | 'search' => {
    if (!exePath) return 'no-exe';
    if (identifying) return 'identifying';
    if (searchMode) return 'search';
    if (candidates.length === 0) return 'no-match';
    return 'matches';
  });

  const fileMeta = $derived.by(() => {
    if (!exePath) return null;
    return {
      name: baseName(exePath),
      path: exePath,
    };
  });

  const stripColor = $derived(
    stage === 'no-match'
      ? 'var(--color-warn)'
      : stage === 'identifying'
        ? 'var(--color-info)'
        : 'var(--color-spool)',
  );

  // ── Mount: pick exe immediately, or bail back to library ───────────────
  onMount(async () => {
    // Reinstall flow: opened with `?reinstall=<id>` from an uninstalled game.
    const reinstall = new URLSearchParams(window.location.search).get('reinstall');
    if (reinstall) {
      reinstallTargetId = reinstall;
      try {
        const games = await api.listGames();
        reinstallName = games.find((g) => g.id === reinstall)?.game_name ?? null;
      } catch {
        // Best-effort hint only — the backend still reuses the entry by id.
      }
    }
    await pickExe();
  });

  async function pickExe() {
    const result = await openDialog({
      title: 'Pick the game executable',
      multiple: false,
      filters: [
        { name: 'Executable', extensions: ['exe', ''] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (typeof result !== 'string') {
      // User cancelled the picker — close this popup.
      await getCurrentWindow().close();
      return;
    }
    exePath = result;
    // A freshly picked exe resets any manual folder choice. Seed the install
    // folder with the exe's parent dir; identify() refines it once a candidate
    // with a detected install root comes back.
    folderTouched = false;
    gameFolder = parentDir(result);
    // Seed the untracked name from the filename so the no-match path has a
    // sensible default the user can edit.
    untrackedName = cleanFileName(result);
    await identify();
  }

  async function browseFolder() {
    const result = await openDialog({
      title: 'Pick the install folder',
      directory: true,
      multiple: false,
      defaultPath: gameFolder ?? undefined,
    });
    if (typeof result === 'string') {
      gameFolder = result;
      folderTouched = true;
    }
  }

  async function identify() {
    if (!exePath) return;
    error = null;
    identifying = true;
    picked = null;
    searchMode = false;
    searchQuery = '';
    const seq = ++searchSeq;
    try {
      const result = await api.searchByExe(exePath);
      if (seq !== searchSeq) return; // a newer search/identify superseded this
      candidates = result;
      picked = candidates[0] ?? null;
      applyDetectedFolder(picked);
    } catch (e) {
      if (seq !== searchSeq) return;
      error = String(e);
      candidates = [];
    } finally {
      identifying = false;
    }
  }

  // ── Manual search (debounced) ──────────────────────────────────────────
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  // Monotonic token shared by identify() and the debounced search: a slow
  // in-flight request that resolves after a newer one bails instead of
  // overwriting candidates / picked / the install folder with stale results.
  // clearTimeout only cancels a pending timer, not an already-dispatched fetch.
  // (#292)
  let searchSeq = 0;

  function onSearchInput(value: string) {
    searchQuery = value;
    searchMode = value.trim().length > 0;
    if (searchTimer) clearTimeout(searchTimer);
    if (!searchMode) {
      // Cleared the box — fall back to the original identify results.
      candidates = [];
      identify();
      return;
    }
    searchTimer = setTimeout(async () => {
      const seq = ++searchSeq;
      try {
        const result = await api.searchGames(value);
        if (seq !== searchSeq) return; // a newer search/identify superseded this
        candidates = result;
        picked = candidates[0] ?? null;
        applyDetectedFolder(picked);
      } catch (e) {
        if (seq !== searchSeq) return;
        error = String(e);
        candidates = [];
      }
    }, 250);
  }

  // ── Submit ─────────────────────────────────────────────────────────────
  async function addWithCandidate() {
    if (!exePath || !picked) return;
    try {
      await api.addGame({
        game_name: picked.name,
        exe_path: exePath,
        steam_id: picked.steam_id,
        gog_id: picked.gog_id,
        lutris_slug: picked.lutris_slug,
        manifest_install_dir: picked.manifest_install_dir,
        save_paths: picked.save_paths,
        game_folder_path: gameFolder,
        reinstall_target_id: reinstallTargetId,
      });
      await getCurrentWindow().close();
    } catch (e) {
      error = String(e);
    }
  }

  async function addWithoutTracking() {
    if (!exePath) return;
    try {
      // The editable name field is the single source of truth here — it's
      // seeded from the filename, filled in when a candidate is explicitly
      // clicked, and freely editable, so a wrong auto-highlighted match never
      // names the entry. Fall back to the cleaned filename if it's been cleared.
      const name = untrackedName.trim() || cleanFileName(exePath);
      await api.addGame({
        game_name: name,
        exe_path: exePath,
        save_paths: [],
        game_folder_path: gameFolder,
        reinstall_target_id: reinstallTargetId,
      });
      await getCurrentWindow().close();
    } catch (e) {
      error = String(e);
    }
  }

  async function cancel() {
    await getCurrentWindow().close();
  }

  function shorten(name: string, max = 28) {
    return name.length <= max ? name : name.slice(0, max - 2) + '…';
  }
</script>

<div
  class="flex h-screen flex-col bg-bg-0 text-ink-0"
  use:gamepadScope={{ onBack: () => history.back() }}
>
  <AppChrome sub="ADD ENTRY" onback={() => history.back()} />

  <main class="flex flex-1 flex-col overflow-hidden">
    <!-- Header -->
    <header class="px-6 pb-3 pt-5">
      <MonoLabel size={10}>Spool · catalog new entry</MonoLabel>
      <h1
        class="font-display mt-1.5 mb-1 text-[22px] font-bold leading-tight"
        style:letter-spacing="-0.02em"
      >
        Add a game
      </h1>
      <p class="m-0 max-w-[540px] text-[12px] leading-relaxed text-ink-2">
        Pick the game's executable. Spool runs it through ludusavi to identify it so saves back up
        automatically.
      </p>
    </header>

    {#if reinstallTargetId}
      <!-- Reinstall hint: opened from an uninstalled game's "Reinstall…". -->
      <div
        class="mx-6 mb-1 flex items-center gap-2.5 rounded-md border px-3.5 py-2.5"
        style:border-color="color-mix(in srgb, var(--color-spool) 35%, transparent)"
        style:background="color-mix(in srgb, var(--color-spool) 10%, transparent)"
      >
        <HardDriveDownload size={15} class="shrink-0 text-spool" />
        <span class="text-[12px] leading-snug text-ink-1">
          {#if reinstallName}
            Reinstalling <strong class="font-semibold">{reinstallName}</strong> — your saves, playtime and
            artwork are kept.
          {:else}
            Reinstalling an existing entry — your saves, playtime and artwork are kept.
          {/if}
        </span>
      </div>
    {/if}

    <!-- Body -->
    <div class="flex flex-1 flex-col gap-3 overflow-hidden px-6 pb-2 pt-2">
      <!-- Exe strip -->
      {#if fileMeta}
        <div
          class="relative flex items-center gap-3.5 overflow-hidden rounded-md border border-line-1 bg-bg-1 px-3.5 py-3"
        >
          <div class="absolute inset-y-0 left-0 w-[3px]" style:background={stripColor}></div>
          <div
            class="flex size-[38px] shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-1"
          >
            <FileWarning size={18} strokeWidth={1.4} />
          </div>
          <div class="min-w-0 flex-1">
            <div class="font-mono truncate text-[13px] font-medium">{fileMeta.name}</div>
            <div
              class="font-mono mt-1 flex items-center gap-2.5 text-[10.5px] tracking-[0.04em] text-ink-3"
            >
              <span class="min-w-0 truncate">{fileMeta.path}</span>
            </div>
          </div>
          <Btn variant="ghost" onclick={pickExe}>
            {#snippet icon()}<Folder size={14} />{/snippet}
            Change file
          </Btn>
        </div>

        <!-- Install folder — auto-detected from the exe, confirm or override.
             Enables LAN sharing so peers can install this game from you. -->
        <div
          class="relative flex items-center gap-3.5 overflow-hidden rounded-md border border-line-1 bg-bg-1 px-3.5 py-3"
        >
          <div class="absolute inset-y-0 left-0 w-[3px]" style:background="var(--color-spool)"></div>
          <div
            class="flex size-[38px] shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-1"
          >
            <Folder size={18} strokeWidth={1.4} />
          </div>
          <div class="min-w-0 flex-1">
            <div class="font-mono truncate text-[13px] font-medium">
              {gameFolder ?? 'No install folder set'}
            </div>
            <div
              class="font-mono mt-1 flex items-center gap-1.5 text-[10.5px] tracking-[0.04em] text-ink-3"
            >
              <Wifi size={11} />
              <span class="min-w-0 truncate">
                {gameFolder
                  ? 'Install folder — shared on your LAN so nearby devices can install it.'
                  : 'Set a folder to share this game on your LAN.'}
              </span>
            </div>
          </div>
          <Btn variant="ghost" onclick={browseFolder}>
            {#snippet icon()}<Folder size={14} />{/snippet}
            Change folder
          </Btn>
        </div>

        <!-- Game name — the title, and the name used when adding without save
             tracking (its single source of truth). Seeded from the filename;
             clicking a candidate below fills it in. Editable in every stage so a
             wrong ludusavi match can be corrected. Also drives artwork (Steam
             store search → Steam CDN, else SteamGridDB by name) for an untracked
             add. -->
        <div
          class="relative flex items-center gap-3.5 overflow-hidden rounded-md border border-line-1 bg-bg-1 px-3.5 py-3"
        >
          <div class="absolute inset-y-0 left-0 w-[3px]" style:background="var(--color-spool)"></div>
          <div
            class="flex size-[38px] shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-1"
          >
            <Type size={18} strokeWidth={1.4} />
          </div>
          <div class="min-w-0 flex-1">
            <input
              id="untracked-name"
              bind:value={untrackedName}
              placeholder="Game name"
              class="font-mono w-full bg-transparent text-[13px] font-medium text-ink-0 outline-none placeholder:text-ink-3"
            />
            <div
              class="font-mono mt-1 text-[10.5px] tracking-[0.04em] text-ink-3"
            >
              Title — and the name (and artwork) used when adding without save tracking.
            </div>
          </div>
        </div>
      {/if}

      <!-- Search bar -->
      {#if stage !== 'identifying' && fileMeta}
        <div
          class="flex h-8 items-center gap-2 rounded-sm border bg-bg-2 px-2.5 transition-colors"
          style:border-color={searchMode ? 'var(--color-line-3)' : 'var(--color-line-1)'}
        >
          <Search size={14} class="text-ink-2" />
          <input
            value={searchQuery}
            placeholder="Search ludusavi"
            oninput={(e) => onSearchInput((e.currentTarget as HTMLInputElement).value)}
            class="font-sans min-w-0 flex-1 bg-transparent text-[12.5px] text-ink-0 outline-none placeholder:text-ink-3"
          />
          {#if searchMode}
            <span class="font-mono text-[10px] tracking-[0.06em] text-ink-2">
              {candidates.length} result{candidates.length === 1 ? '' : 's'}
            </span>
          {/if}
        </div>
      {/if}

      <!-- Error banner -->
      {#if error}
        <div
          class="rounded-sm border border-bad/40 bg-bad/10 px-3 py-2 text-[12px] text-bad"
        >
          <code class="font-mono text-[11px]">{error}</code>
        </div>
      {/if}

      <!-- Body switch -->
      <div class="flex min-h-0 flex-1 flex-col">
        {#if stage === 'no-exe'}
          <p class="font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">Waiting for file…</p>
        {:else if stage === 'identifying'}
          <div class="flex flex-1 flex-col items-center justify-center gap-3.5 py-8">
            <!-- Spinning reels: two circles rotating in opposite directions -->
            <svg viewBox="0 0 48 24" class="size-12 text-spool">
              <g style="transform-origin: 14px 12px; animation: spool-spin 1s linear infinite;">
                <circle cx="14" cy="12" r="6" stroke="currentColor" stroke-width="1.5" fill="none" />
                <circle cx="14" cy="6.5" r="1.4" fill="currentColor" />
              </g>
              <g
                style="transform-origin: 34px 12px; animation: spool-spin 1.4s linear infinite reverse;"
              >
                <circle cx="34" cy="12" r="6" stroke="currentColor" stroke-width="1.5" fill="none" />
                <circle cx="34" cy="6.5" r="1.4" fill="currentColor" />
              </g>
            </svg>
            <div class="text-center">
              <div
                class="font-display text-[17px] font-semibold"
                style:letter-spacing="-0.01em"
              >
                Identifying through ludusavi…
              </div>
              <div class="mt-1 text-[12px] text-ink-3">Usually takes 1–2 seconds.</div>
            </div>
          </div>
        {:else if stage === 'no-match'}
          <div class="mb-2 mt-1.5 flex items-center justify-between px-0.5">
            <MonoLabel size={10} class="text-warn">Ludusavi · No automatic match</MonoLabel>
            <span class="text-[11px] text-ink-3">Try a different name above.</span>
          </div>
          <div
            class="flex flex-1 flex-col items-center justify-center gap-3.5 rounded-sm border border-line-1 bg-bg-1 p-6 text-center"
          >
            <div class="inline-flex size-10 items-center justify-center rounded-full bg-warn/15 text-warn">
              <FileWarning size={18} strokeWidth={1.5} />
            </div>
            <div>
              <div class="font-display text-[18px] font-semibold" style:letter-spacing="-0.012em">
                Spool couldn't identify
                <span class="font-mono text-[15px]">{fileMeta?.name}</span>
              </div>
              <p class="m-0 mt-1.5 max-w-[460px] text-[12.5px] leading-relaxed text-ink-2">
                Try the search above with a shorter name. If ludusavi still doesn't know this game,
                set the <span class="text-ink-1">Game name</span> above and add it without save
                tracking — Spool will launch it and use that name to fetch cover art, but won't back
                up saves.
              </p>
              <p class="m-0 mt-2 max-w-[460px] text-[11.5px] leading-relaxed text-ink-3">
                You can still track its saves later: add it, play once so the game
                creates a save, then open its
                <span class="text-ink-1">Edit&nbsp;→&nbsp;Saves</span> tab to point
                Spool at the save folder. From then on it backs up and syncs like
                any other game.
              </p>
            </div>
            <a
              href="https://github.com/mtkennerly/ludusavi/issues"
              target="_blank"
              rel="noreferrer"
              class="inline-flex items-center gap-1.5 rounded-sm border border-line-1 px-2.5 py-1 text-[11.5px] text-ink-1 transition-colors hover:bg-white/5 hover:text-ink-0"
            >
              <ExternalLink size={12} />
              File an issue against ludusavi
            </a>
          </div>
        {:else}
          <!-- matches or search — same layout, different heading copy -->
          <div class="mb-2 mt-1 flex items-center justify-between px-0.5">
            <MonoLabel size={10} class={stage === 'matches' ? 'text-spool' : 'text-ink-2'}>
              {stage === 'matches'
                ? `Ludusavi · ${candidates.length} candidate${candidates.length === 1 ? '' : 's'}`
                : `Ludusavi · search results · ${candidates.length}`}
            </MonoLabel>
            {#if stage === 'matches'}
              <span class="text-[11px] text-ink-3">Search above to widen the lookup.</span>
            {/if}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto rounded-sm border border-line-1 bg-bg-1">
            {#each candidates as cand (candKey(cand))}
              <div class="border-b border-line-1 last:border-b-0">
                <CandidateRow
                  {cand}
                  picked={!!picked && candKey(picked) === candKey(cand)}
                  onpick={() => pick(cand)}
                />
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>

    <!-- Footer -->
    <footer
      class="flex items-center gap-2 border-t border-line-1 bg-black/20 px-5 py-3"
    >
      <Btn variant="ghost" onclick={cancel}>Cancel</Btn>
      <div class="flex-1"></div>
      <Btn variant="ghost" onclick={addWithoutTracking} disabled={!exePath || identifying}>
        Add without save tracking
      </Btn>
      <Btn
        variant="primary"
        onclick={addWithCandidate}
        disabled={!picked || identifying}
        class="min-w-[200px] justify-center"
      >
        {identifying
          ? 'Identifying…'
          : picked
            ? `Add as "${shorten(picked.name)}"`
            : 'Pick a candidate'}
      </Btn>
    </footer>
  </main>
</div>

