<script lang="ts">
  import { createLibrary } from '$lib/library.svelte';
  import LibraryDesktop from '$lib/components/LibraryDesktop.svelte';

  const lib = createLibrary();
</script>

<LibraryDesktop {lib} />
