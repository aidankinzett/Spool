<script lang="ts">
  /**
   * Unsynced-session override — "Play here instead".
   *
   * Shown when a launch is blocked because another device has a session for this
   * game whose saves aren't in the cloud yet — it's actively playing, asleep
   * mid-session, or exited but hasn't finished uploading. The user can knowingly
   * override it. This is a destructive confirm: playing here overwrites that
   * device's session marker, and if its latest progress never reached the cloud
   * it can be lost. The copy makes that explicit.
   *
   * On confirm we re-launch with `steal = true`; the run:phase events then
   * drive the rest of the UI as usual.
   */
  import { AlertTriangle, MoonStar } from '@lucide/svelte';
  import Btn from '$lib/components/Btn.svelte';
  import ModalShell from '$lib/components/ModalShell.svelte';

  let {
    gameName,
    deviceName,
    accent = null,
    coverUrl = null,
    context = 'desktop',
    onConfirm,
    onCancel,
  }: {
    /** Display name of the game being launched. */
    gameName: string;
    /** Name of the other device with the unsynced session. */
    deviceName: string;
    /** Cover-art accent hex; falls back to the brand spool colour. */
    accent?: string | null;
    /** Webview-loadable cover URL (via `assetUrl`); placeholder when null. */
    coverUrl?: string | null;
    /** Surface this floats over — tweaks the scrim opacity. */
    context?: 'desktop' | 'gamemode';
    /** User chose to override — play here, stealing the suspended lock. */
    onConfirm: () => void;
    /** User backed out. */
    onCancel: () => void;
  } = $props();

  // Guard against a double-fire while the launch is kicking off.
  let confirming = $state(false);

  function confirm() {
    if (confirming) return;
    confirming = true;
    // onConfirm is fire-and-forget (typed void) but is usually async and can
    // reject — e.g. the launch it kicks off throws. The host normally unmounts
    // us on success, so this only matters on failure: reset the guard so the
    // modal re-enables instead of stranding with a dead "Starting…" button and
    // Escape / Back / close all gated off by `confirming`. (#289)
    Promise.resolve(onConfirm()).catch(() => {
      confirming = false;
    });
  }
</script>

<ModalShell
  breadcrumb="SESSION · UNSYNCED"
  {accent}
  {context}
  width="560px"
  closeDisabled={confirming}
  onClose={onCancel}
  ariaLabelledBy="sl-modal-title"
>
  <!-- hero -->
  <div
    class="flex items-start gap-[18px]"
    style:padding="20px 24px 18px"
    style:border-bottom="1px solid var(--color-line-1)"
  >
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2" style:margin-bottom="9px">
        <span
          class="font-mono inline-flex items-center gap-1.5 whitespace-nowrap uppercase"
          style:font-size="9.5px"
          style:letter-spacing="0.12em"
          style:color="var(--color-warn)"
        >
          <MoonStar size={12} />
          SAVES NOT IN CLOUD
        </span>
      </div>
      <h1
        id="sl-modal-title"
        class="font-display"
        style:margin="0"
        style:font-size="24px"
        style:font-weight="700"
        style:letter-spacing="-0.02em"
        style:line-height="1.08"
      >
        Play here instead?
      </h1>
      <div style:margin-top="6px" style:font-size="13.5px" style:color="var(--color-ink-1)" style:font-weight="500">
        {gameName}
      </div>
      <p
        style:margin="10px 0 0"
        style:font-size="13px"
        style:color="var(--color-ink-2)"
        style:line-height="1.5"
        style:max-width="400px"
      >
        <strong class="font-semibold text-ink-1">{deviceName}</strong> has a session for this game
        whose latest saves haven't reached the cloud yet.
      </p>
    </div>
    <div
      class="shrink-0 overflow-hidden rounded-sm border border-line-1 bg-bg-2"
      style:width="62px"
      style:height="88px"
    >
      {#if coverUrl}
        <img src={coverUrl} alt={gameName} class="h-full w-full object-cover" />
      {:else}
        <div class="h-full w-full" style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"></div>
      {/if}
    </div>
  </div>

  <!-- data-loss warning -->
  <div style:padding="18px 24px 8px">
    <div
      class="flex items-start gap-2.5 rounded-sm"
      style:padding="11px 13px"
      style:border="1px solid color-mix(in srgb, var(--color-warn) 22%, transparent)"
      style:background="linear-gradient(90deg, color-mix(in srgb, var(--color-warn) 9%, transparent), color-mix(in srgb, var(--color-warn) 2%, transparent) 60%, transparent)"
    >
      <span class="mt-0.5 flex shrink-0" style:color="var(--color-warn)"><AlertTriangle size={15} /></span>
      <span class="flex-1" style:font-size="12.5px" style:color="var(--color-ink-1)" style:line-height="1.5">
        The safe move is to open {deviceName}, close the game, and let it sync first. Playing here
        instead restores the older cloud save, so <strong class="font-semibold" style:color="var(--color-warn)">that
        device's unsynced progress can be lost</strong>. Only continue if you're sure it's done with.
      </span>
    </div>
  </div>

  <!-- footer -->
  <div class="flex items-center gap-2.5" style:padding="14px 24px 20px">
    <div class="flex-1"></div>
    <Btn variant="ghost" onclick={onCancel} disabled={confirming}>Cancel</Btn>
    <Btn variant="danger" onclick={confirm}>
      {confirming ? 'Starting…' : 'Play here anyway'}
    </Btn>
  </div>
</ModalShell>
