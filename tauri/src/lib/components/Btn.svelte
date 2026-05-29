<script lang="ts">
  /**
   * Button — four variants matching the design system.
   *
   *   ghost     — hairline border, transparent bg, fills on hover (default)
   *   primary   — solid accent fill (uses `accent` prop, defaults to spool)
   *   secondary — raised card surface, used inside dense panels
   *   danger    — hairline + red hover, for destructive actions
   *
   * Caller controls icons via the `icon` snippet (renders before the label).
   * Use `full` to stretch to container width.
   */
  import { shadeHex } from '../tokens';
  import { TOK } from '../tokens';

  type Variant = 'ghost' | 'primary' | 'secondary' | 'danger';

  let {
    variant = 'ghost',
    accent,
    full = false,
    disabled = false,
    type = 'button',
    onclick,
    icon,
    children,
    class: className = '',
  }: {
    variant?: Variant;
    /** Hex color used for the `primary` variant background. */
    accent?: string;
    full?: boolean;
    disabled?: boolean;
    type?: 'button' | 'submit' | 'reset';
    onclick?: (e: MouseEvent) => void;
    icon?: import('svelte').Snippet;
    children: import('svelte').Snippet;
    class?: string;
  } = $props();

  let hover = $state(false);

  const acc = $derived(accent ?? TOK.c.spool);

  // Inline style is the cleanest path here — variants need dynamic colours
  // (the primary accent shifts with cover art) and dynamic hover state.
  const style = $derived.by(() => {
    if (variant === 'primary') {
      return {
        background: hover ? shadeHex(acc, -10) : acc,
        color: '#0b0c0e',
        border: '1px solid transparent',
      };
    }
    if (variant === 'secondary') {
      return {
        background: hover ? 'var(--color-bg-3)' : 'var(--color-bg-2)',
        color: 'var(--color-ink-0)',
        border: '1px solid var(--color-line-2)',
      };
    }
    if (variant === 'danger') {
      return {
        background: hover ? 'rgb(255 122 122 / 0.18)' : 'transparent',
        color: hover ? '#ffa6a6' : 'var(--color-ink-1)',
        border: '1px solid var(--color-line-1)',
      };
    }
    // ghost
    return {
      background: hover ? 'rgb(255 255 255 / 0.06)' : 'transparent',
      color: 'var(--color-ink-0)',
      border: '1px solid var(--color-line-1)',
    };
  });
</script>

<button
  {type}
  {disabled}
  {onclick}
  onmouseenter={() => (hover = true)}
  onmouseleave={() => (hover = false)}
  class="inline-flex cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-sm text-[length:var(--text-base)] font-medium transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-50 {full
    ? 'w-full'
    : ''} {className}"
  style:height="var(--control-h)"
  style:padding-inline="calc(var(--space-unit) * 3)"
  style:background={style.background}
  style:color={style.color}
  style:border={style.border}
>
  {#if icon}{@render icon()}{/if}
  {@render children()}
</button>
