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
import { makeConfig, makeGame } from './fixtures';
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
    get_run_as_admin_in_registry: false,
    list_proton_versions: [],
    check_dependencies: [],
    list_lan_peers: [],
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
  // So getCurrentWindow() (used by WindowChrome via AppChrome) resolves a label
  // instead of throwing on undefined metadata.
  mockWindows('main');
  mockIPC(
    (cmd, args) => {
      if (cmd in merged) return resolve(merged[cmd], (args ?? {}) as Record<string, unknown>);
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
