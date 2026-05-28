import { spawn, type ChildProcess } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import type { Options } from '@wdio/types';

const here = dirname(fileURLToPath(import.meta.url));

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
