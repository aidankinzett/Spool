<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { mockIPC } from '@tauri-apps/api/mocks';
  import SavesPanelHarness from '../../../.storybook/SavesPanelHarness.svelte';

  // The panel reaches into Tauri from its actions: it asks the backend where to
  // open the folder picker, turns a picked folder into a portable template, and
  // sets/clears the custom save. Mock those so the story is fully interactive —
  // Browse (then Add) appends a location, the trash icons remove them, and
  // "Stop tracking" clears the list.
  mockIPC((cmd) => {
    switch (cmd) {
      case 'save_picker_start_dir':
        return '/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/steamuser';
      case 'plugin:dialog|open':
        // What the folder picker "returns" when you click Browse.
        return '/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/steamuser/AppData/Local/MyGame';
      case 'derive_save_template':
        // The portable template the backend derives from that folder.
        return '<winLocalAppData>/MyGame';
      case 'set_custom_save':
      case 'clear_custom_save':
        return undefined;
      default:
        return undefined;
    }
  });

  // Stories render the stateful harness (not SavesPanel directly) so Add/Remove
  // actually mutate the list; args are the harness's props.
  const { Story } = defineMeta({
    title: 'Detail/SavesPanel',
    component: SavesPanelHarness,
    render: template,
    parameters: { layout: 'fullscreen' },
  });
</script>

<!-- Match the editor's dark surface + content column so the rows look right. -->
{#snippet template(args: ComponentProps<typeof SavesPanelHarness>)}
  <div class="bg-bg-0 text-ink-0 min-h-screen px-5 py-4">
    <div style="max-width: 640px;">
      <SavesPanelHarness {...args} />
    </div>
  </div>
{/snippet}

<!-- Non-manifest game, prefix already generated. Browse → Add a location, type
     another template and Add it, remove with the trash icons. -->
<Story name="Not tracked" args={{ savePaths: [], usesProton: true, prefixReady: true, customSave: null }} />

<!-- Proton game never launched: the prefix (and save folder) don't exist yet,
     so a hint tells the user to play it once first. -->
<Story name="Not tracked · launch first" args={{ usesProton: true, prefixReady: false, customSave: null }} />

<!-- One custom location set: the list shows it with a remove (trash) button. -->
<Story
  name="One custom path"
  args={{ customSave: { files: ['<winLocalAppData>/MyGame'], registry: [] } }}
/>

<!-- A game that saves in several places — the list with per-item delete + Add. -->
<Story
  name="Multiple custom paths"
  args={{
    customSave: {
      files: ['<winLocalAppData>/MyGame', '<home>/Saved Games/MyGame', '<winDocuments>/My Games/MyGame'],
      registry: [],
    },
  }}
/>

<!-- ludusavi already covers this game (Windows): status notes manifest
     tracking, and locations can still be added as an override. -->
<Story
  name="Manifest tracked"
  args={{ usesProton: false, savePaths: ['%LOCALAPPDATA%/MyGame'], customSave: null }}
/>
