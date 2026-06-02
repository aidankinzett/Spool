<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    helper,
    icon,
    right,
    children,
  }: {
    title: string;
    helper?: string;
    icon?: Snippet;
    right?: Snippet;
    children: Snippet;
  } = $props();
</script>

<div class="overflow-hidden rounded-[5px] border border-line-1 bg-bg-1">
  <div class="border-b border-dashed border-line-1 bg-bg-2 px-[18px] py-[13px] flex items-start gap-[11px]">
    {#if icon}
      <span class="flex mt-[1px] shrink-0 text-ink-2 w-[26px] h-[26px] rounded-[5px] bg-white/[0.04] items-center justify-center">
        {@render icon()}
      </span>
    {/if}
    <div class="flex-1 min-w-0">
      <div class="text-[14px] font-semibold text-ink-0">{title}</div>
      {#if helper}
        <div class="mt-[3px] max-w-[560px] text-[11.5px] leading-[1.5] text-ink-2">{helper}</div>
      {/if}
    </div>
    {#if right}
      <div class="shrink-0">{@render right()}</div>
    {/if}
  </div>
  <div class="py-[6px]">
    {@render children()}
  </div>
</div>
