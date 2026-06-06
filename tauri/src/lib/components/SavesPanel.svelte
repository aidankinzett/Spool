<script lang="ts">
  /**
   * Saves tab of the game editor — track custom save location(s) for a game
   * ludusavi's manifest doesn't cover (or covers wrongly). Shows the current
   * locations as a list (each removable), plus an add row: pick a folder (the
   * picker opens inside the game's Proton prefix) or type a ludusavi path
   * template. A game can save in several places, so any number can be added;
   * the whole set is registered with ludusavi and replicated to the user's
   * other devices.
   *
   * Self-contained (drives `set_custom_save` / `clear_custom_save` /
   * `derive_save_template`) so it can be storied directly. `customSave` is
   * reported up via `onChange` rather than two-way bound, so the parent owns
   * the entry state. Every add/remove writes the full list through
   * `set_custom_save`; removing the last one clears tracking entirely.
   */
  import type { Snippet } from 'svelte';
  import { Folder, FolderX, Info, Plus, Trash2 } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import { fmtCatalog } from '$lib/format';
  import { toasts } from '$lib/toasts.svelte';
  import type { CustomSave } from '$lib/types';
  import Btn from './Btn.svelte';
  import TextField from './TextField.svelte';

  let {
    gameId,
    catalogNumber,
    savePaths,
    usesProton,
    prefixReady,
    customSave,
    onChange,
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
    /** Called after the custom save changes so the parent can update its entry. */
    onChange: (custom: CustomSave | null) => void;
  } = $props();

  // In-progress template for the add row (from the picker or typed), plus busy.
  let saveTemplate = $state('');
  let savesBusy = $state(false);

  const files = $derived(customSave?.files ?? []);
  const registry = $derived(customSave?.registry ?? []);
  const hasCustom = $derived(files.length > 0);

  // Pick a save folder; the backend opens the picker inside the prefix and
  // turns the chosen folder into a portable template, filled into the add row
  // so the user can review/tweak it before adding.
  async function pickSaveFolder() {
    let defaultPath: string | undefined;
    try {
      defaultPath = (await api.savePickerStartDir(gameId)) ?? undefined;
    } catch (e) {
      console.error('[saves] savePickerStartDir failed:', e);
    }
    const picked = await openDialog({
      title: 'Pick the save folder',
      directory: true,
      multiple: false,
      defaultPath,
    });
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
      toasts.show({
        kind: 'bad',
        label: 'SAVES · FAILED',
        title: "Couldn't update save locations",
        sub: String(e),
      });
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
      toasts.show({
        kind: 'info',
        label: 'SAVES',
        title: 'Already added',
        sub: token,
        catalog: fmtCatalog(catalogNumber),
      });
      return;
    }
    if (await commit([...files, token])) {
      saveTemplate = '';
      toasts.show({
        kind: 'ok',
        label: 'SAVES',
        title: 'Save location added',
        sub: `${token} — synced to your devices`,
        catalog: fmtCatalog(catalogNumber),
      });
    }
  }

  async function removePath(token: string) {
    if (savesBusy) return;
    if (await commit(files.filter((f) => f !== token))) {
      toasts.show({
        kind: 'info',
        label: 'SAVES',
        title: 'Save location removed',
        sub: token,
        catalog: fmtCatalog(catalogNumber),
      });
    }
  }

  async function stopTracking() {
    if (savesBusy) return;
    if (await commit([])) {
      toasts.show({
        kind: 'info',
        label: 'SAVES',
        title: 'Stopped tracking custom saves',
        sub: 'Saves are no longer backed up for this game.',
        catalog: fmtCatalog(catalogNumber),
      });
    }
  }
</script>

{#snippet field(label: string, helper: string, control: Snippet)}
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

{@render field('Save tracking', '', savesStatus)}
{@render field(
  'Save locations',
  "Folders Spool backs up for this game. Add one for each place it saves — they sync to all your devices.",
  savesList,
)}

{#snippet savesStatus()}
  <span class="text-[11.5px] text-ink-2">
    {#if hasCustom}
      Custom — tracked and synced across your devices.
    {:else if savePaths.length > 0}
      Tracked automatically via the ludusavi manifest.
    {:else}
      Not tracked — ludusavi doesn't recognise this game. Add a folder below to
      back up and sync its saves.
    {/if}
  </span>
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
