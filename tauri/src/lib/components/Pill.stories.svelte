<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import Pill from './Pill.svelte';

  const { Story } = defineMeta({
    title: 'Primitives/Pill',
    component: Pill,
    argTypes: {
      kind: {
        control: 'select',
        options: ['ok', 'warn', 'info', 'bad', 'off'],
      },
      soft: { control: 'boolean' },
    },
    args: {
      kind: 'info',
      soft: false,
    },
  });
</script>

<!-- The label echoes the `kind` so each variant is self-describing. -->
{#snippet template(args: Omit<ComponentProps<typeof Pill>, 'children'>)}
  <Pill {...args}>{args.kind}</Pill>
{/snippet}

<Story name="OK" args={{ kind: 'ok' }} />
<Story name="Warn" args={{ kind: 'warn' }} />
<Story name="Info" args={{ kind: 'info' }} />
<Story name="Bad" args={{ kind: 'bad' }} />
<Story name="Off" args={{ kind: 'off' }} />
<Story name="Soft (info)" args={{ kind: 'info', soft: true }} />
