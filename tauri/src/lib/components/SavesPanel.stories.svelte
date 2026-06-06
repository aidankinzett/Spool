<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { mockIPC } from '@tauri-apps/api/mocks';
  import SavesPanel from './SavesPanel.svelte';

  // The panel reaches into Tauri from its actions: it asks the backend where to
  // open the folder picker, turns a picked folder into a portable template, and
  // sets/clears the custom save. Mock those so exploring the story — Browse,
  // typing a template, "Use this location", "Stop tracking" — resolves with
  // plausible values instead of throwing.
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

  const { Story } = defineMeta({
    title: 'Detail/SavesPanel',
    component: SavesPanel,
    render: template,
    parameters: { layout: 'fullscreen' },
  });

  const BASE = {
    gameId: 'g1',
    catalogNumber: 12,
    savePaths: [],
    usesProton: true,
    prefixReady: true,
    customSave: null,
    onChange: () => {},
  } satisfies ComponentProps<typeof SavesPanel>;
</script>

<!-- Match the editor's dark surface + content column so the rows look right. -->
{#snippet template(args: ComponentProps<typeof SavesPanel>)}
  <div class="bg-bg-0 text-ink-0 min-h-screen px-5 py-4">
    <div style="max-width: 640px;">
      <SavesPanel {...args} />
    </div>
  </div>
{/snippet}

<!-- Non-manifest game, prefix already generated: folder picker + manual entry. -->
<Story name="Not tracked" args={BASE} />

<!-- Proton game never launched: the prefix (and save folder) don't exist yet,
     so a hint tells the user to play it once first. -->
<Story name="Not tracked · launch first" args={{ ...BASE, prefixReady: false }} />

<!-- A custom save folder is already set: shows the location + Stop tracking. -->
<Story
  name="Custom folder set"
  args={{ ...BASE, customSave: { files: ['<winLocalAppData>/MyGame'], registry: [] } }}
/>

<!-- A custom save with several locations (e.g. AppData + Saved Games). -->
<Story
  name="Custom · multiple paths"
  args={{
    ...BASE,
    customSave: {
      files: ['<winLocalAppData>/MyGame', '<home>/Saved Games/MyGame'],
      registry: [],
    },
  }}
/>

<!-- ludusavi already covers this game (Windows): status notes manifest tracking,
     and the picker is offered as an override. -->
<Story
  name="Manifest tracked"
  args={{ ...BASE, usesProton: false, savePaths: ['%LOCALAPPDATA%/MyGame'] }}
/>
