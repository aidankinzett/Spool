<script lang="ts">
  /**
   * Collapsible "Status" section in the library sidebar. One row per play
   * status (icon, label, game count). Selecting a row scopes the game list to
   * that status; selecting it again clears the scope. Combines with the
   * collection scope. Dropping a game row onto a status sets it.
   */
  import { ChevronRight } from '@lucide/svelte';
  import MonoLabel from './MonoLabel.svelte';
  import { PLAY_STATUSES } from '$lib/playStatus';
  import type { Library } from '$lib/library.svelte';

  let { lib }: { lib: Library } = $props();

  let open = $state(true);
  let dropId = $state<string | null>(null);
</script>

<div class="px-2 pb-1.5 pt-0.5">
  <div class="flex items-center gap-1.5 px-2 py-1 text-ink-2">
    <button
      type="button"
      onclick={() => (open = !open)}
      class="flex flex-1 cursor-pointer items-center gap-1.5 border-none bg-transparent p-0 text-inherit"
    >
      <span
        class="flex transition-transform duration-150"
        style:transform={open ? 'rotate(90deg)' : 'rotate(0deg)'}
      >
        <ChevronRight size={12} />
      </span>
      <MonoLabel size={9.5}>Status</MonoLabel>
    </button>
  </div>

  {#if open}
    <div class="mt-0.5 flex flex-col gap-px">
      {#each PLAY_STATUSES as s (s.id)}
        {@const active = lib.activeStatus === s.id}
        {@const Icon = s.icon}
        <button
          type="button"
          aria-pressed={active}
          onclick={() => (lib.activeStatus = active ? null : s.id)}
          ondragover={(e) => {
            if (!e.dataTransfer?.types.includes('text/game-id')) return;
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
            dropId = s.id;
          }}
          ondragleave={() => {
            if (dropId === s.id) dropId = null;
          }}
          ondrop={(e) => {
            e.preventDefault();
            dropId = null;
            const gid = e.dataTransfer?.getData('text/game-id');
            if (gid) lib.setPlayStatus(gid, s.id);
          }}
          class="relative flex w-full items-center gap-2.5 rounded-sm py-1.5 pl-3 pr-2 text-left transition-colors hover:bg-white/[0.025]"
          style:background={dropId === s.id
            ? `color-mix(in srgb, ${s.color} 14%, transparent)`
            : active
              ? 'var(--color-bg-3)'
              : 'transparent'}
        >
          {#if active}
            <span
              class="absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-full"
              style:background={s.color}
            ></span>
          {/if}
          <Icon size={13} style="color: {s.color}" />
          <span class="flex-1 text-[12.5px] text-ink-0">{s.label}</span>
          <span class="text-[11px] text-ink-3">{lib.statusCounts[s.id]}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>
