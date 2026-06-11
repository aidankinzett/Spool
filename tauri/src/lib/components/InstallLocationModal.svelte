<script lang="ts">
  /**
   * First-download install-location prompt.
   *
   * Opens when a LAN download starts while no library folders are configured,
   * instead of silently installing into Spool's hidden app-data folder. The
   * pick is registered as the default-install library folder (managed in
   * Settings → Library folders from then on), after which the pending download
   * starts — so the question is asked exactly once, at the moment it matters.
   *
   * Picking is the whole interaction, like the source chooser: a drive row
   * confirms a `<drive>/Spool` folder, "Use Spool's data folder" confirms the
   * app-data fallback, Cancel abandons the download.
   *
   * Presentational apart from the two read-only lookups it opens with (the
   * drive list and the fallback path); the host owns folder registration and
   * the install kickoff.
   */
  import { onMount } from 'svelte';
  import { ChevronRight, Folder, HardDrive } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import { fmtSize } from '$lib/format';
  import type { DriveInfo } from '$lib/types';
  import ModalShell from '$lib/components/ModalShell.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    gameName,
    accent = null,
    coverUrl = null,
    context = 'desktop',
    onConfirm,
    onClose,
  }: {
    /** Display name of the game whose download is waiting on the answer. */
    gameName: string;
    /** Cover-art accent hex; falls back to the brand spool colour. */
    accent?: string | null;
    /** Webview-loadable cover URL; placeholder when null. */
    coverUrl?: string | null;
    /** Surface this floats over — only tweaks the scrim opacity. */
    context?: 'desktop' | 'gamemode';
    /** User picked a folder (`null` = the app-data fallback). */
    onConfirm: (path: string | null) => void;
    /** Dismiss via Cancel / the chrome close button / Escape / controller B —
     *  abandons the pending download. */
    onClose: () => void;
  } = $props();

  const acc = $derived(accent ?? BRAND_SPOOL);

  let hover = $state<Record<string, boolean>>({});
  let drives = $state<DriveInfo[]>([]);
  let fallbackDir = $state('');
  let confirming = $state(false);

  onMount(async () => {
    try {
      drives = await api.listDrives();
    } catch {
      drives = [];
    }
    try {
      fallbackDir = await api.defaultLanInstallDir();
    } catch {
      fallbackDir = '';
    }
  });

  /** Same seeding rule as Settings' drive picker: games live in a `Spool/`
   *  subfolder of the chosen drive, never its root. */
  function seedPath(mount: string): string {
    const isWin = mount.includes('\\');
    const trimmed = mount.replace(/[\\/]+$/, '');
    return isWin ? `${trimmed}\\Spool` : `${trimmed}/Spool`;
  }

  function confirm(path: string | null) {
    if (confirming) return;
    confirming = true;
    onConfirm(path);
  }

  async function browse() {
    const picked = await openDialog({ title: 'Pick a library folder', directory: true, multiple: false });
    if (typeof picked === 'string') confirm(picked);
  }
</script>

<ModalShell
  breadcrumb="LAN · INSTALL LOCATION"
  breadcrumbColor="var(--color-ink-2)"
  {accent}
  {context}
  width="480px"
  onClose={onClose}
  ariaLabelledBy="il-modal-title"
>
  <!-- hero -->
  <div
    class="flex items-start gap-[16px]"
    style:padding="18px 22px 16px"
    style:border-bottom="1px solid var(--color-line-1)"
  >
    <div
      class="shrink-0 overflow-hidden rounded-sm border border-line-1 bg-bg-2"
      style:width="50px"
      style:height="70px"
    >
      {#if coverUrl}
        <img src={coverUrl} alt={gameName} class="h-full w-full object-cover" />
      {:else}
        <div class="h-full w-full" style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"></div>
      {/if}
    </div>
    <div class="min-w-0 flex-1">
      <h1
        id="il-modal-title"
        class="font-display"
        style:margin="0"
        style:font-size="21px"
        style:font-weight="700"
        style:letter-spacing="-0.02em"
        style:line-height="1.1"
      >
        Where should games install?
      </h1>
      <div style:margin-top="5px" style:font-size="13px" style:color="var(--color-ink-1)" style:font-weight="500">
        {gameName}
      </div>
      <p style:margin="7px 0 0" style:font-size="12.5px" style:color="var(--color-ink-2)" style:line-height="1.45">
        Downloads land in a library folder. Pick a drive to create one — you can change it later in Settings → Library folders.
      </p>
    </div>
  </div>

  <!-- drive list -->
  <div class="flex flex-col" style:padding="12px" style:gap="8px" style:max-height="320px" style:overflow-y="auto">
    {#each drives as drive, i (drive.mount_point)}
      {@const h = hover[drive.mount_point]}
      <button
        type="button"
        onclick={() => confirm(seedPath(drive.mount_point))}
        data-gp-autofocus={i === 0 ? '' : undefined}
        onmouseenter={() => (hover[drive.mount_point] = true)}
        onmouseleave={() => (hover[drive.mount_point] = false)}
        class="group flex cursor-pointer items-center gap-3 rounded-md p-0 text-left transition-[background,border-color] duration-150"
        style:padding="11px 13px"
        style:border="1px solid {h ? acc : 'var(--color-line-2)'}"
        style:background={h ? `${acc}12` : 'var(--color-bg-1)'}
      >
        <span
          class="inline-flex shrink-0 items-center justify-center rounded-sm"
          style:width="30px"
          style:height="30px"
          style:background={`${acc}1f`}
          style:color={acc}
        >
          <HardDrive size={15} />
        </span>
        <div class="min-w-0 flex-1">
          <div class="truncate text-[13px] font-semibold text-ink-0" title={drive.mount_point}>
            {drive.name || drive.mount_point}
          </div>
          <div class="font-mono mt-0.5 flex items-center gap-2 text-[10px] tracking-[0.04em] text-ink-3">
            <span>{seedPath(drive.mount_point)}</span>
            <span>·</span>
            <span>{fmtSize(drive.available_bytes / 1048576)} free</span>
          </div>
        </div>
        <ChevronRight size={16} class="shrink-0 text-ink-3 transition-colors group-hover:text-ink-1" />
      </button>
    {/each}

    <!-- app-data fallback -->
    <button
      type="button"
      onclick={() => confirm(null)}
      onmouseenter={() => (hover['fallback'] = true)}
      onmouseleave={() => (hover['fallback'] = false)}
      class="group flex cursor-pointer items-center gap-3 rounded-md p-0 text-left transition-[background,border-color] duration-150"
      style:padding="11px 13px"
      style:border="1px solid {hover['fallback'] ? acc : 'var(--color-line-2)'}"
      style:background={hover['fallback'] ? `${acc}12` : 'var(--color-bg-1)'}
    >
      <span
        class="inline-flex shrink-0 items-center justify-center rounded-sm"
        style:width="30px"
        style:height="30px"
        style:background="var(--color-bg-3)"
        style:color="var(--color-ink-3)"
      >
        <Folder size={15} />
      </span>
      <div class="min-w-0 flex-1">
        <div class="truncate text-[13px] font-semibold text-ink-0">Use Spool's data folder</div>
        {#if fallbackDir}
          <div class="font-mono mt-0.5 truncate text-[10px] tracking-[0.04em] text-ink-3" title={fallbackDir}>
            {fallbackDir}
          </div>
        {/if}
      </div>
      <ChevronRight size={16} class="shrink-0 text-ink-3 transition-colors group-hover:text-ink-1" />
    </button>
  </div>

  <!-- footer -->
  <div
    class="flex items-center justify-between"
    style:padding="12px 22px 16px"
    style:border-top="1px solid var(--color-line-1)"
    style:background="rgba(0,0,0,0.18)"
  >
    <button
      type="button"
      onclick={browse}
      onmouseenter={() => (hover['browse'] = true)}
      onmouseleave={() => (hover['browse'] = false)}
      class="inline-flex cursor-pointer items-center justify-center gap-1.5 rounded-sm font-medium transition-colors duration-100"
      style:height="34px"
      style:padding-inline="14px"
      style:font-size="13px"
      style:color="var(--color-ink-2)"
      style:border="1px solid var(--color-line-1)"
      style:background={hover['browse'] ? 'rgb(255 255 255 / 0.06)' : 'transparent'}
    >
      <Folder size={14} />
      Browse…
    </button>
    <button
      type="button"
      onclick={onClose}
      onmouseenter={() => (hover['cancel'] = true)}
      onmouseleave={() => (hover['cancel'] = false)}
      class="inline-flex cursor-pointer items-center justify-center rounded-sm font-medium transition-colors duration-100"
      style:height="34px"
      style:padding-inline="14px"
      style:font-size="13px"
      style:color="var(--color-ink-2)"
      style:border="1px solid var(--color-line-1)"
      style:background={hover['cancel'] ? 'rgb(255 255 255 / 0.06)' : 'transparent'}
    >
      Cancel
    </button>
  </div>
</ModalShell>
