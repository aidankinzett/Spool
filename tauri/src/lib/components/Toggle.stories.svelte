<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import Toggle from './Toggle.svelte';

  const { Story } = defineMeta({
    title: 'Primitives/Toggle',
    component: Toggle,
    argTypes: {
      checked: { control: 'boolean' },
      disabled: { control: 'boolean' },
    },
    args: {
      checked: false,
      disabled: false,
    },
  });
</script>

{#snippet template(args: ComponentProps<typeof Toggle>)}
  <Toggle {...args} aria-label="Demo toggle" />
{/snippet}

<Story name="Off" args={{ checked: false }} />
<Story name="On" args={{ checked: true }} />
<Story name="Disabled Off" args={{ checked: false, disabled: true }} />
<Story name="Disabled On" args={{ checked: true, disabled: true }} />
