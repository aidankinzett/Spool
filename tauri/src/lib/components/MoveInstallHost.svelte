<script lang="ts">
  /**
   * Global host for {@link MoveInstallModal}, driven by the
   * {@link moveInstallDialog} store. Mounted once per window in `+layout.svelte`
   * (next to `<RemoveGameHost />`), so any view can open the move-install chooser
   * via `moveInstallDialog.request(game)`.
   *
   * The host loads the configured library folders (the move destinations); the
   * modal itself owns the move IPC + `move:progress` wiring. On success the
   * backend emits `library:changed`, refreshing every open window.
   */
  import MoveInstallModal from '$lib/components/MoveInstallModal.svelte';
  import { moveInstallDialog } from '$lib/moveInstall.svelte';
  import { api } from '$lib/api';
  import type { LibraryFolder } from '$lib/types';

  const req = $derived(moveInstallDialog.current);
  let folders = $state<LibraryFolder[]>([]);

  // Re-fetch the configured folders each time the dialog opens (`req` is a
  // fresh object per `.request()` call, so the effect re-runs per open) —
  // reopening, even for the same game after editing library folders in
  // Settings, refreshes the list.
  $effect(() => {
    if (req) {
      void (async () => {
        try {
          const c = await api.getConfig();
          folders = c.library_folders ?? [];
        } catch {
          folders = [];
        }
      })();
    }
  });
</script>

{#if req}
  <MoveInstallModal
    game={req.game}
    {folders}
    onClose={() => moveInstallDialog.close()}
    onDone={req.onDone}
  />
{/if}
