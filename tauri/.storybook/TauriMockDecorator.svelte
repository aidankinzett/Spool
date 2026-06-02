<script lang="ts">
  /**
   * Wraps a screen-level story and installs a Tauri IPC mock before the
   * wrapped page mounts. The page's onMount IPC calls (list_games, get_config,
   * …) and any dialog/window plugin calls all route through `invoke`, so a
   * single `mockIPC` covers them.
   *
   * `mockIPC` is global, so this is only safe because each story renders in its
   * own canvas iframe (screen metas set `tags: ['!autodocs']` to avoid the Docs
   * page stacking several at once). The mock is set up synchronously here in the
   * decorator's init — which runs before the child story page mounts.
   */
  import type { Snippet } from 'svelte';
  import { installTauriMock, type TauriHandlers } from './tauri-mock';

  let { handlers, children }: { handlers: TauriHandlers; children: Snippet } = $props();

  // Runs during component init, before <children /> (the story page) mounts.
  // The decorator only ever uses the initial handlers, so reading the prop
  // once here (not reactively) is intended.
  // svelte-ignore state_referenced_locally
  installTauriMock(handlers);
</script>

{@render children()}
