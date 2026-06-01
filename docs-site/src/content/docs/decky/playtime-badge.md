---
title: Playtime badge
description: Injecting Spool's cross-device playtime onto Steam's own game pages.
sidebar:
  order: 6
---

The plugin injects a small badge onto Steam's native game-detail page (`/library/app/:appid`) showing Spool's tracked playtime and last-played for that game, when Spool has a match for it. This surfaces Spool's cross-device playtime where you'd expect to see playtime — on the game's page in your Steam library — rather than only inside the plugin's own UI.

## Patching the Steam page

Steam's game-detail component is rendered by Steam, not the plugin, so there's no prop to pass a badge into. At plugin load `src/index.tsx` registers a route patch:

```ts
routerHook.addPatch("/library/app/:appid", (tree) => { … });
```

The patch uses `afterPatch` + `findInReactTree` to walk the rendered React tree, find the `appDetailsClasses.InnerContainer`, and splice a `PlaytimePatchWrapper` element into its children — the same tree-splicing approach used by other Decky plugins such as protondb-decky. Wrapping `props.children` doesn't work here because the game-detail component ignores its children. The patch is removed on dismount to avoid duplicate patches across hot-reloads.

## Reading the appid

`PlaytimePatchWrapper` reads the appid with `useParams<{ appid }>()` — Steam's **internal** memory-based router, not `window.location`. In Steam's CEF context `window.location.pathname` is always `/index.html`, so the route param has to come from Steam's router (`src/lib/steam.ts` extracts the `useParams` hook out of `ReactRouter`). It renders nothing for a falsy appid, otherwise mounts `SpoolPlaytimeBadge`.

## Fetching the data

`SpoolPlaytimeBadge` resolves the server base (`useServerBase`) and calls the `useSpoolPlaytime(appid, base)` hook:

1. `GET ${base}/library` immediately, so the badge appears fast, matching the appid to a library entry via `findSpoolGame` (by `steam_id`, `shortcut_app_id`, or the localStorage inverse map — see [Library & launching](./library-and-launch)).
2. `POST ${base}/fold` in the background to run a cross-device rclone fold, then re-fetch `/library` so playtime and last-played reflect every device's contribution — without needing the full Spool GUI running.

The badge renders only when a match is found with `playtime_minutes > 0`, showing something like:

```
💾 4h 12m played · Last played 3d ago
```

(durations and relative times formatted by `src/lib/format.ts`). When the server isn't running, or no Spool game matches the appid, nothing is injected and Steam's page is untouched.
