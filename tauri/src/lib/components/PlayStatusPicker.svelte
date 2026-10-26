<script lang="ts">
  /**
   * Detail-header control for a game's play status: shows the current status
   * (or "Set status") and opens a `PlayStatusList` popover to change it. The
   * popover is fixed-positioned from the button's rect so the hero's
   * `overflow-hidden` can't clip it, and closes on scroll rather than drift
   * away from the button. It's its own gamepad scope, so B closes it and the
   * dpad stays inside it.
   */
  import { onMount } from 'svelte';
  import { ChevronDown, Tag } from '@lucide/svelte';
  import PlayStatusList from './PlayStatusList.svelte';
  import { gamepadScope } from '$lib/gamepad';
  import { playStatusOf, type PlayStatus } from '$lib/playStatus';

  let {
    game,
    onSelect,
  }: {
    game: { play_status: string | null };
    onSelect: (status: PlayStatus | null) => void;
  } = $props();

  const def = $derived(playStatusOf(game));

  // Popover width (`w-[200px]` below) and the gap kept from the window edge.
  const POPOVER_W = 200;
  const EDGE = 8;

  let open = $state(false);
  let pos = $state({ x: 0, y: 0 });
  let anchorEl: HTMLButtonElement | undefined = $state();
  let popoverEl: HTMLDivElement | undefined = $state();

  function toggle() {
    if (!open && anchorEl) {
      const r = anchorEl.getBoundingClientRect();
      // Clamp so the popover stays on screen when the button is near the
      // window's right edge.
      const x = Math.max(EDGE, Math.min(r.left, window.innerWidth - POPOVER_W - EDGE));
      pos = { x, y: r.bottom + 6 };
    }
    open = !open;
  }

  function handleOutside(e: MouseEvent) {
    if (!open) return;
    const t = e.target as Node;
    if (anchorEl?.contains(t) || popoverEl?.contains(t)) return;
    open = false;
  }
  function handleKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape') open = false;
  }
  function handleScroll(e: Event) {
    if (open && !popoverEl?.contains(e.target as Node)) open = false;
  }
  onMount(() => {
    document.addEventListener('mousedown', handleOutside, true);
    document.addEventListener('keydown', handleKey, true);
    document.addEventListener('scroll', handleScroll, true);
    return () => {
      document.removeEventListener('mousedown', handleOutside, true);
      document.removeEventListener('keydown', handleKey, true);
      document.removeEventListener('scroll', handleScroll, true);
    };
  });
</script>

<button
  bind:this={anchorEl}
  type="button"
  data-testid="play-status-picker"
  onclick={toggle}
  aria-expanded={open}
  class="inline-flex cursor-pointer items-center gap-1.5 rounded-full border py-0.5 pl-1.5 pr-2 text-[11.5px] font-medium backdrop-blur-sm transition-colors"
  style:color={def ? def.color : 'var(--color-ink-1)'}
  style:border-color={def ? `color-mix(in srgb, ${def.color} 40%, transparent)` : 'var(--color-line-2)'}
  style:background={def ? `color-mix(in srgb, ${def.color} 14%, rgb(0 0 0 / 0.35))` : 'rgb(0 0 0 / 0.35)'}
>
  {#if def}
    {@const Icon = def.icon}
    <Icon size={13} strokeWidth={2.25} />
    {def.label}
  {:else}
    <Tag size={12} />
    Set status
  {/if}
  <ChevronDown size={11} class="opacity-70" />
</button>

{#if open}
  <div
    bind:this={popoverEl}
    role="dialog"
    aria-label="Play status"
    use:gamepadScope={{ onBack: () => (open = false) }}
    class="fixed z-50 rounded-md border border-line-2 bg-bg-1 p-1.5"
    style:width="{POPOVER_W}px"
    style:left="{pos.x}px"
    style:top="{pos.y}px"
    style:box-shadow="0 16px 40px rgb(0 0 0 / 0.5)"
  >
    <PlayStatusList
      current={game.play_status}
      onSelect={(s) => {
        open = false;
        onSelect(s);
      }}
    />
  </div>
{/if}
