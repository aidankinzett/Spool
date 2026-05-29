<script lang="ts">
  // Mode-aware chrome wrapper: renders TouchTopBar on touch, WindowChrome
  // on desktop. This is the one place allowed to branch on uiMode for
  // structure — it's a chrome substitution, not a layout branch.
  import { uiMode } from '$lib/uiMode.svelte';
  import WindowChrome from './WindowChrome.svelte';
  import TouchTopBar from './TouchTopBar.svelte';

  let {
    sub,
    accent,
    onback,
    peers = 0,
    transfers = 0,
    conflict = false,
    children,
  }: {
    sub?: string;
    accent?: string;
    /** Touch only: shown as a back button. Omit on the root library page. */
    onback?: () => void;
    /** LAN peer count — forwarded to TouchTopBar sync pill. */
    peers?: number;
    /** Active transfer count — forwarded to TouchTopBar badge. */
    transfers?: number;
    /** Sync conflict/offline flag — drives amber alert state. */
    conflict?: boolean;
    children?: import('svelte').Snippet;
  } = $props();
</script>

{#if uiMode.resolved === 'touch'}
  <TouchTopBar {sub} {accent} {onback} {peers} {transfers} {conflict}>
    {#if children}{@render children()}{/if}
  </TouchTopBar>
{:else}
  <WindowChrome {sub} {accent}>
    {#if children}{@render children()}{/if}
  </WindowChrome>
{/if}
