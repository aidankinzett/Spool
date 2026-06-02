import type { Preview } from '@storybook/sveltekit';
import { mockIPC, mockConvertFileSrc } from '@tauri-apps/api/mocks';

// Global styles: design tokens, fonts, and Tailwind layer. This is the same
// stylesheet the real app loads from `routes/+layout.svelte`.
import '../src/app.css';

// Mirrors the global `<ConfirmHost />` in `routes/+layout.svelte` so any story
// whose component calls `confirmDialog()` has somewhere to render the modal.
import ConfirmHostDecorator from './ConfirmHostDecorator.svelte';

/**
 * Stub the Tauri boundary so components that reach for it render in the
 * browser instead of throwing.
 *
 *   - `mockConvertFileSrc` installs `window.__TAURI_INTERNALS__.convertFileSrc`
 *     so `assetUrl()` (used for cover art / hero images) returns a URL rather
 *     than crashing. The file won't actually exist, so the image simply won't
 *     load — fine for layout/state work. Stories that need real imagery should
 *     pass their own placeholder URLs.
 *   - `mockIPC` intercepts every `invoke()` call. The default below resolves
 *     `undefined` so an incidental command at render time is harmless. A story
 *     for an api-coupled component can re-`mockIPC` in a decorator/loader to
 *     return canned data for the specific commands it calls.
 */
mockConvertFileSrc('windows');
mockIPC(() => undefined);

const preview: Preview = {
  // Generate a Docs page for every component from its argTypes + stories.
  tags: ['autodocs'],
  // Mount ConfirmHost around every story (the story renders into the
  // decorator's `children` snippet) so confirmDialog()-driven modals appear.
  decorators: [(story) => ({ Component: ConfirmHostDecorator, props: { children: story } })],
  parameters: {
    layout: 'centered',
    backgrounds: {
      default: 'spool',
      values: [{ name: 'spool', value: '#0b0c0e' }],
    },
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
  },
};

export default preview;
