/**
 * Regenerate the README screenshots from Storybook.
 *
 * Builds the static Storybook (so the captures match committed component code),
 * serves it locally, then drives a headless Chromium over each story's bare
 * `iframe.html` and writes a PNG per shot into `docs/screenshots/`. Finally it
 * rewrites the `src`/`width`/`height` of every matching marker block in
 * `README.md`, so re-running the script refreshes both the images and their
 * dimensions with no manual edits.
 *
 * The Library shots pull real cover art from Steam's CDN (the same URLs the
 * fixtures use), so this needs network access. The Decky / Game-Mode shots in
 * the README are real SteamOS captures and aren't reproducible here, so they
 * have no marker block and are left untouched.
 *
 *   bun run screenshots            # build Storybook, then capture
 *   bun run screenshots --no-build # reuse an existing storybook-static/
 *   bun run screenshots --no-readme# capture PNGs only, don't touch README
 */
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir, stat, copyFile } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, join, extname } from 'node:path';
import { chromium } from 'playwright';

const __dirname = dirname(fileURLToPath(import.meta.url));
const tauriDir = resolve(__dirname, '..');
const repoRoot = resolve(tauriDir, '..');
const staticDir = join(tauriDir, 'storybook-static');
const outDir = join(repoRoot, 'docs', 'screenshots');
const readmePath = join(repoRoot, 'README.md');
// The docs site is a separate Astro project, so it can only optimise images
// that live inside its own src/. Mirror the captures there too; the guide
// markdown references these copies with stable relative paths.
const docsAssetsDir = join(repoRoot, 'docs-site', 'src', 'assets', 'screenshots');

const args = new Set(process.argv.slice(2));
const skipBuild = args.has('--no-build');
const skipReadme = args.has('--no-readme');

/**
 * The shots to capture. `title`/`name` resolve to a story id via Storybook's
 * index.json. `page` mode screenshots the whole viewport (for `fullscreen`
 * layout stories); otherwise `selector` is element-cropped to that node.
 * `htmlMode` sets `<html data-mode>` so density-scaled CSS matches the layout.
 *
 * A shot may instead drive the UI: `open` clicks a control, then the capture
 * is clipped to `panel`'s box grown by `pad` (top/right/bottom/left CSS px) —
 * used to frame an opened popover with a bit of the surrounding chrome.
 */
const SHOTS = [
  {
    file: 'library-desktop.png',
    title: 'Screens/Library',
    name: 'Desktop',
    width: 1440,
    height: 900,
    htmlMode: 'desktop',
  },
  {
    file: 'cloud-conflict.png',
    title: 'Modals/CloudConflictModal',
    name: 'Cloud newer',
    width: 1000,
    height: 760,
    selector: '[role="dialog"]',
  },
  {
    // The transfers panel shown where it lives: open the chrome's transfer
    // pill in the full library, then frame the panel plus a slice of the
    // window around it. Uploads resolve real covers from the library.
    file: 'transfers.png',
    title: 'Screens/Library',
    name: 'Desktop · transfers',
    width: 1440,
    height: 900,
    htmlMode: 'desktop',
    click: 'button[aria-label="Transfers"]',
    panel: '[role="dialog"][aria-label="Transfers"]',
    pad: { top: 58, right: 28, bottom: 52, left: 160 },
  },
  {
    // Settings → Saves (cloud sync) — click the nav group, then the full page.
    file: 'settings-cloud.png',
    title: 'Screens/Settings',
    name: 'Cloud configured',
    width: 1180,
    height: 820,
    htmlMode: 'desktop',
    click: 'nav button:has-text("Backups & cloud sync")',
  },
  {
    // Settings → Network (LAN sharing).
    file: 'settings-sharing.png',
    title: 'Screens/Settings',
    name: 'Default (Windows)',
    width: 1180,
    height: 820,
    htmlMode: 'desktop',
    click: 'nav button:has-text("LAN sharing")',
  },
  {
    // The Add Game flow with ranked save-match candidates.
    file: 'add-game.png',
    title: 'Screens/Add Game',
    name: 'Matches',
    width: 760,
    height: 700,
    htmlMode: 'desktop',
  },
];

const MIME = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.mjs': 'text/javascript',
  '.json': 'application/json',
  '.css': 'text/css',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.ttf': 'font/ttf',
  '.map': 'application/json',
  '.ico': 'image/x-icon',
};

function log(msg) {
  process.stdout.write(`${msg}\n`);
}

async function exists(p) {
  try {
    await stat(p);
    return true;
  } catch {
    return false;
  }
}

function buildStorybook() {
  log('▶ Building Storybook (storybook-static/)…');
  const res = spawnSync('bun', ['run', 'build-storybook'], {
    cwd: tauriDir,
    stdio: 'inherit',
  });
  if (res.status !== 0) {
    throw new Error('storybook build failed');
  }
}

/** A tiny static file server over storybook-static/ on an ephemeral port. */
function serveStatic() {
  const server = createServer(async (req, res) => {
    try {
      const urlPath = decodeURIComponent((req.url ?? '/').split('?')[0]);
      let filePath = join(staticDir, urlPath);
      if (urlPath.endsWith('/')) filePath = join(filePath, 'index.html');
      if (!filePath.startsWith(staticDir)) {
        res.writeHead(403).end();
        return;
      }
      if (!(await exists(filePath))) {
        res.writeHead(404).end('not found');
        return;
      }
      res.writeHead(200, { 'content-type': MIME[extname(filePath)] ?? 'application/octet-stream' });
      createReadStream(filePath).pipe(res);
    } catch {
      res.writeHead(500).end();
    }
  });
  return new Promise((res) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      res({ server, port });
    });
  });
}

/** Map "Title / Name" → story id from the built index. */
async function loadStoryIndex() {
  const index = JSON.parse(await readFile(join(staticDir, 'index.json'), 'utf8'));
  const byTitleName = new Map();
  for (const entry of Object.values(index.entries)) {
    if (entry.type !== 'story') continue;
    byTitleName.set(`${entry.title} / ${entry.name}`, entry.id);
  }
  return byTitleName;
}

async function waitForReady(page) {
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all(
      Array.from(document.images).map((img) =>
        img.complete && img.naturalWidth > 0
          ? null
          : new Promise((res) => {
              img.addEventListener('load', res, { once: true });
              img.addEventListener('error', res, { once: true });
            }),
      ),
    );
  });
}

async function capture(browser, port, index, shot) {
  const id = index.get(`${shot.title} / ${shot.name}`);
  if (!id) {
    throw new Error(`story not found in index: ${shot.title} / ${shot.name}`);
  }
  const context = await browser.newContext({
    viewport: { width: shot.width, height: shot.height },
    deviceScaleFactor: 2,
    colorScheme: 'dark',
  });
  const page = await context.newPage();
  const url = `http://127.0.0.1:${port}/iframe.html?id=${id}&viewMode=story`;
  await page.goto(url, { waitUntil: 'networkidle' });
  if (shot.htmlMode) {
    await page.evaluate((m) => (document.documentElement.dataset.mode = m), shot.htmlMode);
  }
  await waitForReady(page);
  await page.waitForTimeout(300); // let entry transitions settle

  const dest = join(outDir, shot.file);
  let logicalWidth = shot.width;
  let logicalHeight = shot.height;
  // Optional UI driving before capture (e.g. open a popover, switch a settings
  // tab). `click` accepts one selector or a list, applied in order.
  if (shot.click) {
    for (const sel of [].concat(shot.click)) {
      await page.locator(sel).first().click();
      await page.waitForTimeout(150);
    }
    await waitForReady(page); // any imagery the click revealed
  }
  if (shot.panel) {
    // Clip to the revealed panel grown by `pad` so the shot frames the panel
    // with a slice of the surrounding UI for context.
    const el = page.locator(shot.panel).first();
    await el.waitFor({ state: 'visible' });
    const box = await el.boundingBox();
    const pad = shot.pad ?? {};
    const x = Math.max(0, box.x - (pad.left ?? 0));
    const y = Math.max(0, box.y - (pad.top ?? 0));
    const right = Math.min(shot.width, box.x + box.width + (pad.right ?? 0));
    const bottom = Math.min(shot.height, box.y + box.height + (pad.bottom ?? 0));
    logicalWidth = Math.round(right - x);
    logicalHeight = Math.round(bottom - y);
    await page.screenshot({ path: dest, clip: { x, y, width: right - x, height: bottom - y } });
  } else if (shot.selector) {
    const el = page.locator(shot.selector).first();
    await el.waitFor({ state: 'visible' });
    const box = await el.boundingBox();
    if (box) {
      logicalWidth = Math.round(box.width);
      logicalHeight = Math.round(box.height);
    }
    await el.screenshot({ path: dest });
  } else {
    await page.screenshot({ path: dest });
  }
  await context.close();
  log(`  ✓ ${shot.file}  (${logicalWidth}×${logicalHeight})`);
  return { ...shot, width: logicalWidth, height: logicalHeight };
}

/**
 * Rewrite each `<!-- spool:shot id=NAME -->…<!-- spool:endshot -->` block in
 * the README with a fresh <img> pointing at the captured PNG. Blocks for shots
 * we didn't capture (or images with no marker, like the Decky shots) are left
 * alone. Returns the count updated.
 */
async function updateReadme(captured) {
  if (!(await exists(readmePath))) return 0;
  let md = await readFile(readmePath, 'utf8');
  const byId = new Map(captured.map((c) => [c.file.replace(/\.png$/, ''), c]));
  let updated = 0;
  md = md.replace(
    /<!-- spool:shot id=([\w-]+) -->[\s\S]*?<!-- spool:endshot -->/g,
    (whole, shotId) => {
      const c = byId.get(shotId);
      if (!c) return whole;
      updated++;
      const alt = shotId.replace(/-/g, ' ').replace(/\b\w/g, (ch) => ch.toUpperCase());
      // Emit width only (no height): rendering environments that apply
      // `max-width:100%` without `height:auto` (e.g. VS Code's markdown
      // preview) distort a fixed-height image when the pane is narrower than
      // the width. With width alone, height stays proportional everywhere.
      return (
        `<!-- spool:shot id=${shotId} -->\n` +
        `<img src="docs/screenshots/${c.file}" alt="${alt}" width="${c.width}" />\n` +
        `<!-- spool:endshot -->`
      );
    },
  );
  await writeFile(readmePath, md);
  return updated;
}

async function main() {
  if (!skipBuild) {
    buildStorybook();
  } else if (!(await exists(join(staticDir, 'index.json')))) {
    throw new Error('--no-build given but storybook-static/ is missing; run without --no-build first');
  }

  await mkdir(outDir, { recursive: true });
  const index = await loadStoryIndex();
  const { server, port } = await serveStatic();
  const browser = await chromium.launch();

  log('▶ Capturing stories…');
  const captured = [];
  try {
    for (const shot of SHOTS) {
      captured.push(await capture(browser, port, index, shot));
    }
  } finally {
    await browser.close();
    server.close();
  }

  if (!skipReadme) {
    const n = await updateReadme(captured);
    log(`▶ README: updated ${n} screenshot block(s).`);
  }

  // Mirror the captures into the docs site so its guides can embed them.
  await mkdir(docsAssetsDir, { recursive: true });
  for (const c of captured) {
    await copyFile(join(outDir, c.file), join(docsAssetsDir, c.file));
  }
  log(`▶ docs-site: mirrored ${captured.length} image(s) to src/assets/screenshots/.`);
  log(`✓ Done. PNGs in docs/screenshots/`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
