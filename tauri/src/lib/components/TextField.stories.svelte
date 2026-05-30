<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import TextField from './TextField.svelte';

  const { Story } = defineMeta({
    title: 'Primitives/TextField',
    component: TextField,
    argTypes: {
      placeholder: { control: 'text' },
      mono: { control: 'boolean' },
      masked: { control: 'boolean' },
      readonly: { control: 'boolean' },
    },
    args: {
      value: '',
      mono: false,
      masked: false,
      readonly: false,
    },
  });
</script>

<!-- Fixed-width wrapper so the inline-flex field has a consistent footprint. -->
{#snippet template(args: ComponentProps<typeof TextField>)}
  <div style="width: 280px">
    <TextField {...args} full />
  </div>
{/snippet}

<Story name="Empty" args={{ value: '', placeholder: 'Enter text…' }} />
<Story name="Filled" args={{ value: 'Hollow Knight' }} />
<Story name="Mono (path)" args={{ value: 'C:\\Games\\HollowKnight', mono: true }} />
<Story name="Masked (API key)" args={{ value: 'sgdb_secret_key', masked: true }} />
<Story name="Readonly" args={{ value: 'Cannot edit me', readonly: true }} />
