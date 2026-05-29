<script lang="ts">
  /**
   * One row inside a SettingsCard — 2-column grid layout.
   *
   *   ┌────────────────────────────────────────────────────────────────────┐
   *   │ Label                     │  control / extras                      │
   *   │ helper text               │                                        │
   *   └────────────────────────────────────────────────────────────────────┘
   *
   * `status` renders a small colored dot beside the label:
   *   "ok" → green, "warn" → amber, omitted → no dot.
   */
  type Status = 'ok' | 'warn' | 'info';

  let {
    label,
    helper,
    status,
    control,
    extras,
  }: {
    label: string;
    helper?: string;
    status?: Status;
    control?: import('svelte').Snippet;
    extras?: import('svelte').Snippet;
  } = $props();
</script>

<div class="grid items-start gap-[18px] px-[18px]" style:grid-template-columns="180px 1fr" style:padding-block="calc(var(--space-unit) * 3)">
  <div class="pt-[6px]">
    <div class="flex items-center gap-1.5 text-[length:var(--text-base)] font-medium text-ink-0">
      {label}
      {#if status}
        <span
          class="size-[5px] rounded-full"
          style:background={status === 'ok'
            ? 'var(--color-ok)'
            : status === 'warn'
              ? 'var(--color-warn)'
              : 'var(--color-info)'}
        ></span>
      {/if}
    </div>
    {#if helper}
      <div class="mt-[3px] text-[11px] leading-[1.5] text-ink-2">{helper}</div>
    {/if}
  </div>
  <div>
    {#if control}{@render control()}{/if}
    {#if extras}
      <div class="{control ? 'mt-2' : ''} flex items-center gap-2 flex-wrap">{@render extras()}</div>
    {/if}
  </div>
</div>
