<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { tauriDecorator } from '../../../.storybook/tauri-mock';
  import { SAMPLE_CANDIDATES } from '../../../.storybook/fixtures';
  import Add from './+page.svelte';

  // On mount the Add page opens the OS file picker, then runs the chosen exe
  // through ludusavi. The decorator's base makes the picker resolve to a fixed
  // path (`plugin:dialog|open`); each story varies what `search_by_exe` returns
  // to land on a different stage of the flow.
  const EXE = 'C:/Games/HollowKnight/hollow_knight.exe';

  const { Story } = defineMeta({
    title: 'Screens/Add Game',
    component: Add,
    tags: ['!autodocs'],
    parameters: { layout: 'fullscreen' },
    decorators: [tauriDecorator({ 'plugin:dialog|open': EXE })],
  });
</script>

<!-- Identified: ranked candidate list, top match preselected. -->
<Story name="Matches" parameters={{ tauri: { search_by_exe: SAMPLE_CANDIDATES } }} />

<!-- Single confident match. -->
<Story name="Single match" parameters={{ tauri: { search_by_exe: [SAMPLE_CANDIDATES[0]] } }} />

<!-- Identifying: the spinner state, frozen by a search that never resolves. -->
<Story name="Identifying" parameters={{ tauri: { search_by_exe: () => new Promise(() => {}) } }} />

<!-- No automatic match: offers manual search / add-without-tracking. -->
<Story name="No match" parameters={{ tauri: { search_by_exe: [] } }} />

<!-- ludusavi errored: the error banner renders above the (empty) list. -->
<Story
  name="Error"
  parameters={{
    tauri: {
      search_by_exe: () => {
        throw new Error('ludusavi exited with code 1: manifest not found');
      },
    },
  }}
/>
