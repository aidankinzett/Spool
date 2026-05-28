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

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel}
  {disabled}
  onclick={toggle}
  class="relative inline-flex h-[18px] w-8 shrink-0 cursor-pointer items-center rounded-full transition-colors disabled:cursor-not-allowed disabled:opacity-50"
  style:background={checked ? 'var(--color-spool)' : 'rgb(255 255 255 / 0.10)'}
>
  <span
    class="inline-block size-3.5 rounded-full bg-bg-0 transition-transform"
    style:transform={checked ? 'translateX(15px)' : 'translateX(2px)'}
  ></span>
</button>
