<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import ToastStack from '$lib/components/ToastStack.svelte';
  import ConfirmHost from '$lib/components/ConfirmHost.svelte';
  import RemoveGameHost from '$lib/components/RemoveGameHost.svelte';
  import MoveInstallHost from '$lib/components/MoveInstallHost.svelte';
  import { api } from '$lib/api';
  import { uiMode } from '$lib/uiMode.svelte';
  import { startGamepadNav } from '$lib/gamepad';
  import { startVirtualKeyboard } from '$lib/virtualKeyboard';
  import { platform } from '@tauri-apps/plugin-os';

  let { children } = $props();

  // Resolve the UI mode once per window at boot, after config loads, so
  // <html data-mode> is set before the user interacts. Every window runs
  // this layout, so each resolves its own data-mode.
  async function resolveMode() {
    const config = await api.getConfig();
    await uiMode.init(config.ui_mode);
    if (uiMode.resolved === 'touch') {
      // Deck/Ally are always fullscreen — maximize before first paint.
      // Isolated from init() above: a maximize failure must NOT fall through
      // to resolveMode()'s caller and reset the mode to 'auto', which would
      // silently drop touch back to desktop on every launch (issue #60).
      try {
        await getCurrentWindow().maximize();
      } catch (e) {
        console.error('[layout] maximize failed (non-fatal):', e);
      }
    }
  }

  onMount(() => {
    // Wire controller / keyboard spatial navigation for every window. Idempotent
    // and self-disabling when the Tauri event bridge isn't present.
    startGamepadNav();

    // Pop KDE's on-screen keyboard when a text field is focused — WebKitGTK
    // won't do it itself. Linux-only; the backend commands no-op off KDE, but
    // gate here too so other platforms fire no IPC on every focus change.
    let stopVk: (() => void) | undefined;
    if (platform() === 'linux') stopVk = startVirtualKeyboard();

    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        await resolveMode();
      } catch (e) {
        console.error('[layout] uiMode init failed; defaulting to auto:', e);
        await uiMode.init('auto');
      }
      // Re-resolve live when the user flips the mode in Settings (the change
      // is emitted from whichever window owns the Settings view).
      unlisten = await listen('config:ui-mode-changed', () => {
        resolveMode().catch((e) =>
          console.error('[layout] uiMode re-resolve failed:', e),
        );
      });
    })();
    return () => {
      unlisten?.();
      stopVk?.();
    };
  });
</script>

{@render children()}

<!-- Global toast stack — overlaid on every route. -->
<ToastStack />

<!-- Global confirmation-dialog host — replaces the unreliable window.confirm(). -->
<ConfirmHost />

<!-- Global remove-game chooser host — opened via removeGameDialog.request(). -->
<RemoveGameHost />

<!-- Global move-install chooser host — opened via moveInstallDialog.request(). -->
<MoveInstallHost />
