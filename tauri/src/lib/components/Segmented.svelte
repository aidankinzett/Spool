<script lang="ts">
  // Generic segmented control. Mode-agnostic: sized by density tokens, so
  // it grows to touch targets at [data-mode='touch'] without reading uiMode.
  type Option = { value: string; label: string };
  let {
    options,
    value,
    onchange,
  }: { options: Option[]; value: string; onchange: (value: string) => void } = $props();
</script>

<div
  class="inline-flex gap-1 rounded-sm border border-line-2 bg-white/5"
  style:padding="calc(var(--space-unit) * 1)"
>
  {#each options as o (o.value)}
    {@const active = o.value === value}
    <button
      type="button"
      onclick={() => onchange(o.value)}
      class="cursor-pointer whitespace-nowrap rounded-sm border-none text-[length:var(--text-base)] transition-colors"
      style:height="var(--control-h)"
      style:padding-inline="calc(var(--space-unit) * 3)"
      style:font-weight={active ? 600 : 500}
      style:background={active ? 'var(--color-spool)' : 'transparent'}
      style:color={active ? '#0b0c0e' : 'var(--color-ink-1)'}
    >
      {o.label}
    </button>
  {/each}
</div>
