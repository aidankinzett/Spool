/**
 * Design tokens — TS mirror of `src/app.css` `@theme` block.
 *
 * Use this only when you need a *raw value* in JS (e.g. an SVG `fill`,
 * a dynamically-computed accent for inline style). For all CSS use the
 * Tailwind class equivalents (`bg-bg-1`, `text-ink-0`, `font-mono`, …)
 * — they read from the CSS custom properties so the two stay aligned.
 */

export const TOK = {
  c: {
    bg0: '#0b0c0e',
    bg1: '#101216',
    bg2: '#15181d',
    bg3: '#1c2027',

    line1: 'rgb(255 255 255 / 0.06)',
    line2: 'rgb(255 255 255 / 0.10)',
    line3: 'rgb(255 255 255 / 0.16)',

    ink0: '#f4f4f5',
    ink1: 'rgb(244 244 245 / 0.78)',
    ink2: 'rgb(244 244 245 / 0.56)',
    ink3: 'rgb(244 244 245 / 0.36)',

    ok: '#7ee2a4',
    warn: '#f4b66c',
    info: '#7ec6ff',
    bad: '#ff7a7a',

    spool: '#d7c9a0',
    spoolDeep: '#1a1612',
  },
  font: {
    sans: '"Geist", system-ui, sans-serif',
    display: '"Space Grotesk", system-ui, sans-serif',
    mono: '"JetBrains Mono", ui-monospace, monospace',
  },
  r: { xs: 2, sm: 3, md: 5, lg: 8, pill: 999 },
} as const;

/** Lighten/darken a hex color by `percent` (negative = darker). */
export function shadeHex(hex: string, percent: number): string {
  let c = hex.replace('#', '');
  if (c.length === 3) c = c.split('').map((x) => x + x).join('');
  const num = parseInt(c, 16);
  const shift = Math.round((255 * percent) / 100);
  const r = Math.max(0, Math.min(255, (num >> 16) + shift));
  const g = Math.max(0, Math.min(255, ((num >> 8) & 0xff) + shift));
  const b = Math.max(0, Math.min(255, (num & 0xff) + shift));
  return '#' + ((r << 16) | (g << 8) | b).toString(16).padStart(6, '0');
}
