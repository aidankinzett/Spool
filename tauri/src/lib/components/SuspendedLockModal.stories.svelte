<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { fn } from 'storybook/test';
  import SuspendedLockModal from './SuspendedLockModal.svelte';

  // Shown when launching a game another device holds in a suspended,
  // not-yet-synced session — offers a "play here anyway" override.
  const { Story } = defineMeta({
    title: 'Modals/SuspendedLockModal',
    component: SuspendedLockModal,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    args: {
      gameName: 'Hollow Knight',
      deviceName: 'Steam Deck',
      catalogId: 'SPL-0001',
      accent: '#6fb7c9',
      coverUrl: null,
      context: 'desktop',
      onConfirm: fn(),
      onCancel: fn(),
    },
    render: template,
  });
</script>

{#snippet template(args: ComponentProps<typeof SuspendedLockModal>)}
  <SuspendedLockModal {...args} />
{/snippet}

<Story name="Desktop" args={{ context: 'desktop' }} />
<Story name="Game Mode" args={{ context: 'gamemode' }} />
