<script lang="ts" module>
  /** Buttons we render glyphs for. Face buttons use the Xbox letter + colour
   *  convention (the most widely recognised); shoulders/system buttons are
   *  neutral pills so they read on the graphite UI. */
  export type GpButton = 'a' | 'b' | 'x' | 'y' | 'lb' | 'rb' | 'lt' | 'rt' | 'menu' | 'view';
</script>

<script lang="ts">
  let { button, size = 18 }: { button: GpButton; size?: number } = $props();

  // Xbox face-button palette, lightly muted to sit on the dark UI.
  const FACE: Record<string, { fill: string; letter: string }> = {
    a: { fill: '#6cc04a', letter: 'A' },
    b: { fill: '#e0524a', letter: 'B' },
    x: { fill: '#4a86e0', letter: 'X' },
    y: { fill: '#e0b341', letter: 'Y' },
  };

  const face = $derived(FACE[button]);
  const pillText = $derived(
    button === 'lb'
      ? 'LB'
      : button === 'rb'
        ? 'RB'
        : button === 'lt'
          ? 'LT'
          : button === 'rt'
            ? 'RT'
            : button === 'menu'
              ? '≡'
              : button === 'view'
                ? '⧉'
                : '',
  );
</script>

{#if face}
  <!-- Round face button: coloured disc, dark letter. -->
  <span
    class="gp-face"
    style:width="{size}px"
    style:height="{size}px"
    style:background={face.fill}
    style:font-size="{size * 0.58}px"
  >
    {face.letter}
  </span>
{:else}
  <!-- Shoulder / system button: neutral pill. -->
  <span
    class="gp-pill"
    style:height="{size}px"
    style:min-width="{size}px"
    style:font-size="{size * 0.5}px"
    style:padding-inline="{size * 0.28}px"
  >
    {pillText}
  </span>
{/if}

<style>
  .gp-face {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 9999px;
    color: #0b0c0e;
    font-family: var(--font-sans);
    font-weight: 700;
    line-height: 1;
    flex-shrink: 0;
  }
  .gp-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    background: var(--color-bg-3);
    border: 1px solid var(--color-line-2);
    color: var(--color-ink-1);
    font-family: var(--font-mono);
    font-weight: 600;
    line-height: 1;
    flex-shrink: 0;
  }
</style>
