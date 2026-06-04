<script lang="ts">
  /**
   * Range slider — a styled native `<input type="range">` with a value
   * readout. Sized from the density tokens (`--slider-*`) so it grows in
   * touch/gamepad mode like Btn / TextField / Toggle.
   *
   * Input handling:
   *   - Keyboard: native (arrow keys adjust a focused range).
   *   - Gamepad: the nav engine (lib/gamepad) detects a focused range input and
   *     turns dpad left/right into a one-step adjust (with held-repeat), while
   *     up/down still navigates away. So no per-component wiring is needed.
   *
   * `oncommit` fires on `change` (release / each discrete step) for callers that
   * persist on settle rather than on every intermediate `input`.
   */
  let {
    value = $bindable(0),
    min = 0,
    max = 10,
    step = 1,
    disabled = false,
    oncommit,
    suffix = '',
  }: {
    value: number;
    min?: number;
    max?: number;
    step?: number;
    disabled?: boolean;
    oncommit?: (v: number) => void;
    suffix?: string;
  } = $props();

  // Filled portion of the track, for the gradient fill.
  const pct = $derived(max > min ? ((value - min) / (max - min)) * 100 : 0);
</script>

<div class="flex items-center gap-3">
  <input
    class="gp-slider"
    type="range"
    {min}
    {max}
    {step}
    {disabled}
    bind:value
    onchange={() => oncommit?.(value)}
    style:--pct="{pct}%"
  />
  <span
    class="font-mono tabular-nums text-center text-ink-1"
    style:min-width="2.5ch"
    style:font-size="var(--text-base)"
  >
    {value}{suffix}
  </span>
</div>

<style>
  .gp-slider {
    -webkit-appearance: none;
    appearance: none;
    flex: 1;
    height: var(--slider-track-h);
    border-radius: 9999px;
    /* Filled (spool) up to --pct, track colour after. */
    background: linear-gradient(
      to right,
      var(--color-spool) 0%,
      var(--color-spool) var(--pct),
      var(--color-bg-3) var(--pct),
      var(--color-bg-3) 100%
    );
    cursor: pointer;
  }
  .gp-slider:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* WebKit/Chromium thumb (WebView2 + WebKitGTK). */
  .gp-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: var(--slider-thumb);
    height: var(--slider-thumb);
    border-radius: 9999px;
    background: var(--color-ink-0);
    border: 2px solid var(--color-bg-0);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.4);
    cursor: pointer;
  }
  .gp-slider::-moz-range-thumb {
    width: var(--slider-thumb);
    height: var(--slider-thumb);
    border-radius: 9999px;
    background: var(--color-ink-0);
    border: 2px solid var(--color-bg-0);
    cursor: pointer;
  }
</style>
