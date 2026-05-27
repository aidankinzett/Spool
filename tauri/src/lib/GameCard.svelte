<script lang="ts">
  /**
   * Game card — portrait tile in the library grid.
   *
   * Falls back to a synthetic "cassette sleeve" cover when no SteamGridDB
   * art is available: a duotone gradient + tape-strip header + the game
   * name set in the display face. Mirrors the design system `Cover`
   * component for parity with detail views.
   */
  import { assetUrl } from './api';
  import CatalogId from './components/CatalogId.svelte';
  import type { GameEntry } from './types';

  let { game }: { game: GameEntry } = $props();

  const cover = $derived(assetUrl(game.cover_image_path));

  // Catalog number is assigned by the backend (Library::backfill on load
  // for legacy entries, next_catalog_number() at add-time for new ones).
  const catalog = $derived(`SPL-${game.catalog_number.toString().padStart(4, '0')}`);
</script>

<div class="group flex w-[180px] flex-col gap-2">
  <div
    class="relative aspect-[180/265] overflow-hidden rounded-md bg-bg-2 shadow-[0_8px_24px_rgb(0_0_0_/_0.45)] ring-1 ring-line-1 transition-transform duration-150 group-hover:scale-[1.02] group-hover:ring-line-2"
  >
    {#if cover}
      <img src={cover} alt={game.game_name} class="h-full w-full object-cover" loading="lazy" />
    {:else}
      <!-- Synthetic sleeve cover — brand-default gradient + tape strip + title. -->
      <div
        class="relative h-full w-full"
        style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
      >
        <!-- tape sleeve label across the top -->
        <div
          class="absolute inset-x-0 top-0 h-[14px]"
          style:background="linear-gradient(to bottom, var(--color-spool), color-mix(in srgb, var(--color-spool) 80%, transparent))"
        ></div>
        <!-- side label -->
        <div
          class="absolute left-2.5 top-[18px] font-mono text-[8.5px] uppercase leading-none tracking-[0.16em] text-ink-2"
        >
          Side A
        </div>
        <!-- title -->
        <div class="absolute inset-x-2.5 bottom-2.5">
          <div
            class="font-display text-[15px] font-semibold leading-[1.08] text-ink-0 text-balance"
            style:text-shadow="0 1px 8px rgb(0 0 0 / 0.5)"
          >
            {game.game_name}
          </div>
        </div>
      </div>
    {/if}
  </div>

  <div class="flex items-center justify-between gap-2">
    <div class="truncate text-[12.5px] font-medium text-ink-0" title={game.game_name}>
      {game.game_name}
    </div>
    <CatalogId id={catalog} />
  </div>
</div>
