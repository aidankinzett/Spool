<script lang="ts">
  /**
   * Story harness for the main library window. LibraryDesktop / LibraryTouch
   * take a `lib` controller (from `createLibrary()`), which registers its own
   * onMount to load games/config/peers from the backend. `createLibrary()` must
   * run during component init for that onMount to fire — so it's created here,
   * and the Tauri mock (installed by the wrapping `tauriDecorator`) feeds it.
   */
  import { createLibrary } from '$lib/library.svelte';
  import LibraryDesktop from '$lib/components/LibraryDesktop.svelte';
  import LibraryTouch from '$lib/components/LibraryTouch.svelte';

  let { layout = 'desktop' }: { layout?: 'desktop' | 'touch' } = $props();

  const lib = createLibrary();
</script>

{#if layout === 'touch'}
  <LibraryTouch {lib} />
{:else}
  <LibraryDesktop {lib} />
{/if}
