<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type GameEntry = {
    id: string;
    game_name: string;
    exe_path: string;
    cover_image_path: string | null;
  };

  let games = $state<GameEntry[]>([]);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      games = await invoke<GameEntry[]>("list_games");
    } catch (e) {
      error = String(e);
    }
  });
</script>

{#if error}
  <p style="color: red">Error: {error}</p>
{:else}
  <h1 class="text-3xl font-bold text-blue-500">Library ({games.length} games)</h1>
  <ul>
    {#each games as game (game.id)}
      <li>{game.game_name} <small>— {game.exe_path}</small></li>
    {/each}
  </ul>
{/if}