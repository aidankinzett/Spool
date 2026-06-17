<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { fn } from 'storybook/test';
  import { tauriDecorator } from '../../../.storybook/tauri-mock';
  import { makeGame, SAMPLE_COLLECTIONS } from '../../../.storybook/fixtures';
  import LibraryContextMenu from './LibraryContextMenu.svelte';

  // Right-click menu for a library entry. Positioned at (x, y); its actions call
  // backend commands, so it renders behind the Tauri mock.
  const { Story } = defineMeta({
    title: 'Library/LibraryContextMenu',
    component: LibraryContextMenu,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    args: {
      game: makeGame(),
      x: 80,
      y: 80,
      collections: SAMPLE_COLLECTIONS,
      onToggleCollection: fn(),
      onCreateCollection: fn(() => 'col-new'),
      onclose: fn(),
    },
    decorators: [tauriDecorator()],
    render: template,
  });
</script>

{#snippet template(args: ComponentProps<typeof LibraryContextMenu>)}
  <LibraryContextMenu {...args} />
{/snippet}

<Story name="Shared game" args={{ game: makeGame({ lan_shared: true }) }} />
<Story name="Not shared" args={{ game: makeGame({ lan_shared: false }) }} />
