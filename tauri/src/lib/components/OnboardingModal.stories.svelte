<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { fn } from 'storybook/test';
  import { tauriDecorator } from '../../../.storybook/tauri-mock';
  import { makeConfig } from '../../../.storybook/fixtures';
  import OnboardingModal from './OnboardingModal.svelte';

  // First-run onboarding flow. Loads config/platform/cloud status on mount, so
  // it renders behind the Tauri mock. Step through it with the in-modal nav.
  const { Story } = defineMeta({
    title: 'Modals/OnboardingModal',
    component: OnboardingModal,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    args: { onfinish: fn() },
    decorators: [tauriDecorator()],
  });
</script>

<!-- Fresh install on Windows: cloud not yet connected. -->
<Story
  name="Windows"
  parameters={{ tauri: { app_platform: 'windows', get_config: makeConfig({ onboarding_completed: false }), check_cloud_remote_exists: false } }}
/>

<!-- Steam Deck: the flow also surfaces the Decky backup plugin step. -->
<Story
  name="Linux (Steam Deck)"
  parameters={{
    tauri: {
      app_platform: 'linux',
      get_config: makeConfig({ onboarding_completed: false, device_name: 'Steam Deck' }),
      check_cloud_remote_exists: false,
      decky_plugin_status: { supported: true, installed: false, decky_present: true, bundled_version: '1.2.0' },
    },
  }}
/>
