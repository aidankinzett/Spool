<script lang="ts">
  import { uiMode } from '$lib/uiMode.svelte';
  import { createLibrary } from '$lib/library.svelte';
  import LibraryDesktop from '$lib/components/LibraryDesktop.svelte';
  import LibraryTouch from '$lib/components/LibraryTouch.svelte';

  const lib = createLibrary();
</script>

{#if uiMode.resolved === 'touch'}
  <LibraryTouch {lib} />
{:else}
  <LibraryDesktop {lib} />
{/if}
