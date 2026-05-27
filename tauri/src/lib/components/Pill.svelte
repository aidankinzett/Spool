<script lang="ts">
  /**
   * Status pill — small label with a coloured dot + mono caption.
   *
   * `kind` drives the palette (status meter colors from the design system).
   * `soft` swaps the filled background for a hairline border — same colour
   * key, less weight, used in dense rows.
   */
  type Kind = 'ok' | 'warn' | 'info' | 'bad' | 'off';

  let {
    kind = 'info',
    soft = false,
    children,
  }: {
    kind?: Kind;
    soft?: boolean;
    children: import('svelte').Snippet;
  } = $props();

  // Each kind maps to: bg color (soft fill), fg text color, dot color.
  // Using inline style for the palette so a single component covers all
  // variants without exploding Tailwind output with arbitrary values.
  const palette = $derived(
    {
      ok: { bg: 'rgb(126 226 164 / 0.10)', fg: '#a5edc1', dot: 'var(--color-ok)' },
      warn: { bg: 'rgb(244 182 108 / 0.10)', fg: '#f6cf94', dot: 'var(--color-warn)' },
      info: { bg: 'rgb(126 198 255 / 0.10)', fg: '#a5d5ff', dot: 'var(--color-info)' },
      bad: { bg: 'rgb(255 122 122 / 0.10)', fg: '#ffa6a6', dot: 'var(--color-bad)' },
      off: { bg: 'rgb(255 255 255 / 0.04)', fg: 'var(--color-ink-2)', dot: 'var(--color-ink-3)' },
    }[kind],
  );
</script>

<span
  class="font-mono inline-flex h-[18px] items-center gap-1.5 whitespace-nowrap rounded-sm px-1.5 text-[9.5px] uppercase leading-none tracking-[0.1em]"
  style:background={soft ? 'transparent' : palette.bg}
  style:border={soft ? `1px solid ${palette.dot}44` : 'none'}
  style:color={palette.fg}
>
  <span class="size-[5px] rounded-full" style:background={palette.dot}></span>
  {@render children()}
</span>
