import { spawn, type ChildProcess } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import type { Options } from '@wdio/types';
import { SEED_GAMES } from './e2e/fixtures/library.js';

const here = dirname(fileURLToPath(import.meta.url));

// Isolate the app's data directory so seeded fixtures are deterministic and we
// never touch a real user's library. On Linux, dirs::data_local_dir() honours
// XDG_DATA_HOME, so the app reads <XDG_DATA_HOME>/Spool/library.json. Set at
// module top-level so both the launcher process and forked workers (which spawn
// the app) inherit it. (Windows ignores XDG_DATA_HOME — see the seeded spec,
// which skips off-Linux.)
const e2eDataHome = join(tmpdir(), 'spool-e2e-data');
process.env.XDG_DATA_HOME = e2eDataHome;
const spoolDataDir = join(e2eDataHome, 'Spool');

// The production binary produced by `bun run tauri build --no-bundle`.
// On Linux this is an unbundled ELF; on Windows it's `spool.exe`.
const binary = resolve(
  here,
  'src-tauri/target/release',
  process.platform === 'win32' ? 'spool.exe' : 'spool',
);

// tauri-driver proxies the W3C WebDriver protocol to the platform webdriver
// (WebKitWebDriver on Linux, msedgedriver on Windows). We manage its lifecycle
// manually rather than via a WDIO service so the harness stays dependency-light.
let tauriDriver: ChildProcess | undefined;

export const config: Options.Testrunner = {
  runner: 'local',
  hostname: '127.0.0.1',
  port: 4444,
  path: '/',

  specs: ['./e2e/specs/**/*.e2e.ts'],

  // Tauri exposes a single window per process, so one session at a time.
  maxInstances: 1,

  capabilities: [
    {
      browserName: 'wry',
      // tauri-driver speaks classic W3C WebDriver only; WDIO v9 defaults to
      // BiDi (webSocketUrl), which the driver rejects with "failed to match
      // capabilities". Force classic to get a session.
      'wdio:enforceWebDriverClassic': true,
      // @ts-expect-error tauri:options is a tauri-driver extension capability.
      'tauri:options': {
        application: binary,
      },
    },
  ],

  logLevel: 'warn',
  bail: 0,
  waitforTimeout: 10_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 3,

  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 60_000,
  },

  // Seed a fresh, isolated library.json before any app launches. Runs once in
  // the launcher process; the file persists on disk for every worker session.
  onPrepare: () => {
    rmSync(spoolDataDir, { recursive: true, force: true });
    mkdirSync(spoolDataDir, { recursive: true });
    writeFileSync(
      join(spoolDataDir, 'library.json'),
      JSON.stringify(SEED_GAMES, null, 2),
    );
    // Seed a config marking onboarding complete. Without a config.json the
    // app treats this as a genuine fresh install and shows the first-run
    // onboarding modal over the library, whose overlay intercepts clicks on
    // the seeded game rows.
    writeFileSync(
      join(spoolDataDir, 'config.json'),
      JSON.stringify({ onboarding_completed: true }, null, 2),
    );
  },

  onComplete: () => {
    rmSync(e2eDataHome, { recursive: true, force: true });
  },

  // Start tauri-driver before the session and tear it down after. tauri-driver
  // listens on :4444 and itself spawns the native webdriver on :4445.
  beforeSession: () => {
    tauriDriver = spawn('tauri-driver', ['--port', '4444'], {
      stdio: [null, process.stdout, process.stderr],
    });
  },

  afterSession: () => {
    tauriDriver?.kill();
  },
};
