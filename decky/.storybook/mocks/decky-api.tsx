// Storybook mock for "@decky/api".
//
// `callable(name)` returns an async function that dispatches through the
// shared registry (see registry.ts). `toaster.toast` logs to the console
// instead of showing Steam's native toast.
import { getHandler } from "./registry";

export function callable<Args extends unknown[] = unknown[], Ret = unknown>(
  name: string,
): (...args: Args) => Promise<Ret> {
  return async (...args: Args): Promise<Ret> => {
    const handler = getHandler(name);
    if (!handler) {
      console.warn(
        `[decky-api mock] no handler registered for callable "${name}" — returning undefined. Register one with setCallable("${name}", ...).`,
      );
      return undefined as Ret;
    }
    return (await handler(...args)) as Ret;
  };
}

export const toaster = {
  toast: (t: { title?: string; body?: string }) => {
    console.log(`[toast] ${t.title ?? ""}: ${t.body ?? ""}`);
  },
};

// Other @decky/api exports the plugin doesn't use yet, stubbed so any future
// import resolves rather than throwing.
export const definePlugin = (fn: unknown) => fn;
export const routerHook = {
  addRoute: () => {},
  removeRoute: () => {},
  addGlobalComponent: () => {},
  removeGlobalComponent: () => {},
};
export const call = async () => undefined;
