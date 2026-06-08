<script lang="ts">
  /**
   * LAN download source chooser.
   *
   * When the same game is shared by more than one device on the network, the
   * Download action opens this instead of silently pulling from an arbitrary
   * peer (issue #321). Lists each device that has the game; clicking one starts
   * the install from it. Picking is the whole interaction — there's no separate
   * confirm step, since installing is non-destructive (unlike the cloud-conflict
   * resolver this borrows its chrome from).
   *
   * Presentational: the host owns the live peer list and the install kickoff.
   * Lives over both the desktop library window and the touch layout.
   */
  import { ChevronRight, Wifi, X } from '@lucide/svelte';
  import type { LanPeer, PeerSource } from '$lib/types';
  import { gamepadScope } from '$lib/gamepad';
  import SpoolMark from '$lib/components/SpoolMark.svelte';

  const BRAND_SPOOL = '#d7c9a0';

  let {
    gameName,
    accent = null,
    coverUrl = null,
    sources,
    peers,
    context = 'desktop',
    onPick,
    onClose,
  }: {
    /** Display name of the game being downloaded. */
    gameName: string;
    /** Cover-art accent hex; falls back to the brand spool colour. */
    accent?: string | null;
    /** Webview-loadable cover URL; placeholder when null. */
    coverUrl?: string | null;
    /** Candidate devices to download from — already filtered to shareable +
     *  live by `liveSourcesFor` and name-sorted, so a row only needs the
     *  `online` re-check below to catch a device dropping while the modal is open. */
    sources: PeerSource[];
    /** Live peer list — joined per source for freshness + an offline guard. */
    peers: LanPeer[];
    /** Surface this floats over — only tweaks the scrim opacity. */
    context?: 'desktop' | 'gamemode';
    /** User chose a device to download from. */
    onPick: (source: PeerSource) => void;
    /** Dismiss via Cancel / the chrome close button / Escape / controller B. */
    onClose: () => void;
  } = $props();

  const acc = $derived(accent ?? BRAND_SPOOL);

  let hover = $state<Record<string, boolean>>({});

  /** Live view of one candidate — joins the source against the current peer
   *  list so a device that dropped off mid-modal shows as offline (and can't
   *  be picked into a guaranteed failure). */
  const rows = $derived(
    sources.map((source) => {
      const live = peers.find((p) => p.device_id === source.device_id);
      return {
        source,
        online: live != null && live.file_server_port > 0,
        lastSeenAgoSecs: live?.last_seen_ago_secs ?? null,
      };
    }),
  );

  // First still-online device gets controller focus.
  const firstOnlineId = $derived(rows.find((r) => r.online)?.source.device_id ?? null);

  function pick(source: PeerSource, online: boolean) {
    if (!online) return;
    onPick(source);
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={handleKey} />

<div
  class="ps-scrim fixed inset-0 z-50 flex items-center justify-center"
  style:padding="24px"
  style:background={context === 'desktop' ? 'rgba(4,5,7,0.62)' : 'rgba(4,5,7,0.5)'}
  style:backdrop-filter="blur(2px)"
  style:-webkit-backdrop-filter="blur(2px)"
>
  <div
    class="ps-modal flex flex-col overflow-hidden text-ink-0"
    role="dialog"
    aria-modal="true"
    aria-labelledby="ps-modal-title"
    use:gamepadScope={{ onBack: onClose }}
    style:width="480px"
    style:max-width="calc(100vw - 48px)"
    style:background="var(--color-bg-0)"
    style:border-radius="8px"
    style:box-shadow="0 32px 80px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.07)"
    style:--gp-focus={acc}
  >
    <!-- chrome -->
    <div
      class="flex items-center gap-3"
      style:height="32px"
      style:padding="0 8px 0 14px"
      style:background="rgba(0,0,0,0.32)"
      style:border-bottom="1px solid var(--color-line-1)"
    >
      <SpoolMark size={18} color="var(--color-ink-1)" tape={acc} />
      <span class="font-mono uppercase text-ink-2" style:font-size="10.5px" style:letter-spacing="0.12em">SPOOL</span>
      <span class="text-ink-3" style:font-size="10px">/</span>
      <span
        class="font-mono whitespace-nowrap uppercase text-ink-2"
        style:font-size="10.5px"
        style:letter-spacing="0.12em">LAN · CHOOSE SOURCE</span
      >
      <div class="flex-1"></div>
      <button
        type="button"
        onclick={onClose}
        aria-label="Close"
        class="inline-flex items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-bad/20 hover:text-[#ff9b9b]"
        style:width="28px"
        style:height="22px"
      >
        <X size={11} />
      </button>
    </div>

    <!-- hero -->
    <div
      class="flex items-start gap-[16px]"
      style:padding="18px 22px 16px"
      style:border-bottom="1px solid var(--color-line-1)"
    >
      <div
        class="shrink-0 overflow-hidden rounded-sm border border-line-1 bg-bg-2"
        style:width="50px"
        style:height="70px"
      >
        {#if coverUrl}
          <img src={coverUrl} alt={gameName} class="h-full w-full object-cover" />
        {:else}
          <div class="h-full w-full" style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"></div>
        {/if}
      </div>
      <div class="min-w-0 flex-1">
        <h1
          id="ps-modal-title"
          class="font-display"
          style:margin="0"
          style:font-size="21px"
          style:font-weight="700"
          style:letter-spacing="-0.02em"
          style:line-height="1.1"
        >
          Choose a device
        </h1>
        <div style:margin-top="5px" style:font-size="13px" style:color="var(--color-ink-1)" style:font-weight="500">
          {gameName}
        </div>
        <p style:margin="7px 0 0" style:font-size="12.5px" style:color="var(--color-ink-2)" style:line-height="1.45">
          Shared by {sources.length} devices on your network. Pick which one to download from.
        </p>
      </div>
    </div>

    <!-- device list -->
    <div class="flex flex-col" style:padding="12px" style:gap="8px" style:max-height="320px" style:overflow-y="auto">
      {#each rows as row (row.source.device_id)}
        {@const h = hover[row.source.device_id] && row.online}
        <button
          type="button"
          onclick={() => pick(row.source, row.online)}
          disabled={!row.online}
          data-gp-autofocus={row.source.device_id === firstOnlineId ? '' : undefined}
          onmouseenter={() => (hover[row.source.device_id] = true)}
          onmouseleave={() => (hover[row.source.device_id] = false)}
          class="group flex items-center gap-3 rounded-md p-0 text-left transition-[background,border-color] duration-150 disabled:cursor-not-allowed"
          style:padding="11px 13px"
          style:border="1px solid {h ? acc : 'var(--color-line-2)'}"
          style:background={h ? `${acc}12` : 'var(--color-bg-1)'}
          style:opacity={row.online ? 1 : 0.55}
          style:cursor={row.online ? 'pointer' : 'not-allowed'}
        >
          <span
            class="inline-flex shrink-0 items-center justify-center rounded-sm"
            style:width="30px"
            style:height="30px"
            style:background={row.online ? `${acc}1f` : 'var(--color-bg-3)'}
            style:color={row.online ? acc : 'var(--color-ink-3)'}
          >
            <Wifi size={15} />
          </span>
          <div class="min-w-0 flex-1">
            <div class="truncate text-[13px] font-semibold text-ink-0" title={row.source.device_name}>
              {row.source.device_name}
            </div>
            <div class="font-mono mt-0.5 flex items-center gap-2 text-[10px] tracking-[0.04em] text-ink-3">
              <span>{row.source.addr}</span>
              {#if row.online}
                {#if row.lastSeenAgoSecs != null}
                  <span>·</span>
                  <span>{row.lastSeenAgoSecs}s ago</span>
                {/if}
              {:else}
                <span>·</span>
                <span class="text-warn">offline</span>
              {/if}
            </div>
          </div>
          {#if row.online}
            <ChevronRight
              size={16}
              class="shrink-0 text-ink-3 transition-colors group-hover:text-ink-1"
            />
          {/if}
        </button>
      {/each}
    </div>

    <!-- footer -->
    <div
      class="flex items-center justify-end"
      style:padding="12px 22px 16px"
      style:border-top="1px solid var(--color-line-1)"
      style:background="rgba(0,0,0,0.18)"
    >
      <button
        type="button"
        onclick={onClose}
        onmouseenter={() => (hover['cancel'] = true)}
        onmouseleave={() => (hover['cancel'] = false)}
        class="inline-flex items-center justify-center rounded-sm font-medium transition-colors duration-100"
        style:height="34px"
        style:padding-inline="14px"
        style:font-size="13px"
        style:cursor="pointer"
        style:color="var(--color-ink-2)"
        style:border="1px solid var(--color-line-1)"
        style:background={hover['cancel'] ? 'rgb(255 255 255 / 0.06)' : 'transparent'}
      >
        Cancel
      </button>
    </div>
  </div>
</div>

<style>
  .ps-scrim {
    animation: ps-fade 160ms ease;
  }
  .ps-modal {
    animation: ps-pop 200ms ease;
  }
  @keyframes ps-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes ps-pop {
    from {
      opacity: 0;
      transform: translateY(10px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>
