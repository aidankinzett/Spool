<script lang="ts">
  /**
   * Pick-one list of play statuses plus "No status", with a check on the
   * current one. Shared by the library right-click menu's "Status" row and the
   * detail header's status popover.
   */
  import { Check } from '@lucide/svelte';
  import { PLAY_STATUSES, type PlayStatus } from '$lib/playStatus';

  let {
    current,
    onSelect,
  }: {
    current: string | null;
    onSelect: (status: PlayStatus | null) => void;
  } = $props();
</script>

<div class="flex flex-col gap-px">
  {#each PLAY_STATUSES as s (s.id)}
    {@const on = current === s.id}
    {@const Icon = s.icon}
    <button
      type="button"
      onclick={() => onSelect(on ? null : s.id)}
      class="flex w-full items-center gap-2.5 rounded-sm px-2.5 py-1.5 text-left text-[12.5px] text-ink-0 transition-colors hover:bg-white/5"
    >
      <Icon size={14} style="color: {s.color}" />
      <span class="min-w-0 flex-1 truncate">{s.label}</span>
      {#if on}<Check size={13} strokeWidth={3} style="color: {s.color}" />{/if}
    </button>
  {/each}
  {#if current}
    <button
      type="button"
      onclick={() => onSelect(null)}
      class="mt-1 flex w-full items-center gap-2.5 rounded-sm border-t border-line-1 px-2.5 pb-1.5 pt-2 text-left text-[12.5px] text-ink-2 transition-colors hover:bg-white/5"
    >
      Clear status
    </button>
  {/if}
</div>
