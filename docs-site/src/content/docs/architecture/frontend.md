---
title: SvelteKit frontend
description: Routes and shared lib/ code for tauri/src/ — a pure view onto the Rust backend.
sidebar:
  order: 3
---

The frontend lives in `tauri/src/`. It's purely a view onto the backend state —
every file IO, subprocess call, and HTTP request lives in Rust, reached through
a single typed IPC wrapper.

## Routes

Under `tauri/src/routes/`:

- **`+layout.svelte` / `+layout.ts`** — global chrome (frameless title bar,
  toast stack), dark-theme/density application, cross-window event
  subscriptions, navigation shell.
- **`+page.svelte`** — the main library window. Picks `LibraryDesktop.svelte`
  or `LibraryTouch.svelte` based on the resolved UI mode, and renders the
  `CloudConflictModal` on a save conflict.
- **`add/+page.svelte`** — the Add Game flow (drop/browse an exe → ludusavi
  auto-identifies → ranked candidate list).
- **`edit/+page.svelte`** — the per-game editor (identity, install folder,
  launch settings, LAN sharing, cover refresh, remove).
- **`splash/+page.svelte`** — the full-screen Game-Mode launch splash showing
  `run:phase` progress.
- **`settings/+page.svelte`** — application settings in a two-pane layout
  (Display, Library, Sharing & Sync). Live save on commit, no Save button.

## Shared code

Under `tauri/src/lib/`:

- **`api.ts`** — the single typed wrapper around Tauri's `invoke` IPC bridge.
  Every backend command is a method on the exported `api` object. Components
  never call `invoke` directly. Also exports `assetUrl()` for loading local
  files into the webview.
  :::tip[When you add a Rust command]
  Add its typed wrapper here so the frontend can reach it.
  :::
- **`types.ts`** — TypeScript mirrors of the Rust serde types (`GameEntry`,
  `ConfigData`, `LanPeer`, etc.). Keep these in sync with the Rust structs.
- **`uiMode.svelte.ts`** — single source of truth for UI density.
  `auto | desktop | touch` resolves to `desktop | touch`; auto-detection uses
  `matchMedia('(pointer: coarse)')`. Writes `<html data-mode>` so CSS scales
  targets/spacing.
- **`components/`** — reusable Svelte components: primitives (`Btn`, `Toggle`,
  `TextField`, …), the two library layouts, the toast stack, the LAN transfer
  UI, `GameDetail`, `CloudConflictModal`, and more.
- **`toasts.svelte.ts` / `library.svelte.ts`** — global stores (Svelte 5 runes).
- **`format.ts` / `tokens.ts` / `nav.ts` / `updater.ts`** — display helpers,
  design tokens, navigation, and the updater wrapper.

## UI modes

Spool adapts between a **desktop** layout (sidebar list + detail panel, child
windows for Add/Edit/Settings) and a big-target **touch** layout (shelf of
large tiles, overlays instead of child windows) for handhelds like the Steam
Deck or ROG Ally. The UI is dark-only — there is no theme switcher.

## Talking to the backend

The frontend and backend communicate two ways:

- **Commands** — the frontend calls `api.someCommand(...)`, which invokes a Rust
  `#[tauri::command]` and awaits a typed result.
- **Events** — the backend broadcasts colon-namespaced events
  (`library:changed`, `run:phase`, `lan:peers-changed`, …) via
  `AppHandle::emit` to all open webviews.
