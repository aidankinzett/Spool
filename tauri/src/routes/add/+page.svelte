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
  import { FileWarning, Folder, Search, ExternalLink } from '@lucide/svelte';
  import { api } from '$lib/api';
  import type { SearchCandidate } from '$lib/types';
  import AppChrome from '$lib/components/AppChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import Btn from '$lib/components/Btn.svelte';
  import CandidateRow from '$lib/components/CandidateRow.svelte';

  // ── State ──────────────────────────────────────────────────────────────
  let exePath = $state<string | null>(null);
  let identifying = $state(false);
  let candidates = $state<SearchCandidate[]>([]);
  let picked = $state<SearchCandidate | null>(null);
  let searchQuery = $state('');
  let searchMode = $state(false); // true once the user types in the search box
  let error = $state<string | null>(null);

  const stage = $derived.by((): 'no-exe' | 'identifying' | 'matches' | 'no-match' | 'search' => {
    if (!exePath) return 'no-exe';
    if (identifying) return 'identifying';
    if (searchMode) return 'search';
    if (candidates.length === 0) return 'no-match';
    return 'matches';
  });

  const fileMeta = $derived.by(() => {
    if (!exePath) return null;
    const parts = exePath.split(/[\\/]/);
    return {
      name: parts.at(-1) ?? exePath,
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
    await identify();
  }

  async function identify() {
    if (!exePath) return;
    error = null;
    identifying = true;
    picked = null;
    searchMode = false;
    searchQuery = '';
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

  // ── Manual search (debounced) ──────────────────────────────────────────
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

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
      try {
        candidates = await api.searchGames(value);
        picked = candidates[0] ?? null;
      } catch (e) {
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
        has_cloud_save: picked.has_cloud_save,
        manifest_install_dir: picked.manifest_install_dir,
        save_paths: picked.save_paths,
      });
      await getCurrentWindow().close();
    } catch (e) {
      error = String(e);
    }
  }

  async function addWithoutTracking() {
    if (!exePath) return;
    try {
      const fallbackName = fileMeta?.name.replace(/\.[^.]+$/, '') ?? 'Untitled Game';
      await api.addGame({
        game_name: picked?.name ?? fallbackName,
        exe_path: exePath,
        has_cloud_save: false,
        save_paths: [],
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

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
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
                you can add it without save tracking — Spool will launch it but won't back up saves.
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
            {#each candidates as cand, i (cand.name)}
              <div
                class="border-b border-line-1 last:border-b-0"
                style:border-bottom-style="dashed"
              >
                <CandidateRow
                  {cand}
                  index={i}
                  picked={picked?.name === cand.name}
                  onpick={() => (picked = cand)}
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

