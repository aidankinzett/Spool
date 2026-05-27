/**
 * Spool icon dictionary — line icons drawn on a 16×16 viewBox.
 *
 * `d` is the SVG path data. `fill = true` means render as a solid shape
 * (the path is closed); default is stroke-only with rounded caps and joins.
 *
 * Add new icons here as the design grows. Names stay short and lowercase.
 */

export type IconDef = { d: string; fill?: boolean };

const _icons = {
  play: { d: 'M4 3.2v9.6L13 8z', fill: true },
  search: { d: 'M7 12.5a5.5 5.5 0 1 0 0-11 5.5 5.5 0 0 0 0 11Zm4-1.5 3 3' },
  plus: { d: 'M8 3.5v9M3.5 8h9' },
  folder: {
    d: 'M2 4.5A1.5 1.5 0 0 1 3.5 3h2.6l1.4 1.4h5A1.5 1.5 0 0 1 14 5.9V12a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12V4.5Z',
  },
  cog: {
    d: 'M8 5.6a2.4 2.4 0 1 1 0 4.8 2.4 2.4 0 0 1 0-4.8Zm0-3.6.7 1.4 1.5-.3.6 1.4 1.5.3-.3 1.5 1.4.7-1.4.7.3 1.5-1.5.3-.6 1.4-1.5-.3-.7 1.4-.7-1.4-1.5.3-.6-1.4-1.5-.3.3-1.5L2 8l1.4-.7-.3-1.5 1.5-.3.6-1.4 1.5.3z',
  },
  wifi: { d: 'M2 6.5a8 8 0 0 1 12 0M4.2 9a5 5 0 0 1 7.6 0M6.5 11.5a2 2 0 0 1 3 0M8 13.5h.01' },
  download: { d: 'M8 2.5v8.5m0 0L4.5 7.6M8 11l3.5-3.4M3 13.5h10' },
  upload: { d: 'M8 13.5V5m0 0L4.5 8.4M8 5l3.5 3.4M3 2.5h10' },
  cloud: { d: 'M4.5 11.5a3 3 0 0 1-.3-6 3.5 3.5 0 0 1 6.8-.6 2.8 2.8 0 0 1 .5 5.6Z' },
  trash: { d: 'M2.5 4.5h11M6 4.5V3a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.5M4 4.5l.6 8a1 1 0 0 0 1 .9h4.8a1 1 0 0 0 1-.9l.6-8' },
  pencil: { d: 'm3 13 1-3 7-7 2 2-7 7Z' },
  copy: { d: 'M5 4.5h7.5v9H5zM4 12V3.5h7' },
  external: { d: 'M9.5 3h3.5v3.5M13 3 8 8M11 9v3.5H3.5V5H7' },
  chev: { d: 'm4 6 4 4 4-4' },
  chevR: { d: 'm6 4 4 4-4 4' },
  check: { d: 'm3.5 8.5 3 3 6-7' },
  close: { d: 'M3.5 3.5 12.5 12.5M12.5 3.5l-9 9' },
  steam: { d: 'M8 2.5a5.5 5.5 0 0 0-5.5 5.5l3 1.2A2 2 0 0 1 8 8.2L10.4 6a2.4 2.4 0 1 1 2.4 2.4M5.5 11.5a1.5 1.5 0 1 0 1.5-1.5' },
  signal: { d: 'M2 13.5v-2M5 13.5v-5M8 13.5v-8M11 13.5v-11' },
  clock: { d: 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12ZM8 5v3l2 1.5' },
  hdd: { d: 'M2.5 3.5h11v9h-11zM5 7h6M5 10h3' },
  gamepad: { d: 'M3 6.5h2.5M4.25 5.5v2M11 6.5h.01M9.5 8h.01M2.5 11l1-4a2 2 0 0 1 2-1.5h5a2 2 0 0 1 2 1.5l1 4a1.5 1.5 0 0 1-2.4 1.4l-1.6-1.4h-3l-1.6 1.4A1.5 1.5 0 0 1 2.5 11Z' },
  shield: { d: 'M8 2 3 4v4c0 3 2.5 5 5 6 2.5-1 5-3 5-6V4Z' },
  share: { d: 'M11 5a1.8 1.8 0 1 0 0-3.6A1.8 1.8 0 0 0 11 5Zm0 9.6A1.8 1.8 0 1 0 11 11a1.8 1.8 0 0 0 0 3.6ZM5 9.8a1.8 1.8 0 1 0 0-3.6 1.8 1.8 0 0 0 0 3.6Zm1.5-2.4 3 1.6m0-3.6-3 1.6' },
  sparkle: { d: 'M8 2v3m0 6v3M2 8h3m6 0h3M4 4l2 2m4 4 2 2M4 12l2-2m4-4 2-2' },
  exe: { d: 'M3 2.5h7.5L13 5v8.5H3zM10.5 2.5V5H13M5 9l1.5 1.5L5 12M8 12h3' },
  reel: { d: 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12ZM8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3ZM4 8h1.5M10.5 8H12M8 4v1.5M8 10.5V12' },
  source: { d: 'M8 2 2 4.5 8 7l6-2.5L8 2ZM2 8l6 2.5L14 8M2 11.5 8 14l6-2.5' },
  device: { d: 'M2.5 3.5h11v7h-11zM6 13h4M7 10.5v2.5M9 10.5v2.5' },
  key: { d: 'M10.5 9a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Zm-1.7 1.3L5 14l-1.5-1.5L4.5 11.5 3 10l1.8-1.7' },
  eye: { d: 'M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8s-2.5 4.5-6.5 4.5S1.5 8 1.5 8ZM8 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z' },
  filter: { d: 'M2 3h12L9.5 8.5V13L6.5 11.5V8.5Z' },
  grid: { d: 'M3 3h4v4H3zM9 3h4v4H9zM3 9h4v4H3zM9 9h4v4H9z' },
  list: { d: 'M5.5 4h8M5.5 8h8M5.5 12h8M3 4h.01M3 8h.01M3 12h.01' },
  controller: { d: 'M2 10c0-2 1-4 3-4h6c2 0 3 2 3 4s-1 2-2 2-1.5-1-2-1H6c-.5 0-1 1-2 1s-2 0-2-2ZM5 8h1.5M6 7.5v1M10 8h.01M11 9h.01M11.5 7h.01' },
  // Window controls — paths centered in the 16×16 viewBox so they render
  // correctly when the Icon is placed in a flex-centered button.
  winMin: { d: 'M4 8h8' },
  winMax: { d: 'M4.5 4.5h7v7h-7z' },
  winClose: { d: 'M4.5 4.5 11.5 11.5M11.5 4.5l-7 7' },
} as const satisfies Record<string, IconDef>;

export type IconName = keyof typeof _icons;

// Widen each entry to the open `IconDef` so consumers can read `def.fill`
// without TypeScript narrowing it away on the entries that omit `fill`.
export const icons: Record<IconName, IconDef> = _icons;
