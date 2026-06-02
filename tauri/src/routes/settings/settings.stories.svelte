<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { tauriDecorator } from '../../../.storybook/tauri-mock';
  import { makeConfig, SAMPLE_DEPS } from '../../../.storybook/fixtures';
  import Settings from './+page.svelte';

  // The settings page loads everything from the backend on mount (config, LAN
  // peers, sync status, and — on Linux — Proton builds, the dependency doctor,
  // and the Decky plugin status). Each story below feeds a different backend
  // snapshot through `parameters.tauri`; see `.storybook/tauri-mock.ts`.
  const { Story } = defineMeta({
    title: 'Screens/Settings',
    component: Settings,
    // Full-window page; render it on the whole canvas and skip the stacked
    // Docs page (mockIPC is global, so only one screen may mount at a time).
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    decorators: [tauriDecorator()],
  });
</script>

<!-- Windows desktop, nothing wired up yet: cloud off, SteamGridDB key blank. -->
<Story name="Default (Windows)" parameters={{ tauri: { app_platform: 'windows', get_config: makeConfig() } }} />

<!-- Cloud saves + SteamGridDB configured. -->
<Story
  name="Cloud configured"
  parameters={{
    tauri: {
      app_platform: 'windows',
      get_config: makeConfig({
        cloud_provider: 'dropbox',
        cloud_remote: 'dropbox',
        steamgriddb_api_key: 'sgdb_live_key_xxxxx',
      }),
      check_cloud_remote_exists: true,
      current_sync_status: {
        reachability: 'online',
        server_version: null,
        error: null,
        last_ok_ago_secs: 4,
      },
    },
  }}
/>

<!-- Linux/Steam Deck: surfaces Proton, the dependency doctor, and Decky. -->
<Story
  name="Linux (Proton + Decky)"
  parameters={{
    tauri: {
      app_platform: 'linux',
      get_config: makeConfig({ device_name: 'Steam Deck' }),
      list_proton_versions: [
        { name: 'GE-Proton9-20', path: '/home/deck/.steam/compatibilitytools.d/GE-Proton9-20', source: 'ge' },
        { name: 'Proton 9.0', path: '/home/deck/.steam/steamapps/common/Proton 9.0', source: 'steam' },
      ],
      check_dependencies: SAMPLE_DEPS,
      decky_plugin_status: {
        supported: true,
        installed: true,
        installed_version: '1.2.0',
        bundled_version: '1.2.0',
        decky_present: true,
      },
    },
  }}
/>
