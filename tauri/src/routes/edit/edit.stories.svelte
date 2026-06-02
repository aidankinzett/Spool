<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { tauriDecorator } from '../../../.storybook/tauri-mock';
  import { makeGame } from '../../../.storybook/fixtures';
  import type { GameEntry } from '$lib/types';
  import Edit from './+page.svelte';

  // The Edit page reads `?id=` from the URL, then loads that entry via
  // `list_games`. Storybook's own iframe URL uses `id` for the story id, so we
  // hand back a game whose id matches whatever the URL carries — the lookup
  // always resolves regardless of which story is selected.
  function gameForUrl(over: Partial<GameEntry> = {}) {
    return () => {
      const id = new URLSearchParams(window.location.search).get('id') ?? 'g1';
      return [makeGame({ id, ...over })];
    };
  }

  const { Story } = defineMeta({
    title: 'Screens/Edit Game',
    component: Edit,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    decorators: [tauriDecorator()],
  });
</script>

<!-- Windows entry: Identity / Install / Launch / Sharing tabs. -->
<Story name="Default (Windows)" parameters={{ tauri: { app_platform: 'windows', list_games: gameForUrl() } }} />

<!-- Windows already forces elevation for this exe — the Launch tab shows the
     informational "Registry" badge next to Run as administrator. -->
<Story
  name="Run-as-admin from registry"
  parameters={{ tauri: { app_platform: 'windows', list_games: gameForUrl(), get_run_as_admin_in_registry: true } }}
/>

<!-- Linux: the Launch tab surfaces Proton version, Wine prefix, and deps. -->
<Story
  name="Linux (Proton)"
  parameters={{
    tauri: {
      app_platform: 'linux',
      list_games: gameForUrl(),
      list_proton_versions: [
        { name: 'GE-Proton9-20', path: '/home/deck/.steam/compatibilitytools.d/GE-Proton9-20', source: 'ge' },
        { name: 'Proton 9.0', path: '/home/deck/.steam/steamapps/common/Proton 9.0', source: 'steam' },
      ],
    },
  }}
/>

<!-- No install folder: Sharing is gated and Delete-from-disk is disabled. -->
<Story
  name="No install folder"
  parameters={{ tauri: { app_platform: 'windows', list_games: gameForUrl({ game_folder_path: null, lan_shared: false }) } }}
/>
