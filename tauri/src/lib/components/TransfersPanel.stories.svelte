<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { fn } from 'storybook/test';
  import { makeDownload, SAMPLE_UPLOADS } from '../../../.storybook/fixtures';
  import TransfersPanel from './TransfersPanel.svelte';

  // The expanded transfers dropdown: the active download plus per-peer uploads.
  // `coverFor` resolves a game id to a cover URL — null here, so rows show the
  // placeholder tile.
  const { Story } = defineMeta({
    title: 'Transfers/TransfersPanel',
    component: TransfersPanel,
    render: template,
    args: {
      download: null,
      uploads: [],
      onCancelDownload: fn(),
      onCancelUpload: fn(),
      coverFor: () => null,
    },
  });
</script>

{#snippet template(args: ComponentProps<typeof TransfersPanel>)}
  <div style="width: 360px">
    <TransfersPanel {...args} />
  </div>
{/snippet}

<Story name="Empty" args={{}} />
<Story name="Downloading" args={{ download: makeDownload() }} />
<Story name="Preparing" args={{ download: makeDownload({ status: 'starting', current_file: 'Fetching manifest…', bytes_done: 0 }) }} />
<Story name="Uploading" args={{ uploads: SAMPLE_UPLOADS }} />
<Story name="Download + uploads" args={{ download: makeDownload(), uploads: SAMPLE_UPLOADS }} />
