<script lang="ts" module>
  export interface SelectOption {
    value: string;
    label: string;
  }
</script>

<script lang="ts">
  /**
   * Dropdown select — a focusable trigger that opens a listbox popup. Replaces
   * the native `<select>`, which is awkward under gamepad control (the OS
   * dropdown can't be driven by the nav engine) and doesn't scale with density.
   *
   *   - Sized from --control-h / --text-base, so it grows in gamepad mode.
   *   - The open listbox is its own nav scope: dpad up/down moves between
   *     options, A selects, B / Escape closes. Initial focus lands on the
   *     current value. Mouse/touch click works the same.
   *   - The popup is fixed-positioned off the trigger rect (and flips above when
   *     there isn't room below), so it escapes the settings panes' overflow.
   */
  import { ChevronDown, Check } from '@lucide/svelte';
  import { gamepadScope } from '$lib/gamepad';

  let {
    value = $bindable(''),
    options,
    onchange,
    disabled = false,
    placeholder = 'Select…',
    full = false,
  }: {
    value: string;
    options: SelectOption[];
    onchange?: (v: string) => void;
    disabled?: boolean;
    placeholder?: string;
    full?: boolean;
  } = $props();

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement>();
  let menuEl = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0, width: 0, above: false, maxH: 280 });

  const selected = $derived(options.find((o) => o.value === value) ?? null);

  // Compute the popup's fixed-position rect off the trigger's current screen
  // box. Split out from `openMenu` so it can re-run on scroll/resize while open.
  function computePos() {
    if (!triggerEl) return;
    const r = triggerEl.getBoundingClientRect();
    const gap = 4;
    const spaceBelow = window.innerHeight - r.bottom - gap;
    const spaceAbove = r.top - gap;
    // Flip above only when below is too cramped and above has more room.
    const above = spaceBelow < 200 && spaceAbove > spaceBelow;
    const maxH = Math.min(280, Math.max(120, (above ? spaceAbove : spaceBelow)));
    pos = {
      left: r.left,
      top: above ? r.top - gap : r.bottom + gap,
      width: r.width,
      above,
      maxH,
    };
  }

  function openMenu() {
    if (disabled || !triggerEl) return;
    computePos();
    open = true;
  }

  // The popup is `position: fixed` off a rect captured at open time, so any
  // scroll (in an ancestor pane) or window resize would otherwise leave it
  // floating away from its trigger. While open, recompute on those events —
  // capture-phase so inner scroll containers (the settings panes) are caught,
  // since scroll events don't bubble.
  $effect(() => {
    if (!open) return;
    const onReflow = () => computePos();
    window.addEventListener('scroll', onReflow, true);
    window.addEventListener('resize', onReflow);
    return () => {
      window.removeEventListener('scroll', onReflow, true);
      window.removeEventListener('resize', onReflow);
    };
  });

  function close() {
    open = false;
  }

  function pick(v: string) {
    value = v;
    onchange?.(v);
    close();
    // Return focus to the trigger so nav continues from here.
    triggerEl?.focus();
  }

  function onWindowKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      e.stopPropagation();
      close();
      triggerEl?.focus();
    }
  }

  function onOutside(e: MouseEvent) {
    if (!open) return;
    const t = e.target as Node;
    if (menuEl?.contains(t) || triggerEl?.contains(t)) return;
    close();
  }
</script>

<svelte:window onkeydown={onWindowKey} onpointerdown={onOutside} />

<button
  bind:this={triggerEl}
  type="button"
  {disabled}
  aria-haspopup="listbox"
  aria-expanded={open}
  onclick={() => (open ? close() : openMenu())}
  class="inline-flex cursor-pointer items-center justify-between gap-2 rounded-sm border border-line-1 bg-bg-2 text-ink-0 transition-colors hover:border-line-3 disabled:cursor-not-allowed disabled:opacity-50 {full
    ? 'w-full'
    : ''}"
  style:height="var(--control-h)"
  style:padding-inline="calc(var(--space-unit) * 2.5)"
  style:font-size="var(--text-base)"
  style:min-width="10rem"
>
  <span class="truncate" class:text-ink-3={!selected}>
    {selected ? selected.label : placeholder}
  </span>
  <ChevronDown size={15} class="shrink-0 text-ink-2" />
</button>

{#if open}
  <div
    bind:this={menuEl}
    role="listbox"
    use:gamepadScope={{ onBack: () => { close(); triggerEl?.focus(); } }}
    class="fixed z-[70] overflow-y-auto rounded-md border border-line-2 bg-bg-1 py-1"
    style:left="{pos.left}px"
    style:width="{pos.width}px"
    style:min-width="10rem"
    style:max-height="{pos.maxH}px"
    style:box-shadow="0 18px 48px rgb(0 0 0 / 0.6)"
    style:top={pos.above ? 'auto' : `${pos.top}px`}
    style:bottom={pos.above ? `${window.innerHeight - pos.top}px` : 'auto'}
  >
    {#each options as opt (opt.value)}
      {@const active = opt.value === value}
      <button
        type="button"
        role="option"
        aria-selected={active}
        data-gp-autofocus={active ? '' : undefined}
        onclick={() => pick(opt.value)}
        class="flex w-full items-center gap-2 text-left text-ink-1 transition-colors hover:bg-bg-3 hover:text-ink-0"
        style:padding="calc(var(--space-unit) * 2) calc(var(--space-unit) * 2.5)"
        style:font-size="var(--text-base)"
        style:background={active ? 'var(--color-bg-3)' : 'transparent'}
        style:color={active ? 'var(--color-ink-0)' : undefined}
      >
        <span class="flex w-4 shrink-0 justify-center" style:color="var(--color-spool)">
          {#if active}<Check size={14} />{/if}
        </span>
        <span class="truncate">{opt.label}</span>
      </button>
    {/each}
  </div>
{/if}
