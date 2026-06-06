<script lang="ts">
  /**
   * Story harness for SavesPanel. The panel reports changes via `onChange`
   * rather than two-way binding, so it needs a stateful parent to actually grow
   * and shrink the list in Storybook. This holds `customSave` and feeds it back,
   * so Add / Browse / Remove / Stop tracking are fully interactive — the Tauri
   * mock installed by the story resolves the backend calls.
   */
  import SavesPanel from '$lib/components/SavesPanel.svelte';
  import type { CustomSave } from '$lib/types';

  let {
    gameId = 'g1',
    catalogNumber = 12,
    savePaths = [],
    usesProton = true,
    prefixReady = true,
    customSave: initial = null,
  }: {
    gameId?: string;
    catalogNumber?: number;
    savePaths?: string[];
    usesProton?: boolean;
    prefixReady?: boolean;
    customSave?: CustomSave | null;
  } = $props();

  // Seed once from the story arg; the panel drives it from here on.
  // svelte-ignore state_referenced_locally
  let customSave = $state<CustomSave | null>(initial);
</script>

<SavesPanel
  {gameId}
  {catalogNumber}
  {savePaths}
  {usesProton}
  {prefixReady}
  {customSave}
  onChange={(cs) => (customSave = cs)}
/>
