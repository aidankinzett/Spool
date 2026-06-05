// Shared callable registry for the Storybook mocks.
//
// Outside Steam there is no Decky Loader and no Python backend, so `callable`
// (from the @decky/api mock) can't do real RPC. Instead each `callable(name)`
// resolves through this registry: a story registers a handler keyed by the
// same backend name string the component's callable uses (e.g.
// "list_proton_versions"), and the mock invokes it. Unregistered names resolve
// to `undefined` with a console warning so a missing fixture is obvious.

type Handler = (...args: any[]) => unknown | Promise<unknown>;

const handlers = new Map<string, Handler>();

export function setCallable(name: string, fn: Handler): void {
  handlers.set(name, fn);
}

export function clearCallables(): void {
  handlers.clear();
}

export function getHandler(name: string): Handler | undefined {
  return handlers.get(name);
}
