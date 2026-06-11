<script lang="ts">
  import { uiMode } from '$lib/uiMode.svelte';
  import { createLibrary } from '$lib/library.svelte';
  import LibraryDesktop from '$lib/components/LibraryDesktop.svelte';
  import LibraryTouch from '$lib/components/LibraryTouch.svelte';
  import CloudConflictModal from '$lib/components/CloudConflictModal.svelte';
  import SuspendedLockModal from '$lib/components/SuspendedLockModal.svelte';
  import OnboardingModal from '$lib/components/OnboardingModal.svelte';
  import PeerSourceModal from '$lib/components/PeerSourceModal.svelte';
  import InstallLocationModal from '$lib/components/InstallLocationModal.svelte';
  import { api, assetUrl, peerAssetUrl } from '$lib/api';
  import { openView } from '$lib/nav';
  import { toasts } from '$lib/toasts.svelte';
  import { absDateTime, relDate, fmtSize, isNewerVersion } from '$lib/format';
  import type { RawConflictDetails } from '$lib/types';
  import { onMount } from 'svelte';
  import { listen, emit } from '@tauri-apps/api/event';

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

  // On Linux, the Decky companion plugin is bundled inside the AppImage. When a
  // Spool update ships a newer plugin than the copy installed under
  // ~/homebrew, nudge the user to update it — the Settings → Game Mode
  // companion card shows the same state, but this surfaces it without a visit.
  // Fires at most once per bundled version: the version it was shown for is
  // recorded in config so a re-launch on the same Spool build stays quiet.
  onMount(async () => {
    try {
      if ((await api.appPlatform()) !== 'linux') return;
      const decky = await api.deckyPluginStatus();
      if (!decky.supported || !decky.installed || !decky.installedVersion) return;
      if (!isNewerVersion(decky.bundledVersion, decky.installedVersion)) return;
      const cfg = await api.getConfig();
      if (cfg.decky_update_notified_version === decky.bundledVersion) return;
      cfg.decky_update_notified_version = decky.bundledVersion;
      await api.updateConfig(cfg);
      const id = toasts.show({
        kind: 'info',
        label: 'DECKY',
        title: 'Backup plugin update available',
        sub: `Installed v${decky.installedVersion} · bundled v${decky.bundledVersion}. Update it from Settings.`,
        duration: 12000,
        cta: {
          label: 'Settings',
          onClick: () => {
            toasts.dismiss(id);
            void openView('settings');
          },
        },
      });
    } catch (e) {
      console.error('[library] decky update check failed:', e);
    }
  });

  // Offer to switch to the Gamepad layout when a controller is present and the
  // user is on Auto that resolved to Desktop (e.g. a PC on the TV — no
  // touchscreen to auto-detect). Only nudges Auto users (an explicit Desktop
  // choice is respected) and only once per session. The pad is read via the
  // Rust bridge, since the webview Gamepad API is empty on Linux.
  let gamepadPrompted = false;

  async function switchToGamepad() {
    try {
      const cfg = await api.getConfig();
      cfg.ui_mode = 'touch';
      await api.updateConfig(cfg);
      await emit('config:ui-mode-changed');
    } catch (e) {
      console.error('[library] switch to gamepad layout failed:', e);
    }
  }

  function maybePromptGamepad() {
    if (gamepadPrompted) return;
    if (uiMode.setting !== 'auto' || uiMode.resolved !== 'desktop') return;
    gamepadPrompted = true;
    const id = toasts.show({
      kind: 'info',
      label: 'CONTROLLER',
      title: 'Controller detected',
      sub: 'Switch to the Gamepad layout for bigger targets and controller navigation.',
      duration: 12000,
      cta: {
        label: 'Switch',
        onClick: () => {
          toasts.dismiss(id);
          void switchToGamepad();
        },
      },
    });
  }

  onMount(() => {
    // A pad already plugged in at boot (the common TV-PC case) is reported by
    // the bridge's startup enumeration, not as an event — so query once.
    api.anyGamepadConnected()
      .then((present) => {
        if (present) maybePromptGamepad();
      })
      .catch((e) => console.error('[library] gamepad presence check failed:', e));

    // …and catch hotplugs while the app is running.
    let unlisten: (() => void) | undefined;
    listen<{ kind: string }>('gamepad:input', (e) => {
      if (e.payload.kind === 'connected') maybePromptGamepad();
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => console.error('[library] gamepad listen failed:', e));

    return () => unlisten?.();
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

{#if lib.peerChoice}
  {@const pc = lib.peerChoice}
  <PeerSourceModal
    gameName={pc.game.game_name}
    accent={pc.game.accent_color}
    coverUrl={assetUrl(pc.game.cover_image_path) ??
      (pc.sources[0] ? peerAssetUrl(pc.sources[0], 'cover') : null)}
    sources={pc.sources}
    peers={lib.lanPeers}
    context={uiMode.resolved === 'touch' ? 'gamemode' : 'desktop'}
    onPick={(source) => {
      void lib.chooseDownloadSource(source);
    }}
    onClose={() => {
      lib.peerChoice = null;
    }}
  />
{/if}

{#if lib.installLocationAsk}
  {@const ask = lib.installLocationAsk}
  <InstallLocationModal
    gameName={ask.game.game_name}
    coverUrl={peerAssetUrl(
      {
        device_id: ask.peer.device_id,
        device_name: ask.peer.device_name,
        addr: ask.peer.addr,
        file_server_port: ask.peer.file_server_port,
        source_game_id: ask.game.id,
        shareable: true,
      },
      'cover',
    )}
    context={uiMode.resolved === 'touch' ? 'gamemode' : 'desktop'}
    onConfirm={(path) => {
      void lib.confirmInstallLocation(path);
    }}
    onClose={() => {
      lib.installLocationAsk = null;
    }}
  />
{/if}

{#if lib.suspendedConflict}
  {@const sc = lib.suspendedConflict}
  {@const suspendedGame = lib.games.find((g) => g.id === sc.gameId)}
  {#if suspendedGame}
    <SuspendedLockModal
      gameName={suspendedGame.game_name}
      deviceName={sc.deviceName}
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
