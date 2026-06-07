<script lang="ts">
  /**
   * Saves tab of the game editor. Two jobs:
   *
   *  1. **Manifest override** (manifest-covered games) — show the save locations
   *     ludusavi's manifest declares for this game, grouped/tagged, and let the
   *     user choose which actually sync. Turning off the "settings" tag (or an
   *     individual path) keeps per-device config (graphics options, keybinds)
   *     from being clobbered when saves sync across machines. Stored as exclusion
   *     intent and re-derived per device, so it stays correct cross-OS.
   *  2. **Custom save** (non-manifest games, or extra folders) — track a folder
   *     ludusavi doesn't know about: pick it (the picker opens inside the game's
   *     Proton prefix) or type a ludusavi template.
   *
   * Self-contained (drives `set_custom_save` / `clear_custom_save` /
   * `derive_save_template` / `manifest_save_locations` / `set_manifest_override` /
   * `clear_manifest_override`) so it can be storied directly. `customSave` and
   * `manifestOverride` are reported up via callbacks rather than two-way bound, so
   * the parent owns the entry state.
   */
  import type { Snippet } from 'svelte';
  import { untrack } from 'svelte';
  import { Folder, FolderX, Info, Plus, Trash2 } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import { fmtCatalog } from '$lib/format';
  import { toasts } from '$lib/toasts.svelte';
  import type { CustomSave, ManifestOverride, ManifestPath } from '$lib/types';
  import Btn from './Btn.svelte';
  import EditRow from './EditRow.svelte';
  import TextField from './TextField.svelte';

  let {
    gameId,
    catalogNumber,
    savePaths,
    usesProton,
    prefixReady,
    customSave,
    manifestOverride,
    onChange,
    onOverrideChange,
  }: {
    gameId: string;
    catalogNumber: number;
    /** Manifest save paths — drives the "tracked automatically" status line. */
    savePaths: string[];
    /** True when this game launches through Proton (Linux + Windows exe). */
    usesProton: boolean;
    /** Whether the Proton prefix exists yet — false shows the "launch first" hint. */
    prefixReady: boolean;
    /** Current custom save definition, or null when none is set. */
    customSave: CustomSave | null;
    /** Current manifest override (which manifest locations to skip), or null. */
    manifestOverride: ManifestOverride | null;
    /** Called after the custom save changes so the parent can update its entry. */
    onChange: (custom: CustomSave | null) => void;
    /** Called after the manifest override changes so the parent can update it. */
    onOverrideChange: (override: ManifestOverride | null) => void;
  } = $props();

  // In-progress template for the add row (from the picker or typed), plus busy.
  let saveTemplate = $state('');
  let savesBusy = $state(false);

  const files = $derived(customSave?.files ?? []);
  const registry = $derived(customSave?.registry ?? []);
  const hasCustom = $derived(files.length > 0);

  // ── Manifest override state ────────────────────────────────────────────────
  // The manifest's declared locations for this game (applicable on this device),
  // and the user's exclusions. Optimistic local copies of the override: this
  // panel is the only writer for one game, so we mutate locally and commit.
  let manifestPaths = $state<ManifestPath[]>([]);
  let manifestLoading = $state(false);
  // Optimistic local copies of the override's exclusions, seeded from the prop in
  // an effect (below) so they re-seed when the panel is pointed at another game.
  let exclTags = $state<string[]>([]);
  let exclPaths = $state<string[]>([]);

  const applicablePaths = $derived(manifestPaths.filter((p) => p.applies));
  // Distinct tags across the applicable paths, in a stable order (saves first).
  const distinctTags = $derived(
    [...new Set(applicablePaths.flatMap((p) => p.tags))].sort((a, b) =>
      a === 'save' ? -1 : b === 'save' ? 1 : a.localeCompare(b),
    ),
  );
  const overrideActive = $derived(exclTags.length > 0 || exclPaths.length > 0);
  // Whether ludusavi recognises this game, from the LIVE manifest lookup. This is
  // authoritative: the `savePaths` prop is only an add-time snapshot and is empty
  // for a game added without save tracking that ludusavi still recognises (and
  // backs up) by name. We fall back to the snapshot only while the lookup loads.
  const manifestRecognized = $derived(applicablePaths.length > 0);
  const isManifestGame = $derived(manifestRecognized || savePaths.length > 0);
  // Show the manifest picker for recognised games that aren't using a custom-folder
  // override (the two modes don't overlap in the UI). Gated on the live result
  // since the picker renders those paths.
  const showManifestPicker = $derived(manifestRecognized && !hasCustom);

  function tagLabel(tag: string): string {
    if (tag === 'save') return 'Saves';
    if (tag === 'config') return 'Settings';
    return tag.charAt(0).toUpperCase() + tag.slice(1);
  }

  // A path is dropped by tags only when it has tags and EVERY one is excluded —
  // so excluding "Settings" keeps a file tagged both Save and Settings (mirrors
  // the backend `apply_override`).
  function tagDropped(p: ManifestPath): boolean {
    return p.tags.length > 0 && p.tags.every((t) => exclTags.includes(t));
  }
  function pathSynced(p: ManifestPath): boolean {
    return !exclPaths.includes(p.template) && !tagDropped(p);
  }

  // Seed the optimistic exclusion state from the override, re-seeding when the
  // panel is pointed at another game.
  $effect(() => {
    void gameId; // re-seed only when the game changes...
    // ...reading the override untracked, so a later prop write (our own commit
    // feeding back, or any parent refresh) can't clobber a pending optimistic edit.
    const ov = untrack(() => manifestOverride);
    exclTags = ov?.excluded_tags ?? [];
    exclPaths = ov?.excluded_paths ?? [];
  });

  // Always look up the live manifest for this game — never gate on the stored
  // `savePaths` snapshot, or a game added without save tracking (empty snapshot)
  // that ludusavi actually recognises would never get checked, and its override
  // picker would never appear.
  $effect(() => {
    const id = gameId;
    manifestLoading = true;
    api
      .manifestSaveLocations(id)
      .then((paths) => {
        // Ignore a stale response if the panel was re-pointed at another game.
        if (id === gameId) manifestPaths = paths;
      })
      .catch((e) => console.error('[saves] manifestSaveLocations failed:', e))
      .finally(() => {
        if (id === gameId) manifestLoading = false;
      });
  });

  // A SAVES toast; the label flips to "FAILED" for errors. Closes over catalog.
  function notify(kind: 'ok' | 'bad' | 'info', title: string, sub: string) {
    toasts.show({
      kind,
      label: kind === 'bad' ? 'SAVES · FAILED' : 'SAVES',
      title,
      sub,
      catalog: fmtCatalog(catalogNumber),
    });
  }

  // ── Manifest override (staged) ─────────────────────────────────────────────
  // Toggles only stage locally and report the new override up to the parent —
  // they are NOT persisted here. The editor's "Save changes" button commits them
  // (one backup for the whole edit, not one per tick); see edit/+page.svelte.
  function reportOverride() {
    onOverrideChange(overrideActive ? { excluded_tags: exclTags, excluded_paths: exclPaths } : null);
  }

  function toggleTag(tag: string) {
    exclTags = exclTags.includes(tag)
      ? exclTags.filter((t) => t !== tag)
      : [...exclTags, tag];
    reportOverride();
  }

  function togglePath(p: ManifestPath) {
    if (tagDropped(p)) {
      // Promote tag-level exclusion to individual control: un-exclude the tag
      // and individually exclude the other paths that had it, so they stay
      // excluded while this one becomes synced.
      const droppingTags = p.tags.filter((t) => exclTags.includes(t));
      exclTags = exclTags.filter((t) => !droppingTags.includes(t));
      const toAdd = applicablePaths
        .filter(
          (other) =>
            other.template !== p.template &&
            other.tags.some((t) => droppingTags.includes(t)) &&
            !exclPaths.includes(other.template),
        )
        .map((other) => other.template);
      exclPaths = [...exclPaths, ...toAdd];
      reportOverride();
      return;
    }
    exclPaths = exclPaths.includes(p.template)
      ? exclPaths.filter((t) => t !== p.template)
      : [...exclPaths, p.template];
    reportOverride();
  }

  // ── Custom save ────────────────────────────────────────────────────────────
  async function pickSaveFolder() {
    let defaultPath: string | undefined;
    try {
      defaultPath = (await api.savePickerStartDir(gameId)) ?? undefined;
    } catch (e) {
      console.error('[saves] savePickerStartDir failed:', e);
    }
    let picked: Awaited<ReturnType<typeof openDialog>>;
    try {
      picked = await openDialog({
        title: 'Pick the save folder',
        directory: true,
        multiple: false,
        defaultPath,
      });
    } catch (e) {
      console.error('[saves] folder picker failed:', e);
      return;
    }
    if (typeof picked !== 'string') return;
    try {
      saveTemplate = await api.deriveSaveTemplate(gameId, picked);
    } catch (e) {
      console.error('[saves] deriveSaveTemplate failed:', e);
      saveTemplate = picked; // fall back to the literal path
    }
  }

  // Persist the full file list (or clear when it's empty). Shared by add/remove.
  async function commit(next: string[]): Promise<boolean> {
    savesBusy = true;
    try {
      if (next.length === 0) {
        await api.clearCustomSave(gameId);
        onChange(null);
      } else {
        await api.setCustomSave(gameId, next, registry);
        onChange({ files: next, registry });
      }
      return true;
    } catch (e) {
      notify('bad', "Couldn't update save locations", String(e));
      return false;
    } finally {
      savesBusy = false;
    }
  }

  async function addPath() {
    const token = saveTemplate.trim();
    if (!token || savesBusy) return;
    if (files.includes(token)) {
      saveTemplate = '';
      notify('info', 'Already added', token);
      return;
    }
    if (await commit([...files, token])) {
      saveTemplate = '';
      notify('ok', 'Save location added', `${token} — synced to your devices`);
    }
  }

  async function removePath(token: string) {
    if (savesBusy) return;
    if (await commit(files.filter((f) => f !== token))) {
      notify('info', 'Save location removed', token);
    }
  }

  async function stopTracking() {
    if (savesBusy) return;
    if (await commit([])) {
      notify(
        'info',
        'Stopped tracking custom saves',
        'Saves are no longer backed up for this game.',
      );
    }
  }
</script>

{#snippet field(label: string, helper: string, control: Snippet)}
  <EditRow {label} {helper} children={control} />
{/snippet}

{@render field('Save tracking', '', savesStatus)}

{#if showManifestPicker}
  {@render field(
    'What syncs',
    'Choose which of this game’s save locations sync across your devices. Turn off settings to keep per-device options (e.g. graphics) from being overwritten.',
    manifestPicker,
  )}
{/if}

{@render field(
  showManifestPicker ? 'Extra folders' : 'Save locations',
  showManifestPicker
    ? 'Add a folder ludusavi doesn’t know about — it’s tracked on top of the manifest locations above.'
    : 'Folders Spool backs up for this game. Add one for each place it saves — they sync to all your devices.',
  savesList,
)}

{#snippet savesStatus()}
  <span class="text-[11.5px] text-ink-2">
    {#if hasCustom}
      Custom — tracked and synced across your devices.
    {:else if overrideActive}
      Manifest — some locations excluded; only the ones you picked sync.
    {:else if isManifestGame}
      Tracked automatically via the ludusavi manifest.
    {:else if manifestLoading}
      Checking whether ludusavi recognises this game…
    {:else}
      Not tracked — ludusavi doesn't recognise this game. Add a folder below to
      back up and sync its saves.
    {/if}
  </span>
{/snippet}

{#snippet manifestPicker()}
  <div class="flex flex-col gap-2.5">
    {#if manifestLoading}
      <span class="text-[11.5px] text-ink-3">Loading manifest locations…</span>
    {:else if applicablePaths.length === 0}
      <span class="text-[11.5px] text-ink-3">
        ludusavi lists this game but declares no save locations for this platform.
      </span>
    {:else}
      <!-- Tag toggles: bulk include/exclude a whole category. -->
      <div class="flex flex-wrap gap-1.5">
        {#each distinctTags as tag (tag)}
          {@const excluded = exclTags.includes(tag)}
          <button
            type="button"
            onclick={() => toggleTag(tag)}
            aria-pressed={!excluded}
            class="cursor-pointer rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 {excluded
              ? 'border-line-2 bg-bg-1 text-ink-3 line-through'
              : 'border-spool/40 bg-spool/10 text-ink-0'}"
            title={excluded ? `Click to sync ${tagLabel(tag)}` : `Click to skip ${tagLabel(tag)}`}
          >
            {tagLabel(tag)}
          </button>
        {/each}
      </div>

      <!-- Per-path checkboxes for fine-grained control. -->
      <ul class="flex flex-col gap-1">
        {#each applicablePaths as p (p.template)}
          <li
            class="flex items-center gap-2 rounded-[4px] border border-line-1 bg-bg-1 px-2.5 py-1.5"
          >
            <input
              type="checkbox"
              checked={pathSynced(p)}
              onchange={() => togglePath(p)}
              aria-label={`Sync ${p.pretty}`}
              class="shrink-0 accent-[var(--color-spool)]"
            />
            <span
              class="font-mono min-w-0 flex-1 break-all text-[11px] {pathSynced(p)
                ? 'text-ink-1'
                : 'text-ink-3 line-through'}">{p.pretty}</span
            >
            {#each p.tags as t (t)}
              <span
                class="shrink-0 rounded-[3px] border border-line-2 px-1.5 py-0.5 text-[9.5px] uppercase tracking-wide text-ink-3"
                >{tagLabel(t)}</span
              >
            {/each}
          </li>
        {/each}
      </ul>

      {#if overrideActive}
        <div
          class="flex items-start gap-2 rounded-[4px] border border-line-1 bg-bg-1 px-2.5 py-2 text-[11px] leading-relaxed text-ink-3"
        >
          <Info size={13} class="mt-0.5 shrink-0" />
          <span>
            Applied when you <em>Save changes</em> — Spool then re-backs-up so the
            excluded locations stop syncing. A backup you restore from <em>before</em>
            that can still bring the old data back until older backups age out.
          </span>
        </div>
      {/if}
    {/if}
  </div>
{/snippet}

{#snippet savesList()}
  <div class="flex flex-col gap-2">
    {#if usesProton && !prefixReady}
      <div
        class="flex items-start gap-2 rounded-[4px] border border-warn/30 bg-warn/10 px-2.5 py-2 text-[11.5px] leading-relaxed text-ink-1"
      >
        <Info size={14} class="mt-0.5 shrink-0 text-warn" />
        <span>
          Launch this game once first — its Proton prefix and save folder are
          created on the first run. After you've played and made a save, come back
          and browse to it. You can also type a template like
          <span class="font-mono text-ink-0">&lt;winLocalAppData&gt;/Game</span>
          now.
        </span>
      </div>
    {/if}

    {#if files.length > 0}
      <ul class="flex flex-col gap-1">
        {#each files as f (f)}
          <li
            class="flex items-center gap-2 rounded-[4px] border border-line-1 bg-bg-1 px-2.5 py-1.5"
          >
            <span class="font-mono min-w-0 flex-1 break-all text-[11px] text-ink-1">{f}</span>
            <button
              type="button"
              onclick={() => removePath(f)}
              disabled={savesBusy}
              aria-label="Remove this save location"
              title="Remove"
              class="shrink-0 cursor-pointer rounded-[3px] p-1 text-ink-3 transition-colors hover:bg-white/5 hover:text-bad disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Trash2 size={13} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <div class="flex gap-1.5">
      <TextField bind:value={saveTemplate} mono full placeholder="<winLocalAppData>/MyGame" />
      <Btn variant="ghost" onclick={pickSaveFolder}>
        {#snippet icon()}<Folder size={14} />{/snippet}
        Browse
      </Btn>
      <Btn variant="ghost" onclick={addPath} disabled={savesBusy || !saveTemplate.trim()}>
        {#snippet icon()}<Plus size={14} />{/snippet}
        Add
      </Btn>
    </div>

    {#if hasCustom}
      <div class="flex justify-end pt-0.5">
        <Btn variant="ghost" onclick={stopTracking} disabled={savesBusy}>
          {#snippet icon()}<FolderX size={14} />{/snippet}
          Stop tracking
        </Btn>
      </div>
    {/if}
  </div>
{/snippet}
