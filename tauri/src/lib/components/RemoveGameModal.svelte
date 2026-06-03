<script lang="ts" module>
  export type RemoveChoice = 'library' | 'disk';
</script>

<script lang="ts">
  /**
   * Remove-game chooser — in-app popup for the GameDetail "Remove" action.
   *
   * Presents the two ways to get a game out of the library:
   *   - Remove from library: forget the entry, leave files + save backups alone.
   *   - Delete from disk: forget the entry *and* recursively delete the install
   *     folder (and, on Linux, the game's Proton prefix). Destructive, can't be
   *     undone — only offered when an install folder is on record.
   *
   * Self-driven over a small state machine (choose → working → error). The
   * actual `removeGame` / `deleteGameFromDisk` IPC is delegated to the `perform`
   * callback so this component stays presentational. Mirrors the design
   * language of CloudConflictModal (graphite surface, per-game accent, mono
   * labels, cassette chrome).
   */
  import { BookMarked, HardDrive, Trash2, X } from '@lucide/svelte';
  import { shadeHex } from '$lib/tokens';
  import SpoolMark from '$lib/components/SpoolMark.svelte';
  import CatalogId from '$lib/components/CatalogId.svelte';
  import { gamepadScope } from '$lib/gamepad';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    gameName,
    catalogId = undefined,
    accent = null,
    coverUrl = null,
    folderPath = null,
    perform,
    onClose,
  }: {
    /** Display name of the game being removed. */
    gameName: string;
    /** Pre-formatted catalog id ("SPL-0028"). Hidden when omitted. */
    catalogId?: string;
    /** Cover-art accent hex; falls back to the brand spool colour. */
    accent?: string | null;
    /** Webview-loadable cover URL (via `assetUrl`); placeholder when null. */
    coverUrl?: string | null;
    /** Install folder on record; the disk-delete option is disabled without it. */
    folderPath?: string | null;
    /** Run the chosen removal. Resolve → close; throw → `error` step. */
    perform: (choice: RemoveChoice) => Promise<void>;
    /** Dismiss without removing (Cancel / Escape / close / scrim). */
    onClose: () => void;
  } = $props();

  const acc = $derived(accent ?? BRAND_SPOOL);
  const canDelete = $derived(!!folderPath && folderPath.trim().length > 0);

  type Phase = 'choose' | 'working' | 'error';
  let phase = $state<Phase>('choose');
  let selected = $state<RemoveChoice>('library');
  let errorMsg = $state('');
  let hover = $state<Record<string, boolean>>({});

  const locked = $derived(phase === 'working');

  function pick(choice: RemoveChoice) {
    if (locked) return;
    if (choice === 'disk' && !canDelete) return;
    selected = choice;
  }

  async function confirm() {
    if (locked) return;
    phase = 'working';
    try {
      await perform(selected);
      onClose();
    } catch (e) {
      errorMsg = String(e);
      phase = 'error';
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !locked) onClose();
  }

  interface CardModel {
    key: RemoveChoice;
    label: string;
    sub: string;
    danger: boolean;
    disabled: boolean;
  }

  const cards = $derived<CardModel[]>([
    {
      key: 'library',
      label: 'Remove from library',
      sub: 'Forget this entry. Your files on disk and save backups are left untouched — you can add it again later.',
      danger: false,
      disabled: false,
    },
    {
      key: 'disk',
      label: 'Delete from disk',
      sub: canDelete
        ? "Forget the entry and permanently delete the install folder. This can't be undone."
        : 'No install folder is on record for this game, so there’s nothing to delete.',
      danger: true,
      disabled: !canDelete,
    },
  ]);

  const danger = $derived(selected === 'disk');
  const ctaCol = $derived(danger ? 'var(--color-bad)' : acc);
  const confirmLabel = $derived(
    phase === 'working'
      ? selected === 'disk'
        ? 'Deleting…'
        : 'Removing…'
      : selected === 'disk'
        ? 'Delete from disk'
        : 'Remove from library',
  );
</script>

<svelte:window onkeydown={handleKey} />

{#snippet choiceCard(card: CardModel)}
  {@const active = selected === card.key}
  {@const dangerCol = 'var(--color-bad)'}
  {@const borderCol = card.disabled
    ? 'var(--color-line-1)'
    : active
      ? card.danger
        ? dangerCol
        : acc
      : hover[`card-${card.key}`]
        ? 'var(--color-line-3)'
        : 'var(--color-line-2)'}
  {@const bg = active
    ? card.danger
      ? 'color-mix(in srgb, var(--color-bad) 9%, var(--color-bg-1))'
      : `${acc}12`
    : hover[`card-${card.key}`] && !card.disabled
      ? 'var(--color-bg-2)'
      : 'var(--color-bg-1)'}
  <button
    type="button"
    onclick={() => pick(card.key)}
    data-gp-autofocus={active ? '' : undefined}
    onmouseenter={() => (hover[`card-${card.key}`] = true)}
    onmouseleave={() => (hover[`card-${card.key}`] = false)}
    disabled={card.disabled || locked}
    class="relative flex flex-col items-start gap-2.5 overflow-hidden rounded-md p-0 text-left transition-[background,border-color,opacity] duration-150"
    style:background={bg}
    style:border="1px solid {borderCol}"
    style:opacity={card.disabled ? 0.5 : 1}
    style:cursor={card.disabled || locked ? 'default' : 'pointer'}
    style:box-shadow={active
      ? card.danger
        ? `0 0 0 1px ${dangerCol}55, 0 8px 26px rgb(0 0 0 / 0.28)`
        : `0 0 0 1px ${acc}66, 0 8px 26px ${acc}1f`
      : 'none'}
  >
    <div class="flex w-full items-start gap-3" style:padding="13px 14px 14px">
      <span
        class="mt-0.5 inline-flex shrink-0 items-center justify-center rounded-sm"
        style:width="30px"
        style:height="30px"
        style:background={active
          ? card.danger
            ? 'color-mix(in srgb, var(--color-bad) 18%, transparent)'
            : `${acc}22`
          : 'var(--color-bg-3)'}
        style:color={card.danger
          ? active
            ? dangerCol
            : 'var(--color-ink-1)'
          : active
            ? acc
            : 'var(--color-ink-1)'}
      >
        {#if card.danger}
          <Trash2 size={15} />
        {:else}
          <BookMarked size={15} />
        {/if}
      </span>
      <div class="min-w-0 flex-1">
        <div
          class="font-sans font-semibold"
          style:font-size="13.5px"
          style:color={card.danger && active ? dangerCol : 'var(--color-ink-0)'}
          style:letter-spacing="-0.005em"
        >
          {card.label}
        </div>
        <div style:margin-top="4px" style:font-size="12px" style:color="var(--color-ink-2)" style:line-height="1.4">
          {card.sub}
        </div>
        {#if card.key === 'disk' && canDelete}
          <div
            class="font-mono mt-2 flex items-center gap-1.5 truncate"
            style:font-size="10px"
            style:letter-spacing="0.02em"
            style:color="var(--color-ink-3)"
            title={folderPath ?? undefined}
          >
            <HardDrive size={11} class="shrink-0" />
            <span class="truncate">{folderPath}</span>
          </div>
        {/if}
      </div>
    </div>
  </button>
{/snippet}

<div
  class="rg-scrim fixed inset-0 z-50 flex items-center justify-center"
  style:padding="24px"
  style:background="rgba(4,5,7,0.62)"
  style:backdrop-filter="blur(2px)"
  style:-webkit-backdrop-filter="blur(2px)"
  onclick={(e) => {
    if (e.target === e.currentTarget && !locked) onClose();
  }}
  role="presentation"
>
  <div
    class="rg-modal flex flex-col overflow-hidden text-ink-0"
    role="dialog"
    aria-modal="true"
    aria-labelledby="rg-modal-title"
    use:gamepadScope={{ onBack: () => { if (!locked) onClose(); } }}
    style:--gp-focus={acc}
    style:width="540px"
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
        class="font-mono whitespace-nowrap uppercase text-ink-2"
        style:font-size="10.5px"
        style:letter-spacing="0.12em">REMOVE · ENTRY</span
      >
      <div class="flex-1"></div>
      <button
        type="button"
        onclick={() => !locked && onClose()}
        disabled={locked}
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
        </div>
        <h1
          id="rg-modal-title"
          class="font-display"
          style:margin="0"
          style:font-size="24px"
          style:font-weight="700"
          style:letter-spacing="-0.02em"
          style:line-height="1.05"
        >
          Remove game
        </h1>
        <div style:margin-top="6px" style:font-size="13.5px" style:color="var(--color-ink-1)" style:font-weight="500">
          {gameName}
        </div>
        <p
          style:margin="9px 0 0"
          style:font-size="13px"
          style:color="var(--color-ink-2)"
          style:line-height="1.5"
          style:max-width="380px"
        >
          Choose what happens to this game.
        </p>
      </div>
      <div
        class="shrink-0 overflow-hidden rounded-sm border border-line-1 bg-bg-2"
        style:width="56px"
        style:height="80px"
      >
        {#if coverUrl}
          <img src={coverUrl} alt={gameName} class="h-full w-full object-cover" />
        {:else}
          <div class="h-full w-full" style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"></div>
        {/if}
      </div>
    </div>

    <!-- choice cards -->
    <div class="flex flex-col" style:padding="18px 24px 16px" style:gap="12px">
      {#each cards as card (card.key)}
        {@render choiceCard(card)}
      {/each}
    </div>

    <!-- footer -->
    <div style:padding="16px 24px 22px" style:border-top="1px solid var(--color-line-1)" style:background="rgba(0,0,0,0.18)">
      {#if phase === 'error'}
        <div
          class="mb-3 flex items-start gap-2 rounded-sm"
          style:padding="9px 12px"
          style:border="1px solid color-mix(in srgb, var(--color-bad) 28%, transparent)"
          style:background="color-mix(in srgb, var(--color-bad) 8%, transparent)"
        >
          <X size={14} class="mt-px shrink-0 text-bad" />
          <span class="flex-1" style:font-size="12px" style:color="var(--color-ink-1)" style:line-height="1.4">
            <strong class="font-semibold text-bad">Couldn’t remove.</strong>
            {errorMsg}
          </span>
        </div>
      {:else if selected === 'disk'}
        <div
          class="mb-3 flex items-center gap-2 rounded-sm"
          style:padding="9px 12px"
          style:border="1px solid color-mix(in srgb, var(--color-bad) 22%, transparent)"
          style:background="linear-gradient(90deg, color-mix(in srgb, var(--color-bad) 9%, transparent), transparent 70%)"
        >
          <span class="shrink-0 rounded-full" style:width="6px" style:height="6px" style:background="var(--color-bad)"></span>
          <span class="flex-1" style:font-size="12px" style:color="var(--color-ink-1)">
            The install folder will be <strong class="font-semibold" style:color="var(--color-bad)">permanently deleted</strong>.
          </span>
        </div>
      {/if}

      <div class="flex items-center gap-2.5">
        <div class="flex-1"></div>
        <button
          type="button"
          onclick={() => !locked && onClose()}
          disabled={locked}
          class="inline-flex items-center justify-center whitespace-nowrap rounded-sm font-medium transition-colors duration-100 disabled:opacity-50"
          style:height="34px"
          style:padding-inline="14px"
          style:font-size="13px"
          style:color="var(--color-ink-2)"
          style:border="1px solid var(--color-line-1)"
          style:cursor={locked ? 'default' : 'pointer'}
          style:background={hover['cancel'] && !locked ? 'rgb(255 255 255 / 0.06)' : 'transparent'}
          onmouseenter={() => (hover['cancel'] = true)}
          onmouseleave={() => (hover['cancel'] = false)}
        >
          Cancel
        </button>
        <button
          type="button"
          onclick={confirm}
          disabled={locked}
          class="inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-sm font-medium transition-colors duration-100 disabled:opacity-70"
          style:height="34px"
          style:min-width="170px"
          style:padding-inline="14px"
          style:font-size="13px"
          style:color={danger ? '#fff' : '#0b0c0e'}
          style:border="1px solid transparent"
          style:cursor={locked ? 'default' : 'pointer'}
          style:background={hover['confirm'] && !locked ? shadeHex(ctaCol, -10) : ctaCol}
          onmouseenter={() => (hover['confirm'] = true)}
          onmouseleave={() => (hover['confirm'] = false)}
        >
          {#if !locked}<Trash2 size={14} />{/if}
          {confirmLabel}
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  .rg-scrim {
    animation: rg-fade 160ms ease;
  }
  .rg-modal {
    animation: rg-pop 200ms ease;
  }
  @keyframes rg-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes rg-pop {
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
