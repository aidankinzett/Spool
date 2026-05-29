<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import ToastStack from '$lib/components/ToastStack.svelte';
  import { api } from '$lib/api';
  import { uiMode } from '$lib/uiMode.svelte';

  let { children } = $props();

  // Resolve the UI mode once per window at boot, after config loads, so
  // <html data-mode> is set before the user interacts. Every window runs
  // this layout, so each resolves its own data-mode.
  onMount(async () => {
    try {
      const config = await api.getConfig();
      await uiMode.init(config.ui_mode);
      if (uiMode.resolved === 'touch') {
        // Deck/Ally are always fullscreen — maximize before first paint.
        await getCurrentWindow().maximize();
      }
    } catch (e) {
      console.error('[layout] uiMode init failed; defaulting to auto:', e);
      await uiMode.init('auto');
    }
  });
</script>

{@render children()}

<!-- Global toast stack — overlaid on every route. -->
<ToastStack />
