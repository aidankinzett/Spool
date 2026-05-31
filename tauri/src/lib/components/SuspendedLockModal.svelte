<script lang="ts">
  /**
   * Suspended-session override — "Play here instead".
   *
   * Shown when a launch is blocked because another device holds the play-state
   * lock but is *suspended* mid-session (e.g. a Steam Deck went to sleep). The
   * server keeps such a lock alive for a bounded grace window, so it can't be
   * acquired normally — but the user can knowingly override it. This is a
   * destructive confirm: stealing the lock and playing here means the sleeping
   * device's unsaved (un-backed-up) progress can be overwritten on its next
   * backup. The copy makes that explicit.
   *
   * On confirm we re-launch with `steal = true`; the run:phase events then
   * drive the rest of the UI as usual.
   */
  import { AlertTriangle, MoonStar, X } from '@lucide/svelte';
  import Btn from '$lib/components/Btn.svelte';
  import SpoolMark from '$lib/components/SpoolMark.svelte';
  import CatalogId from '$lib/components/CatalogId.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    gameName,
    deviceName,
    catalogId = undefined,
    accent = null,
    coverUrl = null,
    context = 'desktop',
    onConfirm,
    onCancel,
  }: {
    /** Display name of the game being launched. */
    gameName: string;
    /** Name of the sleeping device currently holding the lock. */
    deviceName: string;
    /** Pre-formatted catalog id ("SPL-0028"). Hidden when omitted. */
    catalogId?: string;
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

  const acc = $derived(accent ?? BRAND_SPOOL);
  // Guard against a double-fire while the launch is kicking off.
  let confirming = $state(false);

  function confirm() {
    if (confirming) return;
    confirming = true;
    onConfirm();
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !confirming) onCancel();
  }
</script>

<svelte:window onkeydown={handleKey} />

<div
  class="fixed inset-0 z-50 flex items-center justify-center sl-scrim"
  style:padding="24px"
  style:background={context === 'desktop' ? 'rgba(4,5,7,0.62)' : 'rgba(4,5,7,0.5)'}
  style:backdrop-filter="blur(2px)"
  style:-webkit-backdrop-filter="blur(2px)"
>
  <div
    class="flex flex-col overflow-hidden text-ink-0 sl-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="sl-modal-title"
    style:width="560px"
    style:max-width="calc(100vw - 48px)"
    style:background="var(--color-bg-0)"
    style:border-radius="8px"
    style:box-shadow="0 32px 80px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.07)"
  >
    <!-- chrome -->
    <div
      class="flex items-center gap-3"
      style:height="32px"
      style:padding="0 8px 0 14px"
      style:background="rgba(0,0,0,0.32)"
      style:border-bottom="1px solid var(--color-line-1)"
    >
      <SpoolMark size={18} color="var(--color-ink-1)" tape={acc} />
      <span class="font-mono uppercase text-ink-2" style:font-size="10.5px" style:letter-spacing="0.12em">SPOOL</span>
      <span class="text-ink-3" style:font-size="10px">/</span>
      <span
        class="font-mono whitespace-nowrap uppercase"
        style:font-size="10.5px"
        style:letter-spacing="0.12em"
        style:color="var(--color-warn)">LOCK · SUSPENDED</span
      >
      <div class="flex-1"></div>
      <button
        type="button"
        onclick={onCancel}
        disabled={confirming}
        aria-label="Close"
        class="inline-flex items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-bad/20 hover:text-[#ff9b9b] disabled:pointer-events-none disabled:opacity-50"
        style:width="28px"
        style:height="22px"
      >
        <X size={11} />
      </button>
    </div>

    <!-- hero -->
    <div
      class="flex items-start gap-[18px]"
      style:padding="20px 24px 18px"
      style:border-bottom="1px solid var(--color-line-1)"
    >
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2" style:margin-bottom="9px">
          {#if catalogId}<CatalogId id={catalogId} accent={accent ?? undefined} />{/if}
          <span
            class="font-mono inline-flex items-center gap-1.5 whitespace-nowrap uppercase"
            style:font-size="9.5px"
            style:letter-spacing="0.12em"
            style:color="var(--color-warn)"
          >
            <MoonStar size={12} />
            DEVICE ASLEEP
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
          <strong class="font-semibold text-ink-1">{deviceName}</strong> is asleep partway through a
          session and still holds the save lock.
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
          Playing here takes over the lock. If {deviceName} hadn’t backed up its latest progress before
          sleeping, <strong class="font-semibold" style:color="var(--color-warn)">that progress can be
          overwritten</strong> when it wakes and syncs. Only continue if you’re sure it’s done with.
        </span>
      </div>
    </div>

    <!-- footer -->
    <div class="flex items-center gap-2.5" style:padding="14px 24px 20px">
      <div class="flex-1"></div>
      <Btn variant="ghost" onclick={onCancel}>Cancel</Btn>
      <Btn variant="danger" onclick={confirm}>
        {confirming ? 'Starting…' : 'Play here anyway'}
      </Btn>
    </div>
  </div>
</div>

<style>
  .sl-scrim {
    animation: sl-fade 160ms ease;
  }
  .sl-modal {
    animation: sl-pop 200ms ease;
  }
  @keyframes sl-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes sl-pop {
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
