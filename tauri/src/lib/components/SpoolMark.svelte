<script lang="ts">
  /**
   * Spool brand mark — cassette glyph (two reels + tape strip across bottom).
   * Renders identical on Win/Linux/macOS — no OS-specific accent.
   *
   * `color` is the body/shell stroke. `tape` is the tape-strip fill (typically
   * the cover-art accent in the live app; falls back to `color`).
   */
  let {
    size = 22,
    color = 'currentColor',
    tape,
    dim = false,
  }: {
    size?: number;
    color?: string;
    tape?: string;
    dim?: boolean;
  } = $props();

  const tapeColor = $derived(tape ?? color);
</script>

<svg
  width={size}
  height={size * (16 / 22)}
  viewBox="0 0 22 16"
  fill="none"
  class="block shrink-0"
>
  <!-- shell -->
  <rect
    x="0.75"
    y="0.75"
    width="20.5"
    height="14.5"
    rx="1.4"
    stroke={color}
    stroke-width="1.5"
    fill="none"
    opacity={dim ? 0.45 : 1}
  />
  <!-- reels -->
  <circle cx="6.5" cy="8" r="2.4" stroke={color} stroke-width="1.4" fill="none" />
  <circle cx="6.5" cy="8" r="0.7" fill={color} />
  <circle cx="15.5" cy="8" r="2.4" stroke={color} stroke-width="1.4" fill="none" />
  <circle cx="15.5" cy="8" r="0.7" fill={color} />
  <!-- tape strip across bottom -->
  <rect x="3" y="12.5" width="16" height="1.4" rx="0.4" fill={tapeColor} opacity="0.85" />
</svg>
