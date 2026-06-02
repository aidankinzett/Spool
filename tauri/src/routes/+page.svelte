<script lang="ts">
  import { uiMode } from '$lib/uiMode.svelte';
  import { createLibrary } from '$lib/library.svelte';
  import LibraryDesktop from '$lib/components/LibraryDesktop.svelte';
  import LibraryTouch from '$lib/components/LibraryTouch.svelte';
  import CloudConflictModal from '$lib/components/CloudConflictModal.svelte';
  import SuspendedLockModal from '$lib/components/SuspendedLockModal.svelte';
  import OnboardingModal from '$lib/components/OnboardingModal.svelte';
  import { api, assetUrl } from '$lib/api';
  import { toasts } from '$lib/toasts.svelte';
  import { absDateTime, relDate, fmtSize } from '$lib/format';
  import type { RawConflictDetails } from '$lib/types';
  import { onMount } from 'svelte';

  const lib = createLibrary();

  let conflictDetails = $state<RawConflictDetails | null>(null);

  // First-run onboarding — show the flow over the library when a fresh config
  // hasn't completed it yet. Returning users (pre-existing configs) are
  // migrated to completed on the backend, so this only fires on a new install.
  let showOnboarding = $state(false);
  onMount(async () => {
    try {
      const cfg = await api.getConfig();
      if (!cfg.onboarding_completed) showOnboarding = true;
    } catch (e) {
      console.error('[library] onboarding check failed:', e);
    }
  });

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
    const localTime = conflictDetails?.local?.modified;
    const cloudTime = conflictDetails?.cloud?.modified;
    if (localTime && cloudTime) {
      return new Date(cloudTime) > new Date(localTime);
    }
    return !localTime;
  });
</script>

{#if uiMode.resolved === 'touch'}
  <LibraryTouch {lib} />
{:else}
  <LibraryDesktop {lib} />
{/if}

{#if showOnboarding}
  <OnboardingModal
    onfinish={() => {
      showOnboarding = false;
      toasts.show({
        kind: 'ok',
        label: 'SETUP',
        title: "You're all set",
        sub: 'Welcome to Spool — add your first game to get started.',
      });
    }}
  />
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

{#if lib.suspendedConflict}
  {@const sc = lib.suspendedConflict}
  {@const suspendedGame = lib.games.find((g) => g.id === sc.gameId)}
  {#if suspendedGame}
    <SuspendedLockModal
      gameName={suspendedGame.game_name}
      deviceName={sc.deviceName}
      catalogId={suspendedGame.catalog_number ? `SPL-${String(suspendedGame.catalog_number).padStart(4, '0')}` : undefined}
      accent={suspendedGame.accent_color}
      coverUrl={assetUrl(suspendedGame.cover_image_path)}
      context={uiMode.resolved === 'touch' ? 'gamemode' : 'desktop'}
      onConfirm={async () => {
        const id = sc.gameId;
        lib.suspendedConflict = null;
        try {
          await api.launchGame(id, true);
        } catch (e) {
          console.error('[library] override launch failed:', e);
        }
      }}
      onCancel={() => {
        lib.suspendedConflict = null;
      }}
    />
  {/if}
{/if}
