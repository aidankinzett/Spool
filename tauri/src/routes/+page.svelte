<script lang="ts">
  import { uiMode } from '$lib/uiMode.svelte';
  import { createLibrary } from '$lib/library.svelte';
  import LibraryDesktop from '$lib/components/LibraryDesktop.svelte';
  import LibraryTouch from '$lib/components/LibraryTouch.svelte';
  import CloudConflictModal from '$lib/components/CloudConflictModal.svelte';
  import { api, assetUrl } from '$lib/api';

  const lib = createLibrary();
</script>

{#if uiMode.resolved === 'touch'}
  <LibraryTouch {lib} />
{:else}
  <LibraryDesktop {lib} />
{/if}

{#if lib.conflictGameId}
  {@const conflictGame = lib.games.find((g) => g.id === lib.conflictGameId)}
  {#if conflictGame}
    <CloudConflictModal
      gameName={conflictGame.game_name}
      catalogId={conflictGame.catalog_number ? `SPL-${String(conflictGame.catalog_number).padStart(4, '0')}` : undefined}
      accent={conflictGame.accent_color}
      coverUrl={assetUrl(conflictGame.cover_image_path)}
      context={uiMode.resolved === 'touch' ? 'gamemode' : 'desktop'}
      resolve={async (side) => {
        await api.resolveCloudConflict(conflictGame.id, side);
      }}
      onCancel={() => {
        lib.conflictGameId = null;
      }}
      onContinue={async () => {
        const id = lib.conflictGameId;
        lib.conflictGameId = null;
        if (id) {
          try {
            await api.launchGame(id);
          } catch (e) {
            console.error('[library] retry launch failed:', e);
          }
        }
      }}
      onLudusavi={() => {
        api.openLudusaviGui().catch((e) => console.error('[ludusavi] open failed:', e));
      }}
      onClose={() => {
        lib.conflictGameId = null;
      }}
    />
  {/if}
{/if}
