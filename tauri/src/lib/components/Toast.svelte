<script lang="ts">
  /**
   * Single toast — matches the design: 380px wide, left accent strip,
   * mono eyebrow + optional catalog + time, title, sub, optional CTA,
   * dismiss X in the corner.
   */
  import { X, Bug } from '@lucide/svelte';
  import {
    toasts,
    fmtToastTime,
    buildIssueUrl,
    type Toast,
  } from '$lib/toasts.svelte';
  import { api } from '$lib/api';
  import MonoLabel from './MonoLabel.svelte';

  let { toast }: { toast: Toast } = $props();

  // Opens a prefilled GitHub issue in the default browser, then dismisses.
  // `openPath` routes through the OS handler, which opens URLs too.
  async function openReport() {
    if (!toast.report) return;
    try {
      const url = await buildIssueUrl(toast.report);
      await api.openPath(url);
    } catch (e) {
      console.error('[toast] failed to open issue report:', e);
    }
    toasts.dismiss(toast.id);
  }

  // Kind → accent CSS variable so the left strip + eyebrow tint match.
  const accent = $derived(
    {
      ok: 'var(--color-ok)',
      info: 'var(--color-info)',
      warn: 'var(--color-warn)',
      bad: 'var(--color-bad)',
    }[toast.kind],
  );

  // Relative-time chip. Reading `toasts.tick` (which bumps every
  // second) keeps the $derived live so "12s" updates without a
  // separate setInterval in the component.
  const timeText = $derived.by(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    toasts.tick;
    return fmtToastTime(toast.createdAt);
  });

  // Determinate progress bar, shown only when `progress` is set.
  // Clamp to 0–1 so a stray value can't overflow the track.
  const hasProgress = $derived(toast.progress !== undefined);
  const progressPct = $derived(
    Math.round(Math.min(1, Math.max(0, toast.progress ?? 0)) * 100),
  );
</script>

<div
  class="relative flex w-[380px] overflow-hidden rounded-md border border-line-2 bg-bg-1"
  style:box-shadow="0 12px 32px rgb(0 0 0 / 0.5)"
>
  <!-- left accent strip -->
  <div class="w-1 self-stretch" style:background={accent}></div>

  <!-- Inner padding carves 36 px on the right so the absolute-
       positioned dismiss X never overlaps the header time chip /
       trailing sub text. The design mock has uniform padding and
       accepts the X overlapping the time, but in real use the
       collision obscures the last ~12 px of the time string —
       worth the small layout divergence to keep the chip readable. -->
  <div class="min-w-0 flex-1 py-3 pl-3.5 pr-9">
    <div class="mb-1.5 flex items-center justify-between gap-2">
      <div class="flex min-w-0 items-center gap-2">
        <MonoLabel size={9.5}>
          <span style:color={accent}>{toast.label}</span>
        </MonoLabel>
        {#if toast.catalog}
          <span
            class="font-mono text-[9px] tracking-[0.06em] text-ink-3"
          >
            {toast.catalog}
          </span>
        {/if}
      </div>
      <span
        class="font-mono shrink-0 text-[9.5px] uppercase tracking-[0.06em] text-ink-3"
      >
        {timeText}
      </span>
    </div>

    <div
      class="mb-0.5 text-[13.5px] font-semibold text-ink-0"
      style:letter-spacing="-0.005em"
    >
      {toast.title}
    </div>
    <div
      class="whitespace-pre-line text-[11.5px] text-ink-2"
      class:mb-2.5={!!toast.cta || !!toast.report || hasProgress}
      style:line-height="1.45"
    >
      {toast.sub}
    </div>

    {#if hasProgress}
      <div class="flex items-center gap-2">
        <div
          class="h-1 flex-1 overflow-hidden rounded-full bg-white/10"
          role="progressbar"
          aria-valuenow={progressPct}
          aria-valuemin="0"
          aria-valuemax="100"
        >
          <div
            class="h-full rounded-full transition-[width] duration-150 ease-out"
            style:width="{progressPct}%"
            style:background={accent}
          ></div>
        </div>
        <span
          class="font-mono shrink-0 text-[9.5px] tabular-nums text-ink-3"
        >
          {progressPct}%
        </span>
      </div>
    {/if}

    {#if toast.cta || toast.report}
      <div class="flex gap-1.5">
        {#if toast.cta}
          <button
            type="button"
            onclick={() => {
              toast.cta?.onClick();
              toasts.dismiss(toast.id);
            }}
            class="inline-flex h-6 cursor-pointer items-center rounded-sm border border-line-1 px-2.5 text-[11.5px] font-medium text-ink-0 transition-colors hover:bg-white/5"
          >
            {toast.cta.label}
          </button>
        {/if}
        {#if toast.report}
          <button
            type="button"
            onclick={openReport}
            class="inline-flex h-6 cursor-pointer items-center gap-1.5 rounded-sm border border-line-1 px-2.5 text-[11.5px] font-medium text-ink-0 transition-colors hover:bg-white/5"
          >
            <Bug size={12} />
            Report issue
          </button>
        {/if}
        <button
          type="button"
          onclick={() => toasts.dismiss(toast.id)}
          class="inline-flex h-6 cursor-pointer items-center rounded-sm border-none px-2.5 text-[11.5px] font-medium text-ink-2 transition-colors hover:text-ink-0"
        >
          Dismiss
        </button>
      </div>
    {/if}
  </div>

  <button
    type="button"
    onclick={() => toasts.dismiss(toast.id)}
    aria-label="Dismiss"
    class="absolute right-2 top-2 inline-flex size-[18px] cursor-pointer items-center justify-center rounded-sm text-ink-3 transition-colors hover:bg-white/5 hover:text-ink-0"
  >
    <X size={11} />
  </button>
</div>
