<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import CassetteProgress from './CassetteProgress.svelte';

  const { Story } = defineMeta({
    title: 'Transfers/CassetteProgress',
    component: CassetteProgress,
    render: template,
    argTypes: {
      percent: { control: { type: 'range', min: 0, max: 100 } },
      accent: { control: 'color' },
      dir: { control: 'inline-radio', options: ['down', 'up'] },
      sourceKind: { control: 'inline-radio', options: [null, 'lan', 'cloud'] },
      height: { control: { type: 'range', min: 2, max: 16 } },
    },
    args: { percent: 48, accent: '#d7c9a0', label: '620 MB / 1.3 GB · 48 MB/s', height: 6 },
  });
</script>

<!-- Fixed-width wrapper so the bar has a consistent footprint. -->
{#snippet template(args: ComponentProps<typeof CassetteProgress>)}
  <div style="width: 360px">
    <CassetteProgress {...args} />
  </div>
{/snippet}

<Story name="Download (LAN)" args={{ dir: 'down', sourceKind: 'lan', source: 'Celeste · Steam Deck' }} />
<Story name="Upload (LAN)" args={{ dir: 'up', sourceKind: 'lan', accent: '#7ee2a4', label: '3.0 GB / 9.0 GB · 60 MB/s', source: 'Hollow Knight · ROG Ally' }} />
<Story name="Cloud sync" args={{ dir: 'up', sourceKind: 'cloud', accent: '#7ec6ff', label: 'Mirroring to remote…', source: 'Dropbox' }} />
<Story name="Complete" args={{ percent: 100, label: 'Done', sourceKind: 'lan', source: 'Celeste · Steam Deck' }} />
