<script lang="ts">
  let {
    title,
    helper,
    children,
  }: {
    title: string;
    helper?: string;
    children: import('svelte').Snippet;
  } = $props();
</script>

<div class="overflow-hidden rounded-[5px] border border-line-1 bg-bg-1">
  <div class="border-b border-dashed border-line-1 bg-bg-2 px-[18px] py-[14px]">
    <div class="text-[14px] font-semibold text-ink-0">{title}</div>
    {#if helper}
      <div class="mt-[3px] max-w-[540px] text-[11.5px] leading-[1.5] text-ink-2">{helper}</div>
    {/if}
  </div>
  <div class="py-2">
    {@render children()}
  </div>
</div>
