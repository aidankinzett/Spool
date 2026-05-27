<script lang="ts">
  import { assetUrl } from './api';
  import type { GameEntry } from './types';

  let { game }: { game: GameEntry } = $props();

  // Portrait cover at the size the WPF app used (180x265). The cover path on
  // disk is whatever SteamGridDB downloaded; `convertFileSrc` turns it into
  // an asset:// URL the webview can load.
  const cover = $derived(assetUrl(game.cover_image_path));
</script>

<div class="group flex w-[180px] flex-col gap-2">
  <div
    class="relative aspect-[180/265] overflow-hidden rounded-lg bg-neutral-800 shadow-md ring-1 ring-white/5 transition-transform duration-150 group-hover:scale-[1.02]"
  >
    {#if cover}
      <img
        src={cover}
        alt={game.game_name}
        class="h-full w-full object-cover"
        loading="lazy"
      />
    {:else}
      <div class="flex h-full w-full items-center justify-center p-3 text-center text-sm text-neutral-400">
        {game.game_name}
      </div>
    {/if}
  </div>

  <div class="truncate text-sm font-medium text-neutral-200" title={game.game_name}>
    {game.game_name}
  </div>
</div>
