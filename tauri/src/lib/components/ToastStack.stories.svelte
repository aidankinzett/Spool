<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import ToastHarness from '../../../.storybook/ToastHarness.svelte';

  // ToastStack renders the global toast store. The harness seeds it with sample
  // toasts (sticky, so they don't fade during review).
  const { Story } = defineMeta({
    title: 'Feedback/ToastStack',
    component: ToastHarness,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    argTypes: { which: { control: 'inline-radio', options: ['mixed', 'single'] } },
  });
</script>

<Story name="Mixed (ok / info / warn / bad)" args={{ which: 'mixed' }} />
<Story name="Single" args={{ which: 'single' }} />
