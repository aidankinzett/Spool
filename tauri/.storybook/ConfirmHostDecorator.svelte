<script lang="ts">
  /**
   * Global Storybook decorator that mounts <ConfirmHost /> alongside every
   * story — the Storybook equivalent of the single `<ConfirmHost />` in
   * `routes/+layout.svelte`. Without it, any component that calls
   * `confirmDialog()` (e.g. GameDetail's "Restore an earlier save") would
   * await a promise that never renders a dialog.
   */
  import type { Snippet } from 'svelte';
  import ConfirmHost from '$lib/components/ConfirmHost.svelte';

  let { children }: { children: Snippet } = $props();
</script>

{@render children()}

<!-- Mounted once per story so confirmDialog() has somewhere to render. -->
<ConfirmHost />
