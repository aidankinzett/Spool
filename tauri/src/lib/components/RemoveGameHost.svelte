<script lang="ts">
  /**
   * Global host for {@link RemoveGameModal}, driven by the
   * {@link removeGameDialog} store. Mounted once per window in `+layout.svelte`
   * (next to `<ConfirmHost />`), so any view can open the three-option remove
   * chooser via `removeGameDialog.request(game)`.
   *
   * This is where the chosen action turns into IPC + a toast — the modal itself
   * stays presentational. On success the backend emits `library:changed`, which
   * refreshes every open window; the optional `onDone` runs too (e.g. closing
   * the edit window).
   */
  import RemoveGameModal, { type RemoveChoice } from '$lib/components/RemoveGameModal.svelte';
  import { removeGameDialog } from '$lib/removeGame.svelte';
  import { api, assetUrl } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import { fmtCatalog } from '$lib/format';

  const req = $derived(removeGameDialog.current);

  async function perform(choice: RemoveChoice) {
    const r = removeGameDialog.current;
    if (!r) return;
    const g = r.game;
    if (choice === 'disk') {
      await api.uninstallGame(g.id);
      toasts.show({
        kind: 'ok',
        label: 'REMOVE · DISK',
        title: 'Removed from disk',
        sub: `${g.game_name} — library entry kept`,
        catalog: fmtCatalog(g.catalog_number),
      });
    } else if (choice === 'both') {
      await api.deleteGameFromDisk(g.id);
      toasts.show({
        kind: 'ok',
        label: 'DELETE · DONE',
        title: 'Deleted from disk and library',
        sub: g.game_name,
        catalog: fmtCatalog(g.catalog_number),
      });
    } else {
      await api.removeGame(g.id);
      toasts.show({
        kind: 'ok',
        label: 'REMOVE · ENTRY',
        title: 'Removed from library',
        sub: `${g.game_name} — files left on disk`,
        catalog: fmtCatalog(g.catalog_number),
      });
    }
    // Throwing above leaves the modal on its error step; on success we run the
    // caller's follow-up (the modal closes itself once `perform` resolves).
    r.onDone?.();
  }
</script>

{#if req}
  <!-- folderPath is strictly `game_folder_path` (NOT the exe's parent): the
       disk options must only be offered when there's a real recorded folder the
       backend (wipe_install_files) will actually delete, so the modal can't
       promise to remove a path it won't touch. -->
  <RemoveGameModal
    gameName={req.game.game_name}
    accent={req.game.accent_color}
    coverUrl={assetUrl(req.game.cover_image_path)}
    folderPath={req.game.game_folder_path}
    {perform}
    onClose={() => removeGameDialog.close()}
  />
{/if}
