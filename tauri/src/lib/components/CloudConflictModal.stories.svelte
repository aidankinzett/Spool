<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { fn } from 'storybook/test';
  import CloudConflictModal from './CloudConflictModal.svelte';

  // Local-vs-cloud save conflict resolver. Opens on the "choose" step; picking a
  // side calls `resolve` (here a no-op that resolves, sending it to "done").
  const LOCAL = { abs: 'Today · 22:40', rel: '6 hours ago', size: '34 MB' };
  const CLOUD = { abs: 'Today · 08:15', rel: '20 hours ago', size: '36 MB' };

  const { Story } = defineMeta({
    title: 'Modals/CloudConflictModal',
    component: CloudConflictModal,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    args: {
      gameName: 'The Witcher 3: Wild Hunt',
      catalogId: 'SPL-0001',
      accent: '#c9a36f',
      coverUrl: null,
      cloudNewer: true,
      localMeta: LOCAL,
      cloudMeta: CLOUD,
      context: 'desktop',
      showLudusavi: true,
      resolve: fn(async () => {}),
      onCancel: fn(),
      onContinue: fn(),
      onLudusavi: fn(),
      onClose: fn(),
    },
    render: template,
  });
</script>

{#snippet template(args: ComponentProps<typeof CloudConflictModal>)}
  <CloudConflictModal {...args} />
{/snippet}

<Story name="Cloud newer" args={{ cloudNewer: true }} />
<Story name="Local newer" args={{ cloudNewer: false }} />
<Story name="Game Mode context" args={{ context: 'gamemode' }} />
<Story name="No metadata" args={{ localMeta: null, cloudMeta: null }} />
