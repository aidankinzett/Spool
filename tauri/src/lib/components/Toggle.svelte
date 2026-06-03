<script lang="ts">
  /**
   * On/off toggle — pill track + sliding thumb. Uses the spool oxide
   * accent for the on-state track. Two-way bound via `checked`.
   */
  let {
    checked = $bindable(false),
    disabled = false,
    onchange,
    'aria-label': ariaLabel,
  }: {
    checked: boolean;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
    'aria-label'?: string;
  } = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<!-- Track + thumb size from the density tokens (var(--toggle-*)), so the
     toggle grows in touch/gamepad mode alongside Btn / TextField. -->
<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel}
  {disabled}
  onclick={toggle}
  class="relative inline-flex shrink-0 cursor-pointer items-center rounded-full transition-colors disabled:cursor-not-allowed disabled:opacity-50"
  style:width="var(--toggle-w)"
  style:height="var(--toggle-h)"
  style:background={checked ? 'var(--color-spool)' : 'rgb(255 255 255 / 0.10)'}
>
  <span
    class="inline-block rounded-full bg-bg-0 transition-transform"
    style:width="var(--toggle-thumb)"
    style:height="var(--toggle-thumb)"
    style:transform={checked
      ? 'translateX(calc(var(--toggle-w) - var(--toggle-thumb) - var(--toggle-pad)))'
      : 'translateX(var(--toggle-pad))'}
  ></span>
</button>
