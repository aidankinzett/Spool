<script lang="ts">
  /**
   * Renders the active confirmation dialog from the global `confirms` store.
   *
   * Mounted once in `+layout.svelte` so every window shares one dialog host.
   * The dialog is purely presentational — it reads `confirms.current` and calls
   * `confirms.confirm()` / `confirms.cancel()`, which resolve the promise the
   * caller is awaiting (see `confirm.svelte.ts`). Built on ModalShell (graphite
   * surface, per-game accent, cassette chrome, mono labels) like the other
   * in-app modals; it adds the CatalogId in the header and an alertdialog role.
   */
  import { confirms } from '$lib/confirm.svelte';
  import { shadeHex } from '$lib/tokens';
  import CatalogId from '$lib/components/CatalogId.svelte';
  import ModalShell from '$lib/components/ModalShell.svelte';

  const cur = $derived(confirms.current);
  const danger = $derived(!!cur?.danger);
  const acc = $derived(cur?.accent ?? '#d7c9a0');
  const ctaCol = $derived(danger ? 'var(--color-bad)' : acc);
  // Split body on blank lines into paragraphs so multi-line prompts read clean.
  const paragraphs = $derived(
    (cur?.body ?? '')
      .split('\n')
      .map((s) => s.trim())
      .filter(Boolean),
  );

  let hover = $state<Record<string, boolean>>({});
</script>

{#if cur}
  <!-- Escape cancels (ModalShell onClose). Enter is deliberately NOT a global
       confirm: Cancel is the autofocused/default target for safety, so a
       reflexive Enter would otherwise fire the (possibly destructive) confirm.
       Enter/Space already activate whichever button has focus. (#285) -->
  <ModalShell
    role="alertdialog"
    breadcrumb={cur.label ?? ''}
    breadcrumbColor={danger ? 'var(--color-bad)' : 'var(--color-ink-2)'}
    accent={cur.accent ?? null}
    width="460px"
    dismissOnScrimClick
    onClose={() => confirms.cancel()}
    ariaLabelledBy="cf-title"
  >
    {#snippet headerExtra()}
      {#if cur.catalog}
        <CatalogId id={cur.catalog} accent={cur.accent ?? undefined} />
      {/if}
    {/snippet}

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
  </ModalShell>
{/if}
