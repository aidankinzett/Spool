<script lang="ts">
  /**
   * Shared shell for in-app modals (CloudConflictModal, SuspendedLockModal,
   * PeerSourceModal, ConfirmHost, OnboardingModal, …).
   *
   * Owns: scrim, modal box, SPOOL chrome header (SpoolMark + breadcrumb + X),
   * entry animations, Escape key handling, and gamepad back. Registers itself
   * in the module-level modal stack so z-index is assigned by stack depth and
   * Escape only fires for the topmost open modal (fixes the multi-modal
   * Escape-closes-all bug when two modals are simultaneously visible).
   *
   * The scrim sits at z-index 50+ (stack depth, capped below 65). In desktop UI
   * mode the WindowChrome strip deliberately renders *above* it at z-65 so the
   * window stays draggable and the controls stay clickable while a modal is open
   * (#432); the scrim therefore reserves top padding equal to --chrome-h in that
   * case so the centred box never slides under it. Touch UI mode (TouchTopBar
   * isn't lifted) and the splash (context="gamemode", no strip) don't reserve —
   * the reservation tracks uiMode.resolved, not just the context prop.
   *
   * Consumers supply their body content via the default snippet and control
   * close-ability via onClose / closeDisabled. The chrome header can be
   * customised (headerExtra) or replaced wholesale by the consumer's own layout
   * (hideHeader) — OnboardingModal uses the latter for its step-progress header.
   */
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { X } from '@lucide/svelte';
  import SpoolMark from './SpoolMark.svelte';
  import { gamepadScope } from '$lib/gamepad';
  import { uiMode } from '$lib/uiMode.svelte';
  import { registerModal, unregisterModal, isTopModal, modalZIndex } from '$lib/modalStack.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    breadcrumb = '',
    breadcrumbColor = 'var(--color-warn)',
    accent = null,
    context = 'desktop',
    width = '540px',
    height = undefined,
    surface = 'var(--color-bg-0)',
    scrimBackground = undefined,
    scrimBlur = 'blur(2px)',
    role = 'dialog',
    dismissOnScrimClick = false,
    hideHeader = false,
    closeDisabled = false,
    onClose,
    onBack,
    ariaLabelledBy = 'modal-title',
    ariaLabel = undefined,
    headerExtra,
    children,
  }: {
    /** Right-hand side of the SPOOL / _ breadcrumb in the chrome header. Omit to hide the breadcrumb. */
    breadcrumb?: string;
    /** CSS colour for the breadcrumb label; defaults to --color-warn. */
    breadcrumbColor?: string;
    /** Cover-art accent hex used for SpoolMark tape, --gp-focus, and X hover. */
    accent?: string | null;
    /** Surface this floats over — controls the default scrim opacity. */
    context?: 'desktop' | 'gamemode';
    /** CSS width of the modal box; e.g. "640px". */
    width?: string;
    /** Optional fixed CSS height of the modal box; e.g. "548px". Capped to the viewport. */
    height?: string;
    /** CSS background of the modal box. Defaults to --color-bg-0. */
    surface?: string;
    /** Override the scrim background colour. Defaults to a context-derived dark wash. */
    scrimBackground?: string;
    /** Override the scrim backdrop-filter. Defaults to blur(2px). */
    scrimBlur?: string;
    /** ARIA role for the box — "dialog" or "alertdialog" (confirmations). */
    role?: 'dialog' | 'alertdialog';
    /** When true, clicking the scrim (outside the box) calls onClose. */
    dismissOnScrimClick?: boolean;
    /** When true, the built-in SPOOL chrome header is not rendered; the consumer supplies its own. */
    hideHeader?: boolean;
    /**
     * When true: the X button is disabled, Escape is suppressed, and gamepad
     * back is a no-op. Use this while a destructive operation is in flight
     * (e.g. while applying a cloud-conflict resolve or starting a launch).
     */
    closeDisabled?: boolean;
    /**
     * Called by the chrome X button, the Escape key (when not closeDisabled
     * and this is the topmost open modal), and a scrim click when
     * dismissOnScrimClick is set. Omit to hide the X button entirely.
     */
    onClose?: () => void;
    /**
     * Called by the gamepad B / back button. Defaults to onClose when absent.
     * Supply this when the back action differs from the X-button/Escape action.
     */
    onBack?: () => void;
    /** aria-labelledby value for the dialog element. Ignored when ariaLabel is set. */
    ariaLabelledBy?: string;
    /** aria-label for the dialog element — use instead of ariaLabelledBy when there's no titled element to point at. */
    ariaLabel?: string;
    /** Optional extra header content rendered right-aligned before the X (e.g. a CatalogId). */
    headerExtra?: Snippet;
    children: Snippet;
  } = $props();

  const acc = $derived(accent ?? BRAND_SPOOL);

  const scrimBg = $derived(
    scrimBackground ?? (context === 'desktop' ? 'rgba(4,5,7,0.62)' : 'rgba(4,5,7,0.5)'),
  );
  // Reserve the title-bar strip so the box never slides under the window chrome,
  // which renders above the scrim. Only the desktop WindowChrome is lifted to
  // z-65; TouchTopBar isn't, and the splash (context="gamemode") has no strip —
  // so reserve only when the desktop chrome is actually present and on top.
  const scrimPad = $derived(
    context === 'desktop' && uiMode.resolved === 'desktop'
      ? 'calc(var(--chrome-h) + 24px) 24px 24px'
      : '24px',
  );

  const id = Symbol();
  const zIndex = $derived(modalZIndex(id));
  const isTop = $derived(isTopModal(id));

  onMount(() => {
    registerModal(id);
    return () => unregisterModal(id);
  });

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !closeDisabled && isTop) {
      onClose?.();
    }
  }

  function handleBack() {
    if (closeDisabled) return;
    (onBack ?? onClose)?.();
  }

  function handleScrimClick(e: MouseEvent) {
    if (!dismissOnScrimClick || closeDisabled) return;
    if (e.target === e.currentTarget) onClose?.();
  }
</script>

<svelte:window onkeydown={handleKey} />

<div
  class="modal-scrim fixed inset-0 flex items-center justify-center"
  style:z-index={zIndex}
  style:padding={scrimPad}
  style:background={scrimBg}
  style:backdrop-filter={scrimBlur}
  style:-webkit-backdrop-filter={scrimBlur}
  onclick={handleScrimClick}
  role="presentation"
>
  <div
    class="modal-box flex flex-col overflow-hidden text-ink-0"
    {role}
    aria-modal="true"
    aria-label={ariaLabel}
    aria-labelledby={ariaLabel ? undefined : ariaLabelledBy}
    use:gamepadScope={{ onBack: handleBack }}
    style:width={width}
    style:max-width="calc(100vw - 48px)"
    style:height={height}
    style:max-height={height ? '100%' : undefined}
    style:background={surface}
    style:border-radius="8px"
    style:box-shadow="0 32px 80px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.07)"
    style:--gp-focus={acc}
  >
    {#if !hideHeader}
      <!-- chrome header -->
      <div
        class="flex shrink-0 items-center gap-3"
        style:height="32px"
        style:padding="0 8px 0 14px"
        style:background="rgba(0,0,0,0.32)"
        style:border-bottom="1px solid var(--color-line-1)"
      >
        <SpoolMark size={18} color="var(--color-ink-1)" tape={acc} />
        <span class="font-mono uppercase text-ink-2" style:font-size="10.5px" style:letter-spacing="0.12em">SPOOL</span>
        {#if breadcrumb}
          <span class="text-ink-3" style:font-size="10px">/</span>
          <span
            class="font-mono whitespace-nowrap uppercase"
            style:font-size="10.5px"
            style:letter-spacing="0.12em"
            style:color={breadcrumbColor}>{breadcrumb}</span
          >
        {/if}
        <div class="flex-1"></div>
        {#if headerExtra}{@render headerExtra()}{/if}
        {#if onClose}
          <button
            type="button"
            onclick={() => onClose?.()}
            disabled={closeDisabled}
            aria-label="Close"
            class="inline-flex items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-bad/20 hover:text-[#ff9b9b] disabled:pointer-events-none disabled:opacity-50"
            style:width="28px"
            style:height="22px"
          >
            <X size={11} />
          </button>
        {/if}
      </div>
    {/if}

    {@render children()}
  </div>
</div>

<style>
  .modal-scrim {
    animation: modal-fade 160ms ease;
  }
  .modal-box {
    animation: modal-pop 200ms ease;
  }
  @keyframes modal-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes modal-pop {
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
