<script lang="ts">
  import { uiMode } from '$lib/uiMode.svelte';
  import { createLibrary } from '$lib/library.svelte';
  import LibraryDesktop from '$lib/components/LibraryDesktop.svelte';
  import LibraryTouch from '$lib/components/LibraryTouch.svelte';
  import CloudConflictModal from '$lib/components/CloudConflictModal.svelte';
  import { api, assetUrl } from '$lib/api';
  import { absDateTime, relDate, fmtSize } from '$lib/format';
  import type { RawConflictDetails } from '$lib/types';

  const lib = createLibrary();

  let conflictDetails = $state<RawConflictDetails | null>(null);

  $effect(() => {
    const id = lib.conflictGameId;
    if (id) {
      conflictDetails = null;
      api.getCloudConflictDetails(id)
        .then((res) => {
          if (lib.conflictGameId === id) {
            conflictDetails = res;
          }
        })
        .catch((e) => {
          console.error('[library] failed to fetch conflict details:', e);
        });
    } else {
      conflictDetails = null;
    }
  });

  const localMeta = $derived.by(() => {
    if (!conflictDetails?.local || !conflictDetails.local.modified) return null;
    const mb = conflictDetails.local.size_bytes / (1024 * 1024);
    return {
      abs: absDateTime(conflictDetails.local.modified),
      rel: relDate(conflictDetails.local.modified),
      size: fmtSize(mb),
    };
  });

  const cloudMeta = $derived.by(() => {
    if (!conflictDetails?.cloud || !conflictDetails.cloud.modified) return null;
    const mb = conflictDetails.cloud.size_bytes / (1024 * 1024);
    return {
      abs: absDateTime(conflictDetails.cloud.modified),
      rel: relDate(conflictDetails.cloud.modified),
      size: fmtSize(mb),
    };
  });

  const cloudNewer = $derived.by(() => {
    if (!conflictDetails?.local?.modified || !conflictDetails?.cloud?.modified) return true;
    return new Date(conflictDetails.cloud.modified) > new Date(conflictDetails.local.modified);
  });
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
      cloudNewer={cloudNewer}
      localMeta={localMeta}
      cloudMeta={cloudMeta}
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
