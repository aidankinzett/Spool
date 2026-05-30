<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import Btn from './Btn.svelte';

  const { Story } = defineMeta({
    title: 'Primitives/Btn',
    component: Btn,
    argTypes: {
      variant: {
        control: 'select',
        options: ['ghost', 'primary', 'secondary', 'danger'],
      },
      accent: { control: 'color' },
      full: { control: 'boolean' },
      disabled: { control: 'boolean' },
    },
    args: {
      variant: 'ghost',
      full: false,
      disabled: false,
    },
  });
</script>

<!-- Shared template: args drive the variant; the label is the children snippet. -->
{#snippet template(args: Omit<ComponentProps<typeof Btn>, 'children'>)}
  <Btn {...args}>Button</Btn>
{/snippet}

<Story name="Ghost" args={{ variant: 'ghost' }} />
<Story name="Primary" args={{ variant: 'primary' }} />
<Story name="Secondary" args={{ variant: 'secondary' }} />
<Story name="Danger" args={{ variant: 'danger' }} />
<Story name="Primary (custom accent)" args={{ variant: 'primary', accent: '#7ee2a4' }} />
<Story name="Disabled" args={{ variant: 'primary', disabled: true }} />
