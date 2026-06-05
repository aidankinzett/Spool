<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { tauriDecorator } from '../../../.storybook/tauri-mock';
  import {
    makeConfig,
    makeDownload,
    SAMPLE_LIBRARY,
    SAMPLE_PEERS,
    SAMPLE_UPLOADS,
  } from '../../../.storybook/fixtures';
  import LibraryHarness from '../../../.storybook/LibraryHarness.svelte';

  // The main window in both layouts. The harness builds a real library
  // controller; `parameters.tauri` feeds it the game list, config, peers, and
  // sync status. `layout` picks LibraryDesktop vs LibraryTouch.
  const ONLINE = { reachability: 'online', server_version: null, error: null, last_ok_ago_secs: 3 };

  const { Story } = defineMeta({
    title: 'Screens/Library',
    component: LibraryHarness,
    tags: ['!autodocs'],
    parameters: {
      layout: 'fullscreen',
      tauri: { list_games: SAMPLE_LIBRARY, get_config: makeConfig(), current_sync_status: ONLINE },
    },
    decorators: [tauriDecorator()],
    argTypes: { layout: { control: 'inline-radio', options: ['desktop', 'touch'] } },
  });
</script>

<!-- Desktop: sidebar list + detail pane. -->
<Story name="Desktop" args={{ layout: 'desktop' }} />

<!-- Desktop with LAN peers visible. -->
<Story
  name="Desktop · with peers"
  args={{ layout: 'desktop' }}
  parameters={{ tauri: { list_games: SAMPLE_LIBRARY, get_config: makeConfig(), current_sync_status: ONLINE, list_lan_peers: SAMPLE_PEERS } }}
/>

<!-- Desktop with the transfers panel populated (1 download + 2 uploads). The
     screenshot script opens the pill to capture the panel in context; the
     uploads resolve real covers from the library via coverFor. -->
<Story
  name="Desktop · transfers"
  args={{ layout: 'desktop' }}
  parameters={{
    tauri: {
      list_games: SAMPLE_LIBRARY,
      get_config: makeConfig(),
      current_sync_status: ONLINE,
      list_active_uploads: SAMPLE_UPLOADS,
      current_peer_download: makeDownload(),
    },
  }}
/>

<!-- Empty library: the first-run / no-games state. -->
<Story
  name="Desktop · empty"
  args={{ layout: 'desktop' }}
  parameters={{ tauri: { list_games: [], get_config: makeConfig(), current_sync_status: ONLINE } }}
/>

<!-- Touch: big-target shelf layout for handhelds. -->
<Story name="Touch" args={{ layout: 'touch' }} />

<!-- Touch with LAN peers (the LAN tab has content). -->
<Story
  name="Touch · with peers"
  args={{ layout: 'touch' }}
  parameters={{ tauri: { list_games: SAMPLE_LIBRARY, get_config: makeConfig(), current_sync_status: ONLINE, list_lan_peers: SAMPLE_PEERS } }}
/>
