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
  export type ConflictPhase = 'choose' | 'applying' | 'done' | 'error';

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
   *   choose → applying → done | error
   *
   * Presentational + self-driven: the component owns the state machine and
   * card animations, but the actual upload/download work is delegated to the
   * `resolve` callback (kept out of here so the backend command can land
   * separately). On success it advances to `done`; on a thrown error it falls
   * to `error`, where the working "Open Ludusavi" fallback still lives.
   *
   * Mirrors the Cloud Save Conflict design (Space Grotesk / Geist / JetBrains
   * Mono, graphite surface, per-game accent, cassette reel + tape primitives).
   */
  import { Check, Clock, Cloud, ExternalLink, HardDrive, RotateCw, X } from '@lucide/svelte';
  import { shadeHex } from '$lib/tokens';
  import SpoolMark from '$lib/components/SpoolMark.svelte';
  import CatalogId from '$lib/components/CatalogId.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    gameName,
    catalogId = undefined,
    accent = null,
    coverUrl = null,
    cloudNewer = true,
    localMeta = null,
    cloudMeta = null,
    context = 'desktop',
    showLudusavi = true,
    progress = null,
    resolve,
    onCancel,
    onContinue,
    onLudusavi,
    onClose,
  }: {
    /** Display name of the game in conflict. */
    gameName: string;
    /** Pre-formatted catalog id ("SPL-0028"). Hidden when omitted. */
    catalogId?: string;
    /** Cover-art accent hex; falls back to the brand spool colour. */
    accent?: string | null;
    /** Webview-loadable cover URL (via `assetUrl`); placeholder when null. */
    coverUrl?: string | null;
    /** Which side is the most recent revision — drives the "MOST RECENT" tag. */
    cloudNewer?: boolean;
    /** Local-copy metadata; both metas present ⇒ the detailed card variant. */
    localMeta?: SaveMeta | null;
    /** Cloud-copy metadata; both metas present ⇒ the detailed card variant. */
    cloudMeta?: SaveMeta | null;
    /** Surface this modal floats over — tweaks the scrim + the done CTA copy. */
    context?: 'desktop' | 'gamemode';
    /** Show the tertiary "Resolve in Ludusavi" fallback link on the choose step. */
    showLudusavi?: boolean;
    /** 0–100 real progress override during `applying`; null ⇒ internal ramp. */
    progress?: number | null;
    /** Perform the resolve. Resolve → `done`, throw → `error`. */
    resolve: (side: ConflictSide) => Promise<void>;
    /** User cancelled (choose step Cancel, or error step "Cancel launch"). */
    onCancel: () => void;
    /** User acknowledged success ("Continue launch" / "Done"). */
    onContinue: () => void;
    /** Open the ludusavi GUI fallback. */
    onLudusavi: () => void;
    /** Dismiss via the chrome close button / Escape (choose + error only). */
    onClose?: () => void;
  } = $props();

  // ── State machine ─────────────────────────────────────────────────────────
  let phase = $state<ConflictPhase>('choose');
  let selected = $state<ConflictSide | null>(null);
  let internalPct = $state(0); // 0–1, used when no `progress` prop is supplied
  let hover = $state<Record<string, boolean>>({});

  const acc = $derived(accent ?? BRAND_SPOOL);
  const hasMeta = $derived(localMeta != null && cloudMeta != null);
  const locked = $derived(phase !== 'choose');

  const sides = $derived<{ local: SideModel; cloud: SideModel }>({
    local: {
      key: 'local' as const,
      label: 'This device',
      sub: 'Keep local — push up to the cloud',
      confirm: "this device’s",
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

  // Bar fill: real progress when provided, else the internal decelerating ramp.
  const barPct = $derived(
    progress != null ? Math.max(0, Math.min(1, progress / 100)) : internalPct,
  );

  // Outcome tag for a given card. The winner reads "win"; the loser reflects
  // the live phase so its badge tracks OVERWRITING → REPLACED / UNCHANGED.
  function outcomeFor(key: ConflictSide): 'win' | ConflictPhase | null {
    if (!locked || !selected) return null;
    return key === selected ? 'win' : phase;
  }

  // Internal progress ramp while applying (keyed on phase only, so prop
  // changes never re-trigger it — and never re-fire `resolve`).
  $effect(() => {
    if (phase !== 'applying') return;
    internalPct = 0;
    const start = performance.now();
    let raf = 0;
    let cancelled = false;
    const tick = (now: number) => {
      if (cancelled) return;
      const p = Math.min(1, (now - start) / 3000);
      // Ease out, capped at 95% — the resolve completion snaps it to 100%.
      internalPct = (1 - Math.pow(1 - p, 1.8)) * 0.95;
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelled = true;
      if (raf) cancelAnimationFrame(raf);
    };
  });

  async function runResolve() {
    const side = selected;
    if (!side) return;
    phase = 'applying';
    try {
      await resolve(side);
      internalPct = 1;
      phase = 'done';
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

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && phase !== 'applying') {
      if (phase === 'error') onCancel();
      else onClose?.();
    }
  }

  // Reel spinner spokes (port of the design's ReelHub, size 26).
  const REEL_SPOKES = (() => {
    const size = 26;
    const cx = size / 2;
    const cy = size / 2;
    const r1 = size * 0.16;
    const r2 = size * 0.36;
    return Array.from({ length: 6 }, (_, k) => {
      const a = (k * Math.PI) / 3;
      return {
        x1: cx + Math.cos(a) * r1,
        y1: cy + Math.sin(a) * r1,
        x2: cx + Math.cos(a) * r2,
        y2: cy + Math.sin(a) * r2,
      };
    });
  })();
</script>

<svelte:window onkeydown={handleKey} />

<!-- ── Reel spinner ────────────────────────────────────────────────────── -->
{#snippet reelHub(color: string)}
  <svg
    width="26"
    height="26"
    viewBox="0 0 26 26"
    class="cc-reel block shrink-0"
    style:color
  >
    <circle cx="13" cy="13" r="11.44" fill="transparent" stroke="currentColor" stroke-width="1.4" />
    {#each REEL_SPOKES as s (s.x1)}
      <line
        x1={s.x1}
        y1={s.y1}
        x2={s.x2}
        y2={s.y2}
        stroke="currentColor"
        stroke-width="1.4"
        stroke-linecap="round"
      />
    {/each}
    <circle cx="13" cy="13" r="10.4" fill="none" stroke="currentColor" stroke-width="0.98" opacity="0.4" />
    <circle cx="13" cy="13" r="3.38" fill="currentColor" />
  </svg>
{/snippet}

<!-- ── One choice card ─────────────────────────────────────────────────── -->
{#snippet choiceCard(side: SideModel)}
  {@const outcome = outcomeFor(side.key)}
  {@const active = selected === side.key}
  {@const winning = outcome === 'win'}
  {@const losing = outcome === 'applying' || outcome === 'done' || outcome === 'error'}
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
    : outcome === 'done'
      ? 'REPLACED'
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
      {:else if side.recent && !locked && !active}
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

      {#if hasMeta && side.meta}
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
{#snippet ghostBtn(key: string, label: string, onclick: () => void)}
  <button
    type="button"
    {onclick}
    class="inline-flex cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap rounded-sm font-medium transition-colors duration-100"
    style:height="34px"
    style:padding-inline="12px"
    style:font-size="13px"
    style:color="var(--color-ink-2)"
    style:border="1px solid var(--color-line-1)"
    style:background={hover[key] ? 'rgb(255 255 255 / 0.06)' : 'transparent'}
    onmouseenter={() => (hover[key] = true)}
    onmouseleave={() => (hover[key] = false)}
  >
    {label}
  </button>
{/snippet}

<div
  class="cc-scrim fixed inset-0 z-50 flex items-center justify-center"
  style:padding="24px"
  style:background={context === 'desktop' ? 'rgba(4,5,7,0.62)' : 'rgba(4,5,7,0.5)'}
  style:backdrop-filter="blur(2px)"
  style:-webkit-backdrop-filter="blur(2px)"
>
  <div
    class="cc-modal flex flex-col overflow-hidden text-ink-0"
    style:width="640px"
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
        style:color="var(--color-warn)">SYNC · CONFLICT</span
      >
      <div class="flex-1"></div>
      <button
        type="button"
        onclick={() => onClose?.()}
        aria-label="Close"
        class="inline-flex items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-bad/20 hover:text-[#ff9b9b]"
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
            class="font-mono whitespace-nowrap uppercase"
            style:font-size="9.5px"
            style:letter-spacing="0.12em"
            style:color="var(--color-warn)">BOTH COPIES CHANGED</span
          >
        </div>
        <h1
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
      {#if phase === 'applying'}
        {@const win = selected ? sides[selected] : null}
        {@const verb = selected === 'local' ? 'Uploading' : 'Downloading'}
        {@const prep = selected === 'local' ? 'to your cloud remote' : 'from your cloud remote'}
        {@const other = selected === 'local' ? 'cloud' : 'local'}
        <div class="flex flex-col" style:gap="12px">
          <div class="flex items-center gap-3">
            {@render reelHub(acc)}
            <div class="min-w-0 flex-1">
              <div class="font-mono" style:font-size="10px" style:letter-spacing="0.16em" style:color={acc}>
                {verb.toUpperCase()}
              </div>
              <div style:margin-top="3px" style:font-size="13px" style:color="var(--color-ink-1)">
                {#if win?.meta}
                  {verb} <strong class="font-semibold text-ink-0">{win.meta.size}</strong>
                  {prep} — the {other} copy is being replaced.
                {:else}
                  {verb} your saves {prep} — the {other} copy is being replaced.
                {/if}
              </div>
            </div>
          </div>
          <!-- tape bar -->
          <div class="w-full">
            <div
              class="relative overflow-hidden rounded-[1px] bg-bg-0"
              style:height="5px"
              style:box-shadow="inset 0 0 0 1px rgba(255,255,255,0.05)"
            >
              <div
                class="absolute left-0 top-0 bottom-0 rounded-[1px] transition-[width] duration-100 ease-linear"
                style:width="{Math.round(barPct * 100)}%"
                style:background={acc}
                style:box-shadow="0 0 10px {acc}77"
              ></div>
            </div>
            <div
              class="pointer-events-none"
              style:height="2px"
              style:margin-top="3px"
              style:background-image="repeating-linear-gradient(to right, rgba(255,255,255,0.10) 0 1px, transparent 1px 12.5%)"
            ></div>
          </div>
          <div class="font-mono" style:font-size="9.5px" style:letter-spacing="0.06em" style:color="var(--color-ink-3)">
            Keep Spool open — this can take a few seconds.
          </div>
        </div>
      {:else if phase === 'done'}
        <div class="flex items-center" style:gap="14px">
          <span
            class="inline-flex shrink-0 items-center justify-center rounded-full"
            style:width="30px"
            style:height="30px"
            style:background="color-mix(in srgb, var(--color-ok) 11%, transparent)"
            style:color="var(--color-ok)"
          >
            <Check size={15} />
          </span>
          <div class="min-w-0 flex-1">
            <div class="font-mono" style:font-size="10px" style:letter-spacing="0.16em" style:color="var(--color-ok)">
              SAVES IN SYNC
            </div>
            <div style:margin-top="3px" style:font-size="13px" style:color="var(--color-ink-1)">
              This device and the cloud now match.{context === 'gamemode' ? ' Continuing launch…' : ''}
            </div>
          </div>
          <button
            type="button"
            onclick={onContinue}
            class="inline-flex cursor-pointer items-center justify-center whitespace-nowrap rounded-sm font-medium transition-colors duration-100"
            style:height="34px"
            style:min-width="132px"
            style:padding-inline="12px"
            style:font-size="13px"
            style:color="#0b0c0e"
            style:border="1px solid transparent"
            style:background={hover['done-cta'] ? shadeHex('#7ee2a4', -10) : 'var(--color-ok)'}
            onmouseenter={() => (hover['done-cta'] = true)}
            onmouseleave={() => (hover['done-cta'] = false)}
          >
            {context === 'gamemode' ? 'Continue launch' : 'Done'}
          </button>
        </div>
      {:else if phase === 'error'}
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
                Couldn’t {local ? 'reach your cloud remote' : 'download from your cloud remote'}.
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
        <!-- choose -->
        {@const sel = selected ? sides[selected] : null}
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
            {#if showLudusavi}
              {@render linkBtn('choose-ludusavi', 'Resolve in Ludusavi', onLudusavi)}
            {/if}
            <div class="flex-1"></div>
            {@render ghostBtn('choose-cancel', 'Cancel', cancel)}
            <button
              type="button"
              onclick={confirm}
              disabled={!sel}
              class="inline-flex items-center justify-center whitespace-nowrap rounded-sm font-medium transition-colors duration-100"
              style:height="34px"
              style:min-width="196px"
              style:padding-inline="12px"
              style:font-size="13px"
              style:cursor={sel ? 'pointer' : 'not-allowed'}
              style:opacity={sel ? 1 : 0.7}
              style:color={sel ? '#0b0c0e' : 'var(--color-ink-3)'}
              style:border={sel ? '1px solid transparent' : '1px solid var(--color-line-2)'}
              style:background={sel
                ? hover['choose-confirm']
                  ? shadeHex(acc, -10)
                  : acc
                : 'var(--color-bg-2)'}
              onmouseenter={() => (hover['choose-confirm'] = true)}
              onmouseleave={() => (hover['choose-confirm'] = false)}
            >
              {sel ? `Keep ${sel.confirm} saves` : 'Select a copy to keep'}
            </button>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .cc-scrim {
    animation: cc-fade 160ms ease;
  }
  .cc-modal {
    animation: cc-pop 200ms ease;
  }
  .cc-reel {
    animation: cc-spin 2.2s linear infinite;
    transform-origin: center;
  }
  @keyframes cc-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes cc-pop {
    from {
      opacity: 0;
      transform: translateY(10px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  @keyframes cc-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
