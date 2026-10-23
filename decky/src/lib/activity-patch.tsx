import { createReactTreePatcher, findInReactTree } from "@decky/ui";
import { useContext, type ComponentType, type Context, type ReactElement, type ReactNode } from "react";
import { SpoolActivity, SpoolSummary } from "../components/activity/spool-activity";

// Steam's id for the game page's "Activity" tab.
const ACTIVITY_TAB_ID = "WhatsNew";

type Tab = { id: string; title: string; content: ReactElement };
type TabsComponent = ComponentType<any>;

let hasWarned = false;
function warnOnce(...args: unknown[]) {
  if (hasWarned) return;
  console.warn("[Spool]", ...args);
  hasWarned = true;
}

// Adds Spool's post-game summary above the game page's tab bar, and its
// activity timeline to the top of the Activity tab.
//
// The game page renders its tabs through a component it reads from a React
// context (a Provider above the page sets it). The component tree between the
// page and that consumer includes a class whose `render` is redefined on the
// instance after the first call, so a tree patch through it only sees the
// first render. Instead, the page's output is wrapped in a nearer Provider for
// the same context whose value is the original tabs component with its output
// patched.
//
// Under the tabs component, the component holding `fnOnCancelFromTabHeader`
// builds the tab list and renders the tab strip (`tabs: [{ id, title,
// content }]`); the Activity entry's content gets Spool's section prepended.

// The context is found by its default value: a fallback tabs component taking
// `setSections` and `concise`. Matching on that, not the module id or export
// name, survives Steam rebuilds.
//
// The module defining it is loaded after Decky indexes Steam's modules at
// startup, so `findModuleExport` (which searches that index) misses it. This
// searches webpack's module factories directly, requiring only the few whose
// source mentions the same names.
//
// The default is read from `_currentValue2`: this runs while the page renders,
// inside Steam's Provider, and React DOM swaps the Provider's value into
// `_currentValue` for that time. `_currentValue2` belongs to secondary
// renderers, which Steam doesn't use, so it keeps the default.
function isTabsContext(e: any): e is Context<TabsComponent> {
  const fallback = e?._currentValue2;
  return (
    e?.$$typeof === Symbol.for("react.context") &&
    typeof fallback === "function" &&
    /setSections/.test(`${fallback}`) &&
    /concise/.test(`${fallback}`)
  );
}

let tabsContext: Context<TabsComponent> | null = null;
// Module count at the last failed search; the next search waits until Steam
// has loaded more modules.
let searchedModuleCount = -1;
function findTabsContext(): Context<TabsComponent> | null {
  if (tabsContext) return tabsContext;
  try {
    let req: any;
    window.webpackChunksteamui.push([[Symbol("spool")], {}, (r: any) => (req = r)]);
    const ids = Object.keys(req?.m ?? {});
    if (ids.length === searchedModuleCount) return null;
    searchedModuleCount = ids.length;
    for (const id of ids) {
      const src = `${req.m[id]}`;
      if (!src.includes("createContext(") || !src.includes("concise") || !src.includes("setSections")) continue;
      const found = Object.values(req(id) ?? {}).find(isTabsContext);
      if (found) return (tabsContext = found);
    }
  } catch (err) {
    warnOnce("Searching for the game page tabs context failed:", err);
    return null;
  }
  warnOnce("Game page tabs context not found; Spool activity disabled.");
  return null;
}

const tabsPatcher = createReactTreePatcher(
  [(t: any) => findInReactTree(t, (x: any) => x?.props?.fnOnCancelFromTabHeader)],
  (args: Array<Record<string, any>>, ret?: ReactElement) => {
    try {
      const strip = findInReactTree(
        ret,
        (x: any) =>
          Array.isArray(x?.props?.tabs) &&
          x.props.tabs.some((t: Tab) => t?.id === ACTIVITY_TAB_ID),
      );
      if (!strip) {
        warnOnce("Activity tab not found on the game page; Spool activity disabled.");
        return ret;
      }
      const appid = (args[0]?.overview?.appid ?? 0) >>> 0;
      // `tabs` is Steam's memoized array, so replace it with a copy rather
      // than mutating it.
      strip.props.tabs = strip.props.tabs.map((t: Tab) =>
        t?.id === ACTIVITY_TAB_ID
          ? {
              ...t,
              content: (
                <>
                  <SpoolActivity appid={appid} />
                  {t.content}
                </>
              ),
            }
          : t,
      );
    } catch (err) {
      warnOnce("Failed to patch the Activity tab:", err);
    }
    return ret;
  },
  "SpoolActivityTab",
);

// Puts the post-game summary between the Play row and the tab bar: into the
// element whose children hold both, just before the child the tab bar is in.
// It's inserted on every render (rendering nothing when there's no summary) so
// the tab bar's position among its siblings never changes, which would
// remount it.
function insertSummary(ret: ReactElement | undefined, appid: number) {
  try {
    const hasTabs = (n: any) => !!findInReactTree(n, (x: any) => x?.props?.fnOnCancelFromTabHeader);
    const parent = findInReactTree(
      ret,
      (x: any) => Array.isArray(x?.props?.children) && x.props.children.some(hasTabs),
    );
    if (!parent) {
      warnOnce("Tab bar not found on the game page; post-game summary disabled.");
      return;
    }
    const at = parent.props.children.findIndex(hasTabs);
    parent.props.children.splice(at, 0, <SpoolSummary key="spool-summary" appid={appid} />);
  } catch (err) {
    warnOnce("Failed to add the post-game summary:", err);
  }
}

// One wrapper per original component, so the Provider's value keeps the same
// identity across renders and React doesn't remount the tabs.
const wrapped = new WeakMap<TabsComponent, TabsComponent>();
function wrapTabs(Orig: TabsComponent): TabsComponent {
  // Only a plain function component can be called inline like this.
  if (typeof Orig !== "function" || Orig.prototype?.render) return Orig;
  let w = wrapped.get(Orig);
  if (!w) {
    const render = Orig as (props: any) => ReactElement;
    w = function SpoolAppDetailsTabs(props: any) {
      const ret = tabsPatcher([props], render(props));
      insertSummary(ret, (props?.overview?.appid ?? 0) >>> 0);
      return ret;
    };
    wrapped.set(Orig, w);
  }
  return w;
}

// Reads the tabs component the page would get, and provides the wrapped one to
// everything below.
function ActivityTabsProvider({
  context,
  children,
}: {
  context: Context<TabsComponent>;
  children: ReactNode;
}) {
  const Orig = useContext(context);
  return <context.Provider value={wrapTabs(Orig)}>{children}</context.Provider>;
}

// Called from the game-page patch with the page component's output.
export function wrapActivityTabs(ret: ReactElement | undefined): ReactElement | undefined {
  const context = findTabsContext();
  if (!context || !ret) return ret;
  return <ActivityTabsProvider context={context}>{ret}</ActivityTabsProvider>;
}
