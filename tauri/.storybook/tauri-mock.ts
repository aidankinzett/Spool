/**
 * Tauri IPC mock helpers for screen-level stories.
 *
 * `installTauriMock` wires `mockIPC` (with event support so `listen`/`emit`
 * work) and `mockConvertFileSrc`, layering per-story command handlers over a
 * set of sensible defaults. Used via `tauriDecorator(...)` in a story's
 * `decorators`, configured through `parameters.tauri`.
 */
import { mockIPC, mockConvertFileSrc, mockWindows } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import { makeConfig, makeGame, makePlaySessions, SAMPLE_PEER_GAMES } from './fixtures';
import TauriMockDecorator from './TauriMockDecorator.svelte';

/**
 * A command handler is either a canned value or a function of the invoke args.
 * Functions may return a Promise (e.g. a never-resolving one to freeze a
 * loading state) or throw to simulate a backend error.
 */
export type TauriHandler = unknown | ((args: Record<string, unknown>) => unknown);
export type TauriHandlers = Record<string, TauriHandler>;

/** Commands every screen tends to touch, so a page never errors on mount. */
function defaultHandlers(): TauriHandlers {
  return {
    app_platform: 'windows',
    list_games: [makeGame()],
    get_config: makeConfig(),
    refresh_save_metadata: undefined,
    // Fills the cross-device activity card in any screen story with a detail
    // pane. Keyed off the requested game name so each game's chart is consistent.
    list_play_sessions: (args: Record<string, unknown>) =>
      makePlaySessions(String(args.gameName ?? '')),
    get_run_as_admin_in_registry: false,
    list_proton_versions: [],
    check_dependencies: [],
    list_lan_peers: [],
    // The merged sidebar (and the peer-drill-down popover) fetch each browsable
    // peer's catalogue. Only called when a story actually supplies peers via
    // `list_lan_peers`, so this is inert in peerless stories.
    fetch_peer_games: SAMPLE_PEER_GAMES,
    list_active_uploads: [],
    take_pending_run: null,
    decky_plugin_status: { supported: false },
    current_sync_status: {
      reachability: 'unconfigured',
      server_version: null,
      error: null,
      last_ok_ago_secs: null,
    },
    notify_splash_ready: undefined,
    // getVersion() from @tauri-apps/api/app.
    'plugin:app|version': '5.0.0',
  };
}

/**
 * `mockConvertFileSrc` `encodeURIComponent`s every path into a dead
 * `asset.localhost` URL — fine for local filesystem paths (the covers simply
 * don't load), but it also mangles fixtures that point a cover/hero at a real
 * `http(s)`/`data:` URL. Wrap the installed mock so absolute URLs round-trip
 * unchanged, letting the Library screen story render real cover imagery for
 * documentation screenshots while every other story keeps the placeholder look.
 */
function passThroughRealUrls(): void {
  const internals = (globalThis as unknown as {
    __TAURI_INTERNALS__: { convertFileSrc: (p: string, protocol?: string) => string };
  }).__TAURI_INTERNALS__;
  const mocked = internals.convertFileSrc;
  internals.convertFileSrc = (path, protocol) =>
    /^(https?|data|blob):/i.test(path) ? path : mocked(path, protocol);
}

function resolve(handler: TauriHandler, args: Record<string, unknown>): unknown {
  return typeof handler === 'function'
    ? (handler as (a: Record<string, unknown>) => unknown)(args)
    : handler;
}

/**
 * Install the IPC mock for a story. Per-story `handlers` win over defaults;
 * unknown commands resolve to `undefined` (fine for fire-and-forget calls like
 * window close / dialog open that the story doesn't care about).
 */
export function installTauriMock(handlers: TauriHandlers = {}): void {
  const merged = { ...defaultHandlers(), ...handlers };
  mockConvertFileSrc('windows');
  passThroughRealUrls();
  // So getCurrentWindow() (used by WindowChrome via AppChrome) resolves a label
  // instead of throwing on undefined metadata.
  mockWindows('main');
  mockIPC(
    (cmd, args) => {
      // Own-property check so a command literally named `toString`/`constructor`
      // can't false-match an inherited Object.prototype member.
      if (Object.prototype.hasOwnProperty.call(merged, cmd))
        return resolve(merged[cmd], (args ?? {}) as Record<string, unknown>);
      return undefined;
    },
    { shouldMockEvents: true },
  );
}

/** Re-export so story harnesses can push `run:phase` etc. into a mounted page. */
export { emit as emitTauriEvent };

/**
 * Build a Storybook decorator that installs the mock from `parameters.tauri`
 * (a `TauriHandlers` map), merged with any handlers passed here directly. The
 * mock is set up in TauriMockDecorator's init, before the wrapped page mounts.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function tauriDecorator(base: TauriHandlers = {}): any {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (story: any, ctx: any) => {
    const fromParams = (ctx?.parameters?.tauri ?? {}) as TauriHandlers;
    return {
      Component: TauriMockDecorator,
      props: { handlers: { ...base, ...fromParams }, children: story },
    };
  };
}
