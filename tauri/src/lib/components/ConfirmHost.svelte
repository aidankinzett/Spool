<script lang="ts">
  /**
   * Renders the active confirmation dialog from the global `confirms` store.
   *
   * Mounted once in `+layout.svelte` so every window shares one dialog host.
   * The dialog is purely presentational — it reads `confirms.current` and calls
   * `confirms.confirm()` / `confirms.cancel()`, which resolve the promise the
   * caller is awaiting (see `confirm.svelte.ts`). Mirrors the design language of
   * CloudConflictModal / RemoveGameModal (graphite surface, per-game accent,
   * cassette chrome, mono labels).
   */
  import { X } from '@lucide/svelte';
  import { confirms } from '$lib/confirm.svelte';
  import { shadeHex } from '$lib/tokens';
  import SpoolMark from '$lib/components/SpoolMark.svelte';
  import CatalogId from '$lib/components/CatalogId.svelte';
  import { gamepadScope } from '$lib/gamepad';

  const BRAND_SPOOL = '#d7c9a0';

  const cur = $derived(confirms.current);
  const acc = $derived(cur?.accent ?? BRAND_SPOOL);
  const danger = $derived(!!cur?.danger);
  const ctaCol = $derived(danger ? 'var(--color-bad)' : acc);
  // Split body on blank lines into paragraphs so multi-line prompts read clean.
  const paragraphs = $derived(
    (cur?.body ?? '')
      .split('\n')
      .map((s) => s.trim())
      .filter(Boolean),
  );

  let hover = $state<Record<string, boolean>>({});

  function handleKey(e: KeyboardEvent) {
    if (!cur) return;
    if (e.key === 'Escape') confirms.cancel();
    else if (e.key === 'Enter') confirms.confirm();
  }
</script>

<svelte:window onkeydown={handleKey} />

{#if cur}
  <div
    class="cf-scrim fixed inset-0 z-[60] flex items-center justify-center"
    style:padding="24px"
    style:background="rgba(4,5,7,0.62)"
    style:backdrop-filter="blur(2px)"
    style:-webkit-backdrop-filter="blur(2px)"
    onclick={(e) => {
      if (e.target === e.currentTarget) confirms.cancel();
    }}
    role="presentation"
  >
    <div
      class="cf-modal flex flex-col overflow-hidden text-ink-0"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="cf-title"
      use:gamepadScope={{ onBack: () => confirms.cancel() }}
      style:--gp-focus={acc}
      style:width="460px"
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
        {#if cur.label}
          <span class="text-ink-3" style:font-size="10px">/</span>
          <span
            class="font-mono whitespace-nowrap uppercase"
            style:font-size="10.5px"
            style:letter-spacing="0.12em"
            style:color={danger ? 'var(--color-bad)' : 'var(--color-ink-2)'}>{cur.label}</span
          >
        {/if}
        <div class="flex-1"></div>
        {#if cur.catalog}
          <CatalogId id={cur.catalog} accent={cur.accent ?? undefined} />
        {/if}
        <button
          type="button"
          onclick={() => confirms.cancel()}
          aria-label="Close"
          class="inline-flex items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-bad/20 hover:text-[#ff9b9b]"
          style:width="28px"
          style:height="22px"
        >
          <X size={11} />
        </button>
      </div>

      <!-- body -->
      <div style:padding="20px 24px 18px">
        <h1
          id="cf-title"
          class="font-display"
          style:margin="0"
          style:font-size="20px"
          style:font-weight="700"
          style:letter-spacing="-0.018em"
          style:line-height="1.12"
        >
          {cur.title}
        </h1>
        {#each paragraphs as p, i (i)}
          <p
            style:margin={i === 0 ? '10px 0 0' : '8px 0 0'}
            style:font-size="13px"
            style:color="var(--color-ink-2)"
            style:line-height="1.5"
          >
            {p}
          </p>
        {/each}
      </div>

      <!-- footer -->
      <div
        class="flex items-center gap-2.5"
        style:padding="14px 24px 18px"
        style:border-top="1px solid var(--color-line-1)"
        style:background="rgba(0,0,0,0.18)"
      >
        <div class="flex-1"></div>
        <button
          type="button"
          onclick={() => confirms.cancel()}
          data-gp-autofocus=""
          class="inline-flex items-center justify-center whitespace-nowrap rounded-sm font-medium transition-colors duration-100"
          style:height="34px"
          style:padding-inline="14px"
          style:font-size="13px"
          style:color="var(--color-ink-2)"
          style:border="1px solid var(--color-line-1)"
          style:cursor="pointer"
          style:background={hover['cancel'] ? 'rgb(255 255 255 / 0.06)' : 'transparent'}
          onmouseenter={() => (hover['cancel'] = true)}
          onmouseleave={() => (hover['cancel'] = false)}
        >
          {cur.cancelLabel ?? 'Cancel'}
        </button>
        <button
          type="button"
          onclick={() => confirms.confirm()}
          class="inline-flex items-center justify-center whitespace-nowrap rounded-sm font-medium transition-colors duration-100"
          style:height="34px"
          style:min-width="120px"
          style:padding-inline="16px"
          style:font-size="13px"
          style:color={danger ? '#fff' : '#0b0c0e'}
          style:border="1px solid transparent"
          style:cursor="pointer"
          style:background={hover['confirm'] ? shadeHex(ctaCol, -10) : ctaCol}
          onmouseenter={() => (hover['confirm'] = true)}
          onmouseleave={() => (hover['confirm'] = false)}
        >
          {cur.confirmLabel ?? 'Confirm'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .cf-scrim {
    animation: cf-fade 140ms ease;
  }
  .cf-modal {
    animation: cf-pop 180ms ease;
  }
  @keyframes cf-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes cf-pop {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>
