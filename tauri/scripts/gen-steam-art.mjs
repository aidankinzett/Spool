// Generates Spool's branded Steam "Big Picture" / library artwork as PNGs.
//
// The `add_spool_to_steam` command embeds these (include_bytes!) and drops them
// into Steam's grid dir so the Spool shortcut presents with brand art instead
// of a blank tile. Re-run after editing the designs below:
//
//   cd tauri && bun run scripts/gen-steam-art.mjs
//
// Output: tauri/src-tauri/assets/steam/{portrait,wide,hero,logo}.png
//
// Rendering uses @resvg/resvg-js with the same brand fonts the app ships
// (Space Grotesk for the wordmark, Geist for the tagline), loaded straight from
// node_modules so the output matches the in-app typography.

import { Resvg } from '@resvg/resvg-js';
import { decompress } from 'wawoff2';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const outDir = resolve(root, 'src-tauri/assets/steam');
mkdirSync(outDir, { recursive: true });

// resvg-js (2.6.x) loads TTF/OTF but not the .woff2 files @fontsource ships, so
// decompress each brand face to a TTF buffer in-memory before handing it over.
const FONT_DIR = resolve(root, 'node_modules/@fontsource');
async function woff2ToTtf(relPath) {
  const buf = readFileSync(resolve(FONT_DIR, relPath));
  return Buffer.from(await decompress(buf));
}
const fontBuffers = await Promise.all([
  woff2ToTtf('space-grotesk/files/space-grotesk-latin-700-normal.woff2'),
  woff2ToTtf('space-grotesk/files/space-grotesk-latin-600-normal.woff2'),
  woff2ToTtf('geist-sans/files/geist-sans-latin-500-normal.woff2'),
  woff2ToTtf('geist-sans/files/geist-sans-latin-600-normal.woff2'),
]);

// ── Brand tokens (mirror app.css) ────────────────────────────────────────────
const BG_0 = '#0b0c0e'; // window void
const BG_1 = '#101216'; // pane
const BG_2 = '#15181d'; // raised
const OXIDE = '#d7c9a0'; // tape oxide accent
const INK = '#f4f4f5';

// Cassette brand mark (the SpoolMark glyph), drawn into a 0 0 220 160 box so it
// scales cleanly. `s` strokes the shell/reels; `tape` fills the bottom strip.
function cassette(s, tape, { reelFill = s } = {}) {
  return `
    <g transform="scale(10)">
      <rect x="0.75" y="0.75" width="20.5" height="14.5" rx="1.4" stroke="${s}" stroke-width="1.5" fill="none"/>
      <circle cx="6.5" cy="8" r="2.4" stroke="${s}" stroke-width="1.4" fill="none"/>
      <circle cx="6.5" cy="8" r="0.7" fill="${reelFill}"/>
      <circle cx="15.5" cy="8" r="2.4" stroke="${s}" stroke-width="1.4" fill="none"/>
      <circle cx="15.5" cy="8" r="0.7" fill="${reelFill}"/>
      <rect x="3" y="12.5" width="16" height="1.4" rx="0.4" fill="${tape}" opacity="0.85"/>
    </g>`;
}

// Shared gradient + soft oxide glow defs, parameterised by id suffix.
function defs(id) {
  return `
    <defs>
      <linearGradient id="bg${id}" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="${BG_1}"/>
        <stop offset="0.55" stop-color="${BG_0}"/>
        <stop offset="1" stop-color="#070809"/>
      </linearGradient>
      <radialGradient id="glow${id}" cx="0.5" cy="0.42" r="0.6">
        <stop offset="0" stop-color="${OXIDE}" stop-opacity="0.16"/>
        <stop offset="1" stop-color="${OXIDE}" stop-opacity="0"/>
      </radialGradient>
    </defs>`;
}

// A faint hairline grid texture, cassette-deck-panel feel.
function hairlines(w, h, gap = 26, opacity = 0.04) {
  let lines = '';
  for (let y = gap; y < h; y += gap) {
    lines += `<line x1="0" y1="${y}" x2="${w}" y2="${y}" stroke="${INK}" stroke-width="1" opacity="${opacity}"/>`;
  }
  return lines;
}

// ── Portrait capsule 600×900 (the library tile) ──────────────────────────────
const portrait = `
<svg xmlns="http://www.w3.org/2000/svg" width="600" height="900" viewBox="0 0 600 900">
  ${defs('P')}
  <rect width="600" height="900" fill="url(#bgP)"/>
  <rect width="600" height="900" fill="url(#glowP)"/>
  ${hairlines(600, 900, 30, 0.035)}
  <g transform="translate(190,250)">${cassette(OXIDE, OXIDE, { reelFill: INK })}</g>
  <text x="300" y="640" text-anchor="middle" font-family="Space Grotesk" font-weight="700"
        font-size="118" letter-spacing="2" fill="${INK}">SPOOL</text>
  <text x="300" y="694" text-anchor="middle" font-family="Geist" font-weight="500"
        font-size="26" letter-spacing="11" fill="${OXIDE}">GAME LIBRARY</text>
  <rect x="210" y="726" width="180" height="3" rx="1.5" fill="${OXIDE}" opacity="0.55"/>
</svg>`;

// ── Wide capsule 920×430 ─────────────────────────────────────────────────────
const wide = `
<svg xmlns="http://www.w3.org/2000/svg" width="920" height="430" viewBox="0 0 920 430">
  ${defs('W')}
  <rect width="920" height="430" fill="url(#bgW)"/>
  <rect width="920" height="430" fill="url(#glowW)"/>
  ${hairlines(920, 430, 28, 0.04)}
  <g transform="translate(90,135) scale(0.82)">${cassette(OXIDE, OXIDE, { reelFill: INK })}</g>
  <text x="330" y="225" font-family="Space Grotesk" font-weight="700"
        font-size="118" letter-spacing="1" fill="${INK}">SPOOL</text>
  <text x="334" y="278" font-family="Geist" font-weight="500"
        font-size="24" letter-spacing="10" fill="${OXIDE}">GAME LIBRARY</text>
</svg>`;

// ── Hero banner 1920×620 (atmospheric — the logo overlays it) ─────────────────
const hero = `
<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="620" viewBox="0 0 1920 620">
  <defs>
    <linearGradient id="bgH" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="${BG_2}"/>
      <stop offset="0.5" stop-color="${BG_0}"/>
      <stop offset="1" stop-color="#070809"/>
    </linearGradient>
    <radialGradient id="glowH" cx="0.74" cy="0.5" r="0.55">
      <stop offset="0" stop-color="${OXIDE}" stop-opacity="0.18"/>
      <stop offset="1" stop-color="${OXIDE}" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="fadeH" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="${BG_0}" stop-opacity="0.85"/>
      <stop offset="0.6" stop-color="${BG_0}" stop-opacity="0"/>
    </linearGradient>
  </defs>
  <rect width="1920" height="620" fill="url(#bgH)"/>
  <rect width="1920" height="620" fill="url(#glowH)"/>
  ${hairlines(1920, 620, 34, 0.035)}
  <g transform="translate(1180,150) scale(1.5)" opacity="0.9">${cassette(OXIDE, OXIDE, { reelFill: INK })}</g>
  <rect width="1920" height="620" fill="url(#fadeH)"/>
</svg>`;

// ── Logo (transparent, overlays the hero) ────────────────────────────────────
const logo = `
<svg xmlns="http://www.w3.org/2000/svg" width="900" height="380" viewBox="0 0 900 380">
  <g transform="translate(340,0) scale(1.0)">${cassette(OXIDE, OXIDE, { reelFill: INK })}</g>
  <text x="450" y="290" text-anchor="middle" font-family="Space Grotesk" font-weight="700"
        font-size="150" letter-spacing="2" fill="${INK}">SPOOL</text>
  <text x="450" y="346" text-anchor="middle" font-family="Geist" font-weight="500"
        font-size="30" letter-spacing="13" fill="${OXIDE}">GAME LIBRARY</text>
</svg>`;

function render(name, svg) {
  const resvg = new Resvg(svg, {
    font: { fontBuffers, loadSystemFonts: false, defaultFontFamily: 'Geist' },
    background: 'rgba(0,0,0,0)',
  });
  const png = resvg.render().asPng();
  const path = resolve(outDir, `${name}.png`);
  writeFileSync(path, png);
  console.log(`wrote ${path} (${png.length} bytes)`);
}

render('portrait', portrait);
render('wide', wide);
render('hero', hero);
render('logo', logo);
console.log('done');
