<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { fn } from 'storybook/test';
  import TransferPill from './TransferPill.svelte';

  // The titlebar transfer indicator: down/up counts + aggregate progress bars.
  const { Story } = defineMeta({
    title: 'Transfers/TransferPill',
    component: TransferPill,
    render: template,
    args: {
      downloadCount: 0,
      downloadPercent: 0,
      uploadCount: 0,
      uploadPercent: 0,
      open: false,
      onclick: fn(),
    },
  });
</script>

{#snippet template(args: ComponentProps<typeof TransferPill>)}
  <TransferPill {...args} />
{/snippet}

<Story name="Idle" args={{}} />
<Story name="Downloading" args={{ downloadCount: 1, downloadPercent: 48 }} />
<Story name="Uploading" args={{ uploadCount: 2, uploadPercent: 73 }} />
<Story name="Both" args={{ downloadCount: 1, downloadPercent: 48, uploadCount: 2, uploadPercent: 73 }} />
<Story name="Open" args={{ downloadCount: 1, downloadPercent: 48, uploadCount: 2, uploadPercent: 73, open: true }} />
