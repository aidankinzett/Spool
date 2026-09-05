<script lang="ts">
  /**
   * Test-only parent for {@link LibraryContextMenu}, mirroring how
   * `LibraryDesktop` mounts it: the menu lives inside an `{#if}` whose state
   * object also *supplies the `game` prop*, and `onclose` clears that state.
   * The coupling is the point — a handler that calls `onclose()` before reading
   * `game` reads it after its source is gone (#488).
   */
  import LibraryContextMenu from './LibraryContextMenu.svelte';
  import type { GameEntry } from '$lib/types';

  let { game }: { game: GameEntry } = $props();
  // Seeded once, exactly as a right-click seeds it in LibraryDesktop.
  // svelte-ignore state_referenced_locally
  let ctxMenu = $state<{ game: GameEntry; x: number; y: number } | null>({ game, x: 10, y: 10 });
</script>

{#if ctxMenu}
  <LibraryContextMenu
    game={ctxMenu.game}
    x={ctxMenu.x}
    y={ctxMenu.y}
    onclose={() => (ctxMenu = null)}
  />
{/if}
