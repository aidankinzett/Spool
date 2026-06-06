<script lang="ts">
  /**
   * Saves tab of the game editor — track a custom save location for a game
   * ludusavi's manifest doesn't cover (or covers wrongly). Pick a folder (the
   * picker opens inside the game's Proton prefix) or type a ludusavi path
   * template; the definition is registered with ludusavi and replicated to the
   * user's other devices.
   *
   * Self-contained (drives `set_custom_save` / `clear_custom_save` /
   * `derive_save_template` itself) so it can be storied directly. `customSave`
   * is reported up via `onChange` rather than two-way bound, so the parent owns
   * the entry state. Mirrors the two-column field layout of the other tabs.
   */
  import type { Snippet } from 'svelte';
  import { Check, Folder, FolderX, Info } from '@lucide/svelte';
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
    /** Called after the custom save is set or cleared so the parent can update. */
    onChange: (custom: CustomSave | null) => void;
  } = $props();

  // In-progress template from the picker or typed directly, plus a busy flag.
  let saveTemplate = $state('');
  let savesBusy = $state(false);

  const hasCustom = $derived(!!customSave && customSave.files.length > 0);

  // Pick a save folder; the backend opens the picker inside the prefix and
  // turns the chosen folder into a portable template.
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

  async function applyCustomSave() {
    const token = saveTemplate.trim();
    if (!token || savesBusy) return;
    savesBusy = true;
    try {
      await api.setCustomSave(gameId, [token], []);
      onChange({ files: [token], registry: [] });
      saveTemplate = '';
      toasts.show({
        kind: 'ok',
        label: 'SAVES',
        title: 'Save location set',
        sub: `${token} — synced to your devices`,
        catalog: fmtCatalog(catalogNumber),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'SAVES · FAILED',
        title: "Couldn't set save location",
        sub: String(e),
      });
    } finally {
      savesBusy = false;
    }
  }

  async function clearCustomSave() {
    if (savesBusy) return;
    savesBusy = true;
    try {
      await api.clearCustomSave(gameId);
      onChange(null);
      toasts.show({
        kind: 'info',
        label: 'SAVES',
        title: 'Stopped tracking custom save',
        sub: 'Saves are no longer backed up for this game.',
        catalog: fmtCatalog(catalogNumber),
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'SAVES · FAILED',
        title: "Couldn't stop tracking",
        sub: String(e),
      });
    } finally {
      savesBusy = false;
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
{#if hasCustom}
  {@render field(
    'Custom save folder',
    'Spool backs this up after each session and syncs it to your other devices.',
    savesCurrent,
  )}
{:else}
  {@render field(
    'Set a save folder',
    "Use this when ludusavi doesn't recognise the game, or saves it in the wrong place. Pick the folder, or type a ludusavi path template. Applies to all your devices.",
    savesPicker,
  )}
{/if}

{#snippet savesStatus()}
  <span class="text-[11.5px] text-ink-2">
    {#if hasCustom}
      Custom folder — tracked and synced across your devices.
    {:else if savePaths.length > 0}
      Tracked automatically via the ludusavi manifest.
    {:else}
      Not tracked — ludusavi doesn't recognise this game. Set a folder below to
      back up and sync its saves.
    {/if}
  </span>
{/snippet}
{#snippet savesCurrent()}
  <div class="flex flex-col items-start gap-2">
    <div
      class="font-mono w-full break-all rounded-[4px] border border-line-1 bg-bg-1 px-2.5 py-2 text-[11px] text-ink-1"
    >
      {#each customSave!.files as f (f)}
        <div>{f}</div>
      {/each}
    </div>
    <Btn variant="ghost" onclick={clearCustomSave} disabled={savesBusy}>
      {#snippet icon()}<FolderX size={14} />{/snippet}
      Stop tracking
    </Btn>
  </div>
{/snippet}
{#snippet savesPicker()}
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
    <div class="flex gap-1.5">
      <TextField bind:value={saveTemplate} mono full placeholder="<winLocalAppData>/MyGame" />
      <Btn variant="ghost" onclick={pickSaveFolder}>
        {#snippet icon()}<Folder size={14} />{/snippet}
        Browse
      </Btn>
    </div>
    <div class="flex items-center justify-between">
      {#if saveTemplate.trim()}
        <span class="font-mono truncate text-[10px] text-ink-3">→ {saveTemplate.trim()}</span>
      {:else}
        <span></span>
      {/if}
      <Btn variant="ghost" onclick={applyCustomSave} disabled={savesBusy || !saveTemplate.trim()}>
        {#snippet icon()}<Check size={14} />{/snippet}
        {savesBusy ? 'Saving…' : 'Use this location'}
      </Btn>
    </div>
  </div>
{/snippet}
