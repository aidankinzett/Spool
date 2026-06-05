<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { fn } from 'storybook/test';
  import RemoveGameModal from './RemoveGameModal.svelte';

  // The remove/delete chooser: forget the entry, or delete the install folder
  // from disk. The disk option is disabled when there's no folder on record.
  const { Story } = defineMeta({
    title: 'Modals/RemoveGameModal',
    component: RemoveGameModal,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    args: {
      gameName: 'The Witcher 3: Wild Hunt',
      catalogId: 'SPL-0001',
      accent: '#c9a36f',
      coverUrl: null,
      folderPath: 'C:/Games/The Witcher 3',
      perform: fn(async () => {}),
      onClose: fn(),
    },
    render: template,
  });
</script>

{#snippet template(args: ComponentProps<typeof RemoveGameModal>)}
  <RemoveGameModal {...args} />
{/snippet}

<Story name="With install folder" args={{}} />
<Story name="No install folder" args={{ folderPath: null }} />
