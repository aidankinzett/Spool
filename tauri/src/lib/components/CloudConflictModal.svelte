<script lang="ts" module>
  /** Save-side metadata for one copy (this device or the cloud). */
  export interface SaveMeta {
    /** Absolute timestamp, e.g. "Today · 21:40". */
    abs: string;
    /** Relative timestamp, e.g. "6 hours ago". */
    rel: string;
    /** Human save size, e.g. "0.42 MB". */
    size: string;
  }

  export type ConflictSide = 'local' | 'cloud';
  export type ConflictPhase = 'choose' | 'applying' | 'error';

  /** A resolved choice-card model (one per side). */
  interface SideModel {
    key: ConflictSide;
    label: string;
    sub: string;
    confirm: string;
    recent: boolean;
    meta: SaveMeta | null;
  }
</script>

<script lang="ts">
  /**
   * Cloud save conflict — in-app resolver.
   *
   * Replaces the "Open Ludusavi" escape hatch as the primary path when
   * ludusavi reports both the local and cloud copies changed since the last
   * sync. A blocking, destructive modal: pick which copy wins; the loser is
   * overwritten. Lives over two surfaces (desktop library window + the
   * full-screen Game Mode splash) and runs a small state machine:
   *
   *   choose → applying → (auto-close) | error
   *
   * Presentational + self-driven: the component owns the state machine and
   * card animations, but the actual upload/download work is delegated to the
   * `resolve` callback (kept out of here so the backend command can land
   * separately). While applying, the primary button shows an inline loading
   * state; on success it calls `onContinue` straight away so the host closes
   * the modal (no extra success screen). On a thrown error it falls to
   * `error`, where the working "Open Ludusavi" fallback still lives.
   *
   * Mirrors the Cloud Save Conflict design (Space Grotesk / Geist / JetBrains
   * Mono, graphite surface, per-game accent, cassette reel + tape primitives).
   */
  import { Check, Clock, Cloud, ExternalLink, HardDrive, LoaderCircle, RotateCw, X } from '@lucide/svelte';
  import { shadeHex } from '$lib/tokens';
  import ModalShell from '$lib/components/ModalShell.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    gameName,
    accent = null,
    coverUrl = null,
    cloudNewer = true,
    localMeta = null,
    cloudMeta = null,
    context = 'desktop',
    showLudusavi = true,
    resolve,
    onCancel,
    onContinue,
    onLudusavi,
    onClose,
  }: {
    /** Display name of the game in conflict. */
    gameName: string;
    /** Cover-art accent hex; falls back to the brand spool colour. */
    accent?: string | null;
    /** Webview-loadable cover URL (via `assetUrl`); placeholder when null. */
    coverUrl?: string | null;
    /** Which side is the most recent revision — drives the "MOST RECENT" tag. */
    cloudNewer?: boolean;
    /** Local-copy metadata; detail card renders if either side has metadata. */
    localMeta?: SaveMeta | null;
    /** Cloud-copy metadata; detail card renders if either side has metadata. */
    cloudMeta?: SaveMeta | null;
    /** Surface this modal floats over — tweaks the scrim + the done CTA copy. */
    context?: 'desktop' | 'gamemode';
    /** Show the tertiary "Resolve in Ludusavi" fallback link on the choose step. */
    showLudusavi?: boolean;
    /** Perform the resolve. Resolve → auto-continue, throw → `error`. */
    resolve: (side: ConflictSide) => Promise<void>;
    /** User cancelled (choose step Cancel, or error step "Cancel launch"). */
    onCancel: () => void;
    /** Resolve succeeded — host closes the modal and continues the launch. */
    onContinue: () => void;
    /** Open the ludusavi GUI fallback. */
    onLudusavi: () => void;
    /** Dismiss via the chrome close button / Escape (choose + error only). */
    onClose?: () => void;
  } = $props();

  // ── State machine ─────────────────────────────────────────────────────────
  let phase = $state<ConflictPhase>('choose');
  let selected = $state<ConflictSide | null>(null);
  let hover = $state<Record<string, boolean>>({});

  const acc = $derived(accent ?? BRAND_SPOOL);
  const locked = $derived(phase !== 'choose');

  const sides = $derived<{ local: SideModel; cloud: SideModel }>({
    local: {
      key: 'local' as const,
      label: 'This device',
      sub: 'Keep local — push up to the cloud',
      confirm: "this device's",
      recent: !cloudNewer,
      meta: localMeta,
    },
    cloud: {
      key: 'cloud' as const,
      label: 'Cloud',
      sub: 'Keep cloud — pull down to this device',
      confirm: 'cloud',
      recent: cloudNewer,
      meta: cloudMeta,
    },
  });

  // Outcome tag for a given card. The winner reads "win"; the loser reflects
  // the live phase so its badge tracks OVERWRITING (applying) → UNCHANGED (error).
  function outcomeFor(key: ConflictSide): 'win' | ConflictPhase | null {
    if (!locked || !selected) return null;
    return key === selected ? 'win' : phase;
  }

  async function runResolve() {
    const side = selected;
    if (!side) return;
    phase = 'applying';
    try {
      await resolve(side);
      // Success — hand straight back to the host, which closes this modal and
      // continues the launch. No separate success screen.
      onContinue();
    } catch (e) {
      console.error('[cloud-conflict] resolve failed:', e);
      phase = 'error';
    }
  }

  function pick(side: ConflictSide) {
    if (locked) return;
    selected = side;
  }

  function confirm() {
    if (!selected || locked) return;
    void runResolve();
  }

  function cancel() {
    phase = 'choose';
    selected = null;
    onCancel();
  }

  // Unified close handler for the chrome X, Escape, and gamepad back.
  // During error, cancels the launch; during choose, dismisses.
  function handleClose() {
    if (phase === 'error') onCancel();
    else onClose?.();
  }
</script>

<!-- ── One choice card ─────────────────────────────────────────────────── -->
{#snippet choiceCard(side: SideModel)}
  {@const outcome = outcomeFor(side.key)}
  {@const active = selected === side.key}
  {@const winning = outcome === 'win'}
  {@const losing = outcome === 'applying' || outcome === 'error'}
  {@const h = hover[`card-${side.key}`] && !locked}
  {@const borderCol = active || winning
    ? acc
    : losing
      ? 'var(--color-line-1)'
      : h
        ? 'var(--color-line-3)'
        : 'var(--color-line-2)'}
  {@const bg = active || winning
    ? `${acc}12`
    : losing
      ? 'rgba(0,0,0,0.18)'
      : h
        ? 'var(--color-bg-2)'
        : 'var(--color-bg-1)'}
  {@const loseBadge = outcome === 'applying'
    ? 'OVERWRITING'
    : outcome === 'error'
      ? 'UNCHANGED'
      : ''}
  {@const loseBadgeCol = outcome === 'error' ? 'var(--color-ink-3)' : 'var(--color-warn)'}
  <button
    type="button"
    onclick={() => pick(side.key)}
    onmouseenter={() => (hover[`card-${side.key}`] = true)}
    onmouseleave={() => (hover[`card-${side.key}`] = false)}
    disabled={locked}
    data-gp-autofocus={side.recent ? '' : undefined}
    class="relative flex flex-col overflow-hidden rounded-md p-0 text-left transition-[background,border-color,opacity] duration-150"
    style:background={bg}
    style:border="1px solid {borderCol}"
    style:opacity={losing ? 0.62 : 1}
    style:cursor={locked ? 'default' : 'pointer'}
    style:box-shadow={active || winning ? `0 0 0 1px ${acc}66, 0 8px 26px ${acc}1f` : 'none'}
  >
    <!-- band -->
    <div
      class="flex items-center justify-between"
      style:padding="10px 12px"
      style:background={active || winning ? acc : 'var(--color-bg-2)'}
      style:border-bottom="1px solid {active || winning ? acc : 'var(--color-line-1)'}"
    >
      <div class="flex items-center" style:gap="9px">
        <span
          class="inline-flex items-center justify-center rounded-sm"
          style:width="24px"
          style:height="24px"
          style:background={active || winning ? 'rgba(0,0,0,0.22)' : 'var(--color-bg-3)'}
          style:color={active || winning ? 'rgba(255,255,255,0.95)' : 'var(--color-ink-1)'}
        >
          {#if side.key === 'local'}
            <HardDrive size={14} />
          {:else}
            <Cloud size={14} />
          {/if}
        </span>
        <span
          class="font-mono whitespace-nowrap font-semibold uppercase"
          style:font-size="11px"
          style:letter-spacing="0.14em"
          style:color={active || winning ? 'rgba(0,0,0,0.82)' : 'var(--color-ink-0)'}
        >
          {side.label}
        </span>
      </div>

      {#if winning}
        <span
          class="font-mono inline-flex items-center gap-1.5 whitespace-nowrap font-semibold"
          style:font-size="9px"
          style:letter-spacing="0.12em"
          style:color="rgba(0,0,0,0.78)"
        >
          <span class="flex" style:color="rgba(0,0,0,0.82)"><Check size={12} /></span>
          KEEPING
        </span>
      {:else if losing}
        <span
          class="font-mono inline-flex items-center gap-1.5 whitespace-nowrap font-semibold"
          style:font-size="9px"
          style:letter-spacing="0.12em"
          style:color={loseBadgeCol}
          style:border="1px solid color-mix(in srgb, {loseBadgeCol} 33%, transparent)"
          style:border-radius="3px"
          style:padding="2px 6px"
        >
          {loseBadge}
        </span>
      {:else if side.recent && side.meta && !locked && !active}
        <span
          class="font-mono inline-flex items-center gap-1.5 whitespace-nowrap"
          style:font-size="9px"
          style:letter-spacing="0.12em"
          style:color="var(--color-spool)"
          style:border="1px solid color-mix(in srgb, var(--color-spool) 33%, transparent)"
          style:border-radius="3px"
          style:padding="2px 6px"
        >
          <span class="rounded-full" style:width="5px" style:height="5px" style:background="var(--color-spool)"></span>
          MOST RECENT
        </span>
      {/if}
    </div>

    <!-- body -->
    <div class="flex flex-1 flex-col" style:padding="13px 14px 14px" style:gap="12px">
      <div style:font-size="12.5px" style:color="var(--color-ink-1)" style:line-height="1.35">
        {side.sub}
      </div>

      {#if side.meta}
        <div class="grid items-end" style:grid-template-columns="1fr auto" style:gap="12px">
          <div class="flex min-w-0 flex-col" style:gap="4px">
            <span
              class="font-mono whitespace-nowrap uppercase"
              style:font-size="9px"
              style:letter-spacing="0.14em"
              style:color="var(--color-ink-3)">Last modified</span
            >
            <span
              class="font-sans font-semibold"
              style:font-size="14px"
              style:color="var(--color-ink-0)"
              style:letter-spacing="-0.005em">{side.meta.abs}</span
            >
            <span
              class="font-mono"
              style:font-size="10px"
              style:letter-spacing="0.04em"
              style:color={side.recent ? 'var(--color-spool)' : 'var(--color-ink-3)'}
              >{side.meta.rel}</span
            >
          </div>
          <div class="flex flex-col text-right" style:gap="4px">
            <span
              class="font-mono whitespace-nowrap uppercase"
              style:font-size="9px"
              style:letter-spacing="0.14em"
              style:color="var(--color-ink-3)">Save size</span
            >
            <span
              class="font-sans font-semibold"
              style:font-size="14px"
              style:color="var(--color-ink-0)"
              style:letter-spacing="-0.005em">{side.meta.size}</span
            >
          </div>
        </div>
      {:else}
        <div
          class="flex items-center gap-2 rounded-sm"
          style:padding="9px 11px"
          style:border="1px dashed var(--color-line-2)"
          style:background="var(--color-bg-0)"
        >
          <span class="flex" style:color="var(--color-ink-3)"><Clock size={14} /></span>
          <span
            class="font-mono whitespace-nowrap"
            style:font-size="10px"
            style:letter-spacing="0.06em"
            style:color="var(--color-ink-3)">No save details</span
          >
        </div>
      {/if}
    </div>
  </button>
{/snippet}

<!-- ── Footer link ─────────────────────────────────────────────────────── -->
{#snippet linkBtn(key: string, label: string, onclick: () => void, color = 'var(--color-ink-3)')}
  <button
    type="button"
    {onclick}
    class="font-mono inline-flex cursor-pointer items-center gap-1.5 whitespace-nowrap border-none bg-transparent p-0 uppercase"
    style:font-size="10px"
    style:letter-spacing="0.08em"
    style:color={hover[key] ? 'var(--color-ink-1)' : color}
    onmouseenter={() => (hover[key] = true)}
    onmouseleave={() => (hover[key] = false)}
  >
    <span class="flex"><ExternalLink size={12} /></span>
    {label}
  </button>
{/snippet}

<!-- ── Footer buttons (ghost + primary), 34px tall to match the design ─── -->
{#snippet ghostBtn(key: string, label: string, onclick: () => void, disabled = false)}
  <button
    type="button"
    {onclick}
    {disabled}
    class="inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-sm font-medium transition-colors duration-100 disabled:pointer-events-none disabled:opacity-50"
    style:height="34px"
    style:padding-inline="12px"
    style:font-size="13px"
    style:cursor={disabled ? 'default' : 'pointer'}
    style:color="var(--color-ink-2)"
    style:border="1px solid var(--color-line-1)"
    style:background={hover[key] && !disabled ? 'rgb(255 255 255 / 0.06)' : 'transparent'}
    onmouseenter={() => (hover[key] = true)}
    onmouseleave={() => (hover[key] = false)}
  >
    {label}
  </button>
{/snippet}

<ModalShell
  breadcrumb="SYNC · CONFLICT"
  {accent}
  {context}
  width="640px"
  closeDisabled={phase === 'applying'}
  onClose={handleClose}
  ariaLabelledBy="cc-modal-title"
>
  <!-- hero -->
  <div
    class="flex items-start gap-[18px]"
    style:padding="20px 24px 18px"
    style:border-bottom="1px solid var(--color-line-1)"
  >
    <div class="min-w-0 flex-1">
      <h1
        id="cc-modal-title"
        class="font-display"
        style:margin="0"
        style:font-size="26px"
        style:font-weight="700"
        style:letter-spacing="-0.02em"
        style:line-height="1.05"
      >
        Cloud save conflict
      </h1>
      <div style:margin-top="6px" style:font-size="13.5px" style:color="var(--color-ink-1)" style:font-weight="500">
        {gameName}
      </div>
      <p
        style:margin="9px 0 0"
        style:font-size="13px"
        style:color="var(--color-ink-2)"
        style:line-height="1.5"
        style:max-width="440px"
      >
        Your saves here and in the cloud have both changed since the last sync. Keep one — the
        other will be overwritten.
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

  <!-- choice cards -->
  <div class="grid" style:padding="18px 24px 16px" style:grid-template-columns="1fr 1fr" style:gap="14px">
    {@render choiceCard(sides.local)}
    {@render choiceCard(sides.cloud)}
  </div>

  <!-- footer / status -->
  <div style:padding="16px 24px 22px" style:border-top="1px solid var(--color-line-1)" style:background="rgba(0,0,0,0.18)">
    {#if phase === 'error'}
      {@const local = selected === 'local'}
      <div class="flex flex-col" style:gap="14px">
        <div class="flex items-start gap-3">
          <span
            class="inline-flex shrink-0 items-center justify-center rounded-full"
            style:width="30px"
            style:height="30px"
            style:background="color-mix(in srgb, var(--color-bad) 11%, transparent)"
            style:color="var(--color-bad)"
          >
            <X size={15} />
          </span>
          <div class="min-w-0 flex-1">
            <div class="font-mono" style:font-size="10px" style:letter-spacing="0.16em" style:color="var(--color-bad)">
              {local ? 'UPLOAD FAILED' : 'DOWNLOAD FAILED'}
            </div>
            <div style:margin-top="3px" style:font-size="13px" style:color="var(--color-ink-1)" style:line-height="1.45">
              Couldn't {local ? 'reach your cloud remote' : 'download from your cloud remote'}.
              <strong class="font-semibold text-ink-0">Nothing was overwritten</strong> — your
              {local ? 'saves on this device' : 'local saves'} are untouched.
            </div>
          </div>
        </div>
        <div class="flex items-center" style:gap="10px">
          {@render linkBtn('err-ludusavi', 'Open Ludusavi instead', onLudusavi, 'var(--color-ink-2)')}
          <div class="flex-1"></div>
          {@render ghostBtn('err-cancel', 'Cancel launch', cancel)}
          <button
            type="button"
            onclick={() => void runResolve()}
            class="inline-flex cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap rounded-sm font-medium transition-colors duration-100"
            style:height="34px"
            style:min-width="110px"
            style:padding-inline="12px"
            style:font-size="13px"
            style:color="#0b0c0e"
            style:border="1px solid transparent"
            style:background={hover['err-retry'] ? shadeHex(acc, -10) : acc}
            onmouseenter={() => (hover['err-retry'] = true)}
            onmouseleave={() => (hover['err-retry'] = false)}
          >
            <RotateCw size={13} />
            Try again
          </button>
        </div>
      </div>
    {:else}
      <!-- choose / applying -->
      {@const sel = selected ? sides[selected] : null}
      {@const applying = phase === 'applying'}
      <div class="flex flex-col" style:gap="12px">
        <div
          class="flex items-center gap-2 rounded-sm"
          style:padding="9px 12px"
          style:border="1px solid color-mix(in srgb, var(--color-warn) 20%, transparent)"
          style:background="linear-gradient(90deg, color-mix(in srgb, var(--color-warn) 8%, transparent), color-mix(in srgb, var(--color-warn) 2%, transparent) 50%, transparent)"
        >
          <span class="shrink-0 rounded-full" style:width="6px" style:height="6px" style:background="var(--color-warn)"></span>
          <span class="flex-1" style:font-size="12px" style:color="var(--color-ink-1)">
            The other copy will be <strong class="font-semibold" style:color="var(--color-warn)">permanently replaced</strong>.
          </span>
        </div>
        <div class="flex items-center" style:gap="10px">
          {#if showLudusavi && !applying}
            {@render linkBtn('choose-ludusavi', 'Resolve in Ludusavi', onLudusavi)}
          {/if}
          <div class="flex-1"></div>
          {@render ghostBtn('choose-cancel', 'Cancel', cancel, applying)}
          <button
            type="button"
            onclick={confirm}
            disabled={!sel || applying}
            class="inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-sm font-medium transition-colors duration-100"
            style:height="34px"
            style:min-width="196px"
            style:padding-inline="12px"
            style:font-size="13px"
            style:cursor={applying ? 'default' : sel ? 'pointer' : 'not-allowed'}
            style:opacity={sel ? 1 : 0.7}
            style:color={sel ? '#0b0c0e' : 'var(--color-ink-3)'}
            style:border={sel ? '1px solid transparent' : '1px solid var(--color-line-2)'}
            style:background={sel
              ? hover['choose-confirm'] && !applying
                ? shadeHex(acc, -10)
                : acc
              : 'var(--color-bg-2)'}
            onmouseenter={() => (hover['choose-confirm'] = true)}
            onmouseleave={() => (hover['choose-confirm'] = false)}
          >
            {#if applying}
              <span class="cc-reel inline-flex"><LoaderCircle size={14} /></span>
              Keeping {sel?.confirm} saves…
            {:else}
              {sel ? `Keep ${sel.confirm} saves` : 'Select a copy to keep'}
            {/if}
          </button>
        </div>
      </div>
    {/if}
  </div>
</ModalShell>

<style>
  .cc-reel {
    animation: cc-spin 2.2s linear infinite;
    transform-origin: center;
  }
  @keyframes cc-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
