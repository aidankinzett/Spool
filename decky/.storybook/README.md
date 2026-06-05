# Storybook for the Spool Decky plugin

This renders the plugin's React components **outside Steam**, so you can iterate
on UI without the deploy-to-Deck-and-restart loop (`pnpm deploy` over SSH +
`plugin_loader` restart).

```bash
cd decky
pnpm storybook        # dev server on http://localhost:6006
pnpm build-storybook  # static build (also what CI/sanity-checks run)
```

## Why mocks are needed

Plugin components import from three things that only exist inside Steam's CEF
runtime:

- **`@decky/ui`** — `Focusable`, `ButtonItem`, `ModalRoot`, `ToggleField`, the
  context-menu primitives, the `appDetailsClasses` minified-CSS objects, etc.
  The real ones resolve to Steam's *internal* React components at runtime, so
  they return `undefined` outside Steam and can't be imported as-is.
- **`@decky/api`** — `callable` (RPC to the Python backend) and `toaster`.
- **Ambient globals** — `SteamClient`, Steam's router (`ReactRouter`).

`.storybook/main.ts` aliases `@decky/ui` and `@decky/api` to local DOM shims in
`.storybook/mocks/`. React/ReactDOM and `react-icons` stay real.

## The mocks

| File | What it provides |
|---|---|
| `mocks/decky-ui.tsx` | DOM shims for every `@decky/ui` export the plugin uses. Includes interactive `showModal`/`showContextMenu` (mount via `react-dom`), and a `useParams` that `lib/steam`'s resolver picks up — set its value with `setRouteParams(...)`. |
| `mocks/decky-api.tsx` | `callable(name)` → dispatches through the registry; `toaster.toast` → `console.log`. |
| `mocks/registry.ts` | `setCallable(name, fn)` / `clearCallables()` — the handler table `callable` dispatches to. |
| `mocks/fixtures.ts` | `makeGame()`, LAN fixtures (`PEERS`, `PEER_GAMES`, `makeDownload`), `registerDeckyCallables()`, and `installFetchMock()`. |

## Patterns for writing a story

### 1. Pure presentational component
Just pass props. (e.g. `SpoolMark`, `Reel`, `CoverGrid`, `SpoolBar`.)

```tsx
export const Default: StoryObj<typeof SpoolMark> = {
  args: { size: 64, color: "#f4f4f5", tape: "#d7c9a0" },
};
```

### 2. Component that calls a backend `callable`
Register handlers keyed by the **backend name string** (the argument passed to
`callable(...)` in `src/api/callables.ts`), then render.

```tsx
import { setCallable, clearCallables } from "../../.storybook/mocks/registry";

render: () => {
  clearCallables();
  setCallable("list_proton_versions", async () => VERSIONS);
  setCallable("set_proton_version", async () => ({ ok: true }));
  return <ProtonVersionModal game={makeGame()} closeModal={() => {}} />;
};
```

`registerDeckyCallables()` in `fixtures.ts` registers benign defaults for the
whole set at once — use it, then override individual handlers as a story needs
(e.g. to make a save fail).

### 3. Component that hits the loopback HTTP server
These call `fetch(\`${base}/...\`)` after `useServerBase` resolves the base URL.
Register the `get_server_base` callable (via `registerDeckyCallables`) and stub
`window.fetch` with `installFetchMock`, which routes by URL **substring** (first
match wins — order matters when one URL contains another):

```tsx
registerDeckyCallables();              // get_server_base → MOCK_BASE
installFetchMock({
  "/games": PEER_GAMES,                // must precede "/lan/peers": the game-list
  "/lan/download": null,               //   URL contains both substrings
  "/lan/peers": PEERS,
});
```

Pass `registerDeckyCallables({ serverRunning: false })` to exercise the
"Spool isn't running" state.

### 4. Route-param component (LAN peer pages)
`lib/steam.useParams` reads params Steam's router would supply. Set them before
render:

```tsx
import { setRouteParams } from "../../.storybook/mocks/decky-ui";
setRouteParams({ peerAddr: "192.168.1.20", peerPort: "47632", gameId: "pg1" });
```

## Fidelity — what this is and isn't

A **layout / prop / state harness**, not a visual oracle.

- **High fidelity**: self-contained DOM/SVG with hardcoded styling — `SpoolMark`,
  `Reel`, `TapeMeter`, `SpoolBar`, the LAN page layouts.
- **Metrically matched to Game Mode**: `DialogButton` and `ButtonItem` use real
  values read off live Game Mode via the CEF debugger — `DialogButton.Secondary`
  is `#23262e`, `padding 10px 24px`, `min-width 160px`, `2px` radius, `0.4`
  opacity disabled; the inline `ButtonItem` `Field` row is `flex`, `padding
  10px 16px`, `gap 12px`, label `flex 1` in muted `#67707b @16px`, button
  `flex 0 0 auto`. Focus rings and hover transitions still differ from Steam.
- **Structurally faithful, metrically approximate**: the remaining `@decky/ui`
  shims (`ToggleField`, the context-menu primitives). Right structure and
  behaviour; exact metrics not yet matched against the debugger.
- **The font** is Open Sans, a free near-match for Steam's proprietary Motiva
  Sans (`preview-head.html` + `preview.ts`). Close, not identical.
- **Not renderable**: `patch/patch-wrapper.tsx` — it injects into Steam's live
  app-detail DOM (`findTopCapsule`, ResizeObserver/MutationObserver on Steam's
  capsule) and returns `null` without it. Its *children* (`SpoolBar`, `Reel`,
  `BadgeMenuButton`) have stories; the wrapper itself only works in Game Mode.
- **LAN cover art** won't load — thumbnails are `<img src>` pointing at peer
  HTTP URLs the component builds itself; the grid/layout still reads correctly.

## Deck CEF debugger (for matching real styles)

Steam exposes a Chrome DevTools Protocol endpoint in Game Mode (port 8081) when
remote debugging is enabled. Use it to inspect the live Game-Mode DOM and read
the real `Field`/`ButtonItem` computed styles, or to resolve minified SteamUI
class names via `webpackChunksteamui`. That's the reference for tightening the
`@decky/ui` shims here.
