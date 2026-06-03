<script lang="ts">
  /**
   * Numeric input — the number counterpart to TextField. Same hairline border
   * + oxide focus ring, sized from --control-h / --text-base so it grows in
   * touch/gamepad mode. Two-way bound via `value` (a number); `oncommit` fires
   * on blur/Enter for "persist on commit" callers. An optional `suffix` (e.g.
   * "Mbps") renders inside the field.
   */
  let {
    value = $bindable(0),
    min,
    max,
    step,
    placeholder,
    suffix,
    width = '5rem',
    oncommit,
  }: {
    value: number;
    min?: number;
    max?: number;
    step?: number;
    placeholder?: string;
    suffix?: string;
    /** Width of the input box (the field hugs its content otherwise). */
    width?: string;
    oncommit?: (value: number) => void;
  } = $props();

  let focused = $state(false);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && e.currentTarget instanceof HTMLInputElement) {
      e.currentTarget.blur();
    }
  }
</script>

<div
  class="inline-flex items-center gap-1.5 rounded-sm border bg-bg-2 text-[length:var(--text-base)] transition-colors"
  style:height="var(--control-h)"
  style:padding-inline="calc(var(--space-unit) * 2)"
  style:border-color={focused ? 'var(--color-spool)' : 'var(--color-line-2)'}
>
  <input
    type="number"
    bind:value
    {min}
    {max}
    {step}
    {placeholder}
    onfocus={() => (focused = true)}
    onblur={() => {
      focused = false;
      oncommit?.(value);
    }}
    onkeydown={handleKeydown}
    class="font-mono min-w-0 bg-transparent text-right text-ink-0 outline-none placeholder:text-ink-3"
    style:width
  />
  {#if suffix}
    <span class="font-mono shrink-0 text-ink-3" style:font-size="var(--text-sm)">{suffix}</span>
  {/if}
</div>
