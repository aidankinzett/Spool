<script lang="ts">
  /**
   * Shared shell for in-app modals (CloudConflictModal, SuspendedLockModal,
   * PeerSourceModal).
   *
   * Owns: scrim, modal box, SPOOL chrome header (SpoolMark + breadcrumb + X),
   * entry animations, Escape key handling, and gamepad back. Registers itself
   * in the module-level modal stack so z-index is assigned by stack depth and
   * Escape only fires for the topmost open modal (fixes the multi-modal
   * Escape-closes-all bug when two modals are simultaneously visible).
   *
   * Consumers supply their body content via the default snippet and control
   * close-ability via onClose / closeDisabled.
   */
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { X } from '@lucide/svelte';
  import SpoolMark from './SpoolMark.svelte';
  import { gamepadScope } from '$lib/gamepad';
  import { registerModal, unregisterModal, isTopModal, modalZIndex } from '$lib/modalStack.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    breadcrumb,
    breadcrumbColor = 'var(--color-warn)',
    accent = null,
    context = 'desktop',
    width = '540px',
    closeDisabled = false,
    onClose,
    onBack,
    ariaLabelledBy = 'modal-title',
    children,
  }: {
    /** Right-hand side of the SPOOL / _ breadcrumb in the chrome header. */
    breadcrumb: string;
    /** CSS colour for the breadcrumb label; defaults to --color-warn. */
    breadcrumbColor?: string;
    /** Cover-art accent hex used for SpoolMark tape, --gp-focus, and X hover. */
    accent?: string | null;
    /** Surface this floats over — controls scrim opacity. */
    context?: 'desktop' | 'gamemode';
    /** CSS width of the modal box; e.g. "640px". */
    width?: string;
    /**
     * When true: the X button is disabled, Escape is suppressed, and gamepad
     * back is a no-op. Use this while a destructive operation is in flight
     * (e.g. while applying a cloud-conflict resolve or starting a launch).
     */
    closeDisabled?: boolean;
    /**
     * Called by the chrome X button and the Escape key (when not closeDisabled
     * and this is the topmost open modal). Omit to hide the X button entirely.
     */
    onClose?: () => void;
    /**
     * Called by the gamepad B / back button. Defaults to onClose when absent.
     * Supply this when the back action differs from the X-button/Escape action.
     */
    onBack?: () => void;
    /** aria-labelledby value for the dialog element. */
    ariaLabelledBy?: string;
    children: Snippet;
  } = $props();

  const acc = $derived(accent ?? BRAND_SPOOL);

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
</script>

<svelte:window onkeydown={handleKey} />

<div
  class="modal-scrim fixed inset-0 flex items-center justify-center"
  style:z-index={zIndex}
  style:padding="24px"
  style:background={context === 'desktop' ? 'rgba(4,5,7,0.62)' : 'rgba(4,5,7,0.5)'}
  style:backdrop-filter="blur(2px)"
  style:-webkit-backdrop-filter="blur(2px)"
>
  <div
    class="modal-box flex flex-col overflow-hidden text-ink-0"
    role="dialog"
    aria-modal="true"
    aria-labelledby={ariaLabelledBy}
    use:gamepadScope={{ onBack: handleBack }}
    style:width={width}
    style:max-width="calc(100vw - 48px)"
    style:background="var(--color-bg-0)"
    style:border-radius="8px"
    style:box-shadow="0 32px 80px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.07)"
    style:--gp-focus={acc}
  >
    <!-- chrome header -->
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
        style:color={breadcrumbColor}>{breadcrumb}</span
      >
      <div class="flex-1"></div>
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
