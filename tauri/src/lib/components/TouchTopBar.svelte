<script lang="ts">
  // Touch chrome strip — sized by density tokens, no mode awareness.
  // Replaces WindowChrome on [data-mode='touch']; rendered via AppChrome.
  // Port of the prototype's TopBar (touch_kit.jsx:74).
  import { onMount } from 'svelte';
  import { ChevronLeft, Wifi } from '@lucide/svelte';
  import SpoolMark from './SpoolMark.svelte';
  import MonoLabel from './MonoLabel.svelte';

  let {
    sub,
    accent,
    onback,
    peers = 0,
    transfers = 0,
    conflict = false,
    children,
  }: {
    /** Sub-section label shown after SPOOL/, e.g. "SETTINGS". */
    sub?: string;
    /** Tape-strip colour on the Spool mark. */
    accent?: string;
    /** If provided, a back button is shown that calls this on click. */
    onback?: () => void;
    /** Number of visible LAN peers. */
    peers?: number;
    /** Number of active transfers (for the badge). */
    transfers?: number;
    /** True when sync server is configured but unreachable (amber alert). */
    conflict?: boolean;
    /** Optional center-slot content (search, catalog id, etc.). */
    children?: import('svelte').Snippet;
  } = $props();

  const alert = $derived(conflict || transfers > 0);

  let clock = $state('');
  let batteryPct = $state<number | null>(null);

  function formatClock(d: Date): string {
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
  }

  onMount(() => {
    clock = formatClock(new Date());
    const timer = setInterval(() => { clock = formatClock(new Date()); }, 10_000);

    // Battery API — Chromium/WebView2 only; degrades silently elsewhere.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let batteryObj: any = null;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const onLevelChange = () => { batteryPct = Math.round(batteryObj.level * 100); };
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (navigator as any).getBattery?.()
      .then((b: any) => {
        batteryObj = b;
        batteryPct = Math.round(batteryObj.level * 100);
        batteryObj.addEventListener('levelchange', onLevelChange);
      })
      .catch(() => { /* not available on this platform */ });

    return () => {
      clearInterval(timer);
      batteryObj?.removeEventListener('levelchange', onLevelChange);
    };
  });
</script>

<div
  class="flex shrink-0 items-center gap-3 border-b border-line-1 bg-black/40"
  style:height="var(--chrome-h)"
  style:padding-inline="calc(var(--space-unit) * 4)"
>
  {#if onback}
    <button
      type="button"
      onclick={onback}
      class="inline-flex cursor-pointer items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:text-ink-0"
      style:height="var(--control-h-icon)"
      style:width="var(--control-h-icon)"
      aria-label="Back"
    >
      <ChevronLeft size={20} />
    </button>
  {/if}

  <SpoolMark size={22} color="var(--color-ink-1)" tape={accent ?? 'var(--color-spool)'} />
  <MonoLabel size={10.5}>SPOOL</MonoLabel>
  {#if sub}
    <span class="text-[10px] text-ink-3">/</span>
    <MonoLabel size={10.5} class="text-ink-1">{sub}</MonoLabel>
  {/if}

  <!-- Center slot (search, catalog id, etc.) -->
  <div class="flex-1">
    {#if children}{@render children()}{/if}
  </div>

  <!-- Sync + peers pill -->
  <div
    class={`inline-flex items-center gap-2 rounded-full border ${alert ? 'border-warn/40 bg-warn/10' : 'border-line-2 bg-bg-2'}`}
    style:padding-inline="calc(var(--space-unit) * 3)"
    style:height="calc(var(--control-h) * 0.7)"
  >
    <!-- Status dot -->
    <span
      class="rounded-full"
      style:width="7px"
      style:height="7px"
      style:background={conflict ? 'var(--color-warn)' : peers > 0 ? 'var(--color-ok)' : 'var(--color-ink-3)'}
    ></span>
    <Wifi size={13} class={conflict ? 'text-warn' : 'text-ink-2'} />
    <MonoLabel size={10}>{peers}</MonoLabel>
    {#if transfers > 0}
      <span
        class="inline-flex items-center justify-center rounded-full font-mono text-[10px] font-bold"
        style:min-width="16px"
        style:height="16px"
        style:padding="0 4px"
        style:background="var(--color-spool)"
        style:color="#0b0c0e"
      >{transfers}</span>
    {/if}
  </div>

  <!-- Battery (shown only when API is available) -->
  {#if batteryPct !== null}
    <MonoLabel size={10}>{batteryPct}%</MonoLabel>
  {/if}

  <!-- Clock -->
  {#if clock}
    <MonoLabel size={10}>{clock}</MonoLabel>
  {/if}
</div>
