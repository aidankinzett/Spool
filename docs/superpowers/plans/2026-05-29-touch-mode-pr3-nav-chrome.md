# Touch Mode PR 3 — Navigation Abstraction + Chrome Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace inline `WebviewWindow` spawns with a mode-aware `openView()` helper (touch routes in-app, desktop spawns windows), add `TouchTopBar.svelte` (the handheld chrome), and wrap all pages in `AppChrome.svelte` that picks the right chrome for the resolved mode.

**Architecture:** `lib/nav.ts` centralises all navigation decisions: in touch mode it calls SvelteKit's `goto()`; in desktop mode it spawns the existing `WebviewWindow` with the same configs as today. `TouchTopBar.svelte` ports the prototype's `TopBar` (sync pill, peers, battery, clock, back button) sized by PR 1 density tokens. `AppChrome.svelte` is a thin conditional wrapper — `uiMode.resolved === 'touch'` → `TouchTopBar`, else → `WindowChrome`. All five route pages swap `<WindowChrome>` → `<AppChrome>`. This PR also resolves the cross-PR debt: the `fixed top-[44px]` offsets on the transfers panel and LAN popover are updated to track `var(--chrome-h)` so they don't overlap the 60px touch chrome.

**Tech Stack:** SvelteKit 5 (runes), Tauri 2 (`WebviewWindow`, `getCurrentWindow`), TypeScript, Tailwind v4, Lucide Svelte icons. Verified with `bun run check`, `bun run lint`, and manual test of touch navigation + desktop window spawning.

This is PR 3 of the 6-PR rollout in `design_handoff_touch_mode/Touch Mode - Refactor Plan.md` (§4–5). It depends on PRs 1 and 2, which are both committed on branch `touch-mode-pr1-density-tokens`. The working branch for this PR is `touch-mode-pr3-nav-chrome`.

---

## Design decisions locked in before writing this plan

- **`nav.ts` label names** — desktop `WebviewWindow` labels become `'settings'`, `'add'`, `'browse'`, `'edit'` (matching view names). The current labels (`'settings'`, `'browse-games'`, `'add-game'`, `'edit-game'`) are replaced; no existing windows need to be found by the old labels at runtime since `nav.ts` is the single registration point going forward.
- **`AppChrome` optional touch props** — `peers`, `transfers`, `conflict` default to `0`/`0`/`false`. Non-library pages (settings, add, browse, edit) don't pass them; the library page passes live state. This avoids lifting state into a shared store (PR 4's `library.svelte.ts` will do that properly).
- **Battery API** — `navigator.getBattery()` is Chromium-specific; used with optional chaining + `.catch(() => {})` so it degrades silently on Linux/WebKitGTK.
- **Back button on sub-pages** — each sub-page (settings, add, browse, edit) passes `onback={() => history.back()}` to `AppChrome`. Library root does not → no back button shown. Depth is always library → one sub-view; `history.back()` is sufficient.
- **`conflict` signal** — `syncOff` (server configured but unreachable) drives the amber alert state in `TouchTopBar`. Save-conflict detection (another device playing) is PR 6 scope.
- **Maximize on touch** — `+layout.svelte` calls `getCurrentWindow().maximize()` after `uiMode.init()` resolves to `'touch'`. Only the main window exists in touch mode (sub-views route in-app via `goto()`), so every window that resolves touch maximizes — which is always the main window.
- **`TransfersPanel` / LAN popover debt** — `fixed top-[44px]` at `+page.svelte:689,709` is updated to `style:top="var(--chrome-h)"` in this PR so the panels don't overlap the 60px touch chrome.

## Out of scope for PR 3

- `library.svelte.ts` controller extraction — PR 4.
- `LibraryTouch` layout — PR 5.
- Cross-window real-time mode propagation — dissolved by this PR (touch has one window; desktop multi-window is an acceptable limitation until PR 5).
- `ConfirmSheet`, in-game overlay, onboarding — PR 6.

## File map

| File | Change |
|---|---|
| `tauri/src/lib/nav.ts` | **Create** — `openView(view, params?)` helper |
| `tauri/src/lib/components/TouchTopBar.svelte` | **Create** — touch chrome strip (port of prototype `TopBar`) |
| `tauri/src/lib/components/AppChrome.svelte` | **Create** — conditional wrapper: `TouchTopBar` or `WindowChrome` |
| `tauri/src/routes/+page.svelte` | **Modify** — remove 3 inline window functions; add `openView` calls; fix `top-[44px]`×2; swap `<WindowChrome>` → `<AppChrome>` with live state props |
| `tauri/src/lib/components/GameDetail.svelte` | **Modify** — remove `openEditGame()`; replace with `openView('edit', { id: game.id })` |
| `tauri/src/routes/settings/+page.svelte` | **Modify** — swap `<WindowChrome>` → `<AppChrome onback={...}>` |
| `tauri/src/routes/add/+page.svelte` | **Modify** — swap `<WindowChrome>` → `<AppChrome onback={...}>` |
| `tauri/src/routes/browse/+page.svelte` | **Modify** — swap `<WindowChrome>` → `<AppChrome onback={...}>` |
| `tauri/src/routes/edit/+page.svelte` | **Modify** — swap `<WindowChrome>` → `<AppChrome onback={...}>` |
| `tauri/src/routes/+layout.svelte` | **Modify** — add `maximize()` call when touch resolves |

All `bun` commands run from `tauri/`, all `cargo` commands from `tauri/src-tauri/`.

---

## Task 0: Confirm baseline (prep — no subagent needed)

- [ ] **Step 1: Confirm branch and clean tree**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git branch --show-current   # expect: touch-mode-pr3-nav-chrome
git status --short           # noise: M tauri/src-tauri/Cargo.toml (CRLF), untracked .serena/ and design_handoff_touch_mode/
```

- [ ] **Step 2: Confirm checks are green**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri\src-tauri && cargo check
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: all pass. Stop if they don't — that's a pre-existing issue.

---

## Task 1: Create `lib/nav.ts`

**Files:**
- Create: `tauri/src/lib/nav.ts`

- [ ] **Step 1: Write the file**

Create `tauri/src/lib/nav.ts` with exactly:

```ts
// Centralised navigation helper — picks window-spawn (desktop) or in-app
// routing (touch) based on the resolved UI mode. Caller doesn't need to
// know which strategy is active.
import { goto } from '$app/navigation';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { uiMode } from './uiMode.svelte';

type View = 'settings' | 'add' | 'browse' | 'edit';

const WINDOWS: Record<View, {
  url: string; title: string;
  width: number; height: number;
  minWidth: number; minHeight: number;
}> = {
  settings: { url: '/settings', title: 'Spool — Settings', width: 1180, height: 760, minWidth: 900,  minHeight: 600 },
  add:      { url: '/add',      title: 'Add Game · Spool', width: 720,  height: 560, minWidth: 600,  minHeight: 480 },
  browse:   { url: '/browse',   title: 'Browse Games',     width: 1280, height: 800, minWidth: 1100, minHeight: 600 },
  edit:     { url: '/edit',     title: 'Edit · Spool',     width: 720,  height: 560, minWidth: 600,  minHeight: 480 },
};

/** Open a named view. On touch: routes in-app via goto(). On desktop:
 *  spawns a decorations-free child window (focuses it if already open). */
export async function openView(view: View, params?: Record<string, string>): Promise<void> {
  if (uiMode.resolved === 'touch') {
    const qs = params ? '?' + new URLSearchParams(params).toString() : '';
    await goto(WINDOWS[view].url + qs);
    return;
  }
  const existing = await WebviewWindow.getByLabel(view);
  if (existing) {
    await existing.setFocus();
    return;
  }
  const w = WINDOWS[view];
  new WebviewWindow(view, {
    url: w.url,
    title: w.title,
    width: w.width,
    height: w.height,
    minWidth: w.minWidth,
    minHeight: w.minHeight,
    decorations: false,
    resizable: true,
    center: true,
    backgroundColor: '#0b0c0e',
  });
}
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: both PASS. (No consumer yet — this just type-checks the new file.)

- [ ] **Step 3: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/nav.ts
git commit -m "feat(touch): add nav.ts openView helper (desktop window / touch goto)

Centralises window-spawn vs in-app routing decision. On touch, openView
calls goto(); on desktop, spawns the existing WebviewWindow configs.
Replaces three scattered inline window functions + GameDetail's edit
opener.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Refactor window callers in `+page.svelte`

**Files:**
- Modify: `tauri/src/routes/+page.svelte`

Three inline functions (`openSettingsWindow`, `openBrowseWindow`, `openAddGame`) and the `WebviewWindow` import are replaced by `openView` calls.

- [ ] **Step 1: Swap the `WebviewWindow` import for `openView`**

Line 35 currently reads:
```ts
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
```

Replace with:
```ts
  import { openView } from '$lib/nav';
```

- [ ] **Step 2: Remove `openSettingsWindow` (lines 282–300)**

The entire function block:
```ts
  async function openSettingsWindow() {
    const existing = await WebviewWindow.getByLabel('settings');
    if (existing) {
      await existing.setFocus();
      return;
    }
    new WebviewWindow('settings', {
      url: '/settings',
      title: 'Spool — Settings',
      width: 1180,
      height: 760,
      minWidth: 900,
      minHeight: 600,
      decorations: false,
      resizable: true,
      center: true,
      backgroundColor: '#0b0c0e',
    });
  }
```

Delete it entirely.

- [ ] **Step 3: Remove `openBrowseWindow` (lines 303–321)**

The entire function block:
```ts
  /** Opens (or focuses) the Browse Games child window. */
  async function openBrowseWindow() {
    const existing = await WebviewWindow.getByLabel('browse-games');
    if (existing) {
      await existing.setFocus();
      return;
    }
    new WebviewWindow('browse-games', {
      url: '/browse',
      title: 'Browse Games',
      width: 1280,
      height: 800,
      minWidth: 1100,
      minHeight: 600,
      decorations: false,
      resizable: true,
      center: true,
      backgroundColor: '#0b0c0e',
    });
  }
```

Delete it entirely.

- [ ] **Step 4: Remove `openAddGame` (lines 571–589)**

The entire function block:
```ts
  // ── Add Game popup ─────────────────────────────────────────────────────
  function openAddGame() {
    WebviewWindow.getByLabel('add-game').then((win) => {
      if (win) {
        win.setFocus();
        return;
      }
      new WebviewWindow('add-game', {
        url: '/add',
        title: 'Add Game · Spool',
        width: 720,
        height: 560,
        minWidth: 600,
        minHeight: 480,
        decorations: false,
        resizable: true,
        center: true,
        backgroundColor: '#0b0c0e',
      });
    });
```

Delete it entirely (including the comment line above it).

- [ ] **Step 5: Update callsites**

There are exactly four callsites to update. Find each by text and replace:

`onclick={openBrowseWindow}` → `onclick={() => openView('browse')}`
(appears once, around line 608)

`onclick={openSettingsWindow}` → `onclick={() => openView('settings')}`
(appears twice, around lines 654 and 674)

`onclick={openAddGame}` → `onclick={() => openView('add')}`
(appears twice, around lines 1071 and 1094)

- [ ] **Step 6: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: both PASS. Confirm with `git grep "openSettingsWindow\|openBrowseWindow\|openAddGame\|WebviewWindow" -- tauri/src/routes/+page.svelte` → no matches.

- [ ] **Step 7: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/routes/+page.svelte
git commit -m "refactor(touch): replace inline window functions with openView

openSettingsWindow / openBrowseWindow / openAddGame removed; their
callsites replaced with openView('settings'/'browse'/'add'). Desktop
behavior unchanged; touch gets in-app routing.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Refactor `GameDetail.svelte` edit caller

**Files:**
- Modify: `tauri/src/lib/components/GameDetail.svelte`

- [ ] **Step 1: Read the file to find the exact lines**

Read `tauri/src/lib/components/GameDetail.svelte`, find the `WebviewWindow` import, the `openEditGame` function (around line 125–145), and its callsite (around line 383).

- [ ] **Step 2: Swap the import**

Find the line importing `WebviewWindow`:
```ts
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
```

Replace it with (or add alongside existing imports if `WebviewWindow` is not the only import from that module):
```ts
  import { openView } from '$lib/nav';
```

If `WebviewWindow` is the sole import from `@tauri-apps/api/webviewWindow` in this file, remove the entire import line and add the `openView` import in the existing lib import block.

- [ ] **Step 3: Remove `openEditGame` and update callsite**

The function currently reads:
```ts
  function openEditGame() {
    const url = `/edit?id=${encodeURIComponent(game.id)}`;
    WebviewWindow.getByLabel('edit-game').then((win) => {
      if (win) {
        win.setFocus();
        return;
      }
      new WebviewWindow('edit-game', {
        url,
        title: 'Edit · Spool',
        width: 720,
        height: 560,
        minWidth: 600,
        minHeight: 480,
        decorations: false,
        resizable: true,
        center: true,
        backgroundColor: '#0b0c0e',
      });
    });
  }
```

Delete it. Then find its callsite:
```svelte
    <Btn variant="ghost" onclick={openEditGame}>
```

Replace with:
```svelte
    <Btn variant="ghost" onclick={() => openView('edit', { id: game.id })}>
```

- [ ] **Step 4: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/components/GameDetail.svelte
git commit -m "refactor(touch): replace openEditGame with openView in GameDetail

Removes the last inline WebviewWindow spawn from the library surfaces.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Fix `top-[44px]` chrome-offset debt in `+page.svelte`

**Files:**
- Modify: `tauri/src/routes/+page.svelte` (lines 689 and 709)

These two elements position with `top-[44px]` (hardcoded slightly above the 40px chrome). At touch density `--chrome-h` becomes 60px; they must track the token or they'll overlap the chrome.

- [ ] **Step 1: Fix the transfers panel wrapper (line 689)**

Currently:
```svelte
      class="fixed top-[44px] z-40"
```

Replace with (remove `top-[44px]`, add `style:top`):
```svelte
      class="fixed z-40"
      style:top="var(--chrome-h)"
```

- [ ] **Step 2: Fix the LAN popover (line 709)**

Currently:
```svelte
      class="fixed right-3 top-[44px] z-40 w-[320px] overflow-hidden rounded-md border border-line-2 bg-bg-1"
```

Replace with:
```svelte
      class="fixed right-3 z-40 w-[320px] overflow-hidden rounded-md border border-line-2 bg-bg-1"
      style:top="var(--chrome-h)"
```

- [ ] **Step 3: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: both PASS.

- [ ] **Step 4: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/routes/+page.svelte
git commit -m "fix(touch): make transfers panel + LAN popover track --chrome-h

Replaces hardcoded top-[44px] with var(--chrome-h) so these overlays
sit flush below the strip at both desktop (40px) and touch (60px).

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Create `TouchTopBar.svelte`

**Files:**
- Create: `tauri/src/lib/components/TouchTopBar.svelte`

Port of the prototype's `TopBar` (`design_handoff_touch_mode/prototype/redesign/touch_kit.jsx:74-142`). Sized entirely by PR 1 density tokens — no `uiMode` read inside this component.

- [ ] **Step 1: Write the component**

Create `tauri/src/lib/components/TouchTopBar.svelte` with exactly:

```svelte
<script lang="ts">
  // Touch chrome strip — sized by density tokens, no mode awareness.
  // Replaces WindowChrome on [data-mode='touch']; rendered via AppChrome.
  // Port of the prototype's TopBar (touch_kit.jsx:74).
  import { onMount } from 'svelte';
  import { ChevronLeft, Wifi } from '@lucide/svelte';
  import SpoolMark from './SpoolMark.svelte';
  import MonoLabel from './MonoLabel.svelte';

  let {
    sub,
    accent,
    onback,
    peers = 0,
    transfers = 0,
    conflict = false,
    children,
  }: {
    /** Sub-section label shown after SPOOL/, e.g. "SETTINGS". */
    sub?: string;
    /** Tape-strip colour on the Spool mark. */
    accent?: string;
    /** If provided, a back button is shown that calls this on click. */
    onback?: () => void;
    /** Number of visible LAN peers. */
    peers?: number;
    /** Number of active transfers (for the badge). */
    transfers?: number;
    /** True when sync server is configured but unreachable (amber alert). */
    conflict?: boolean;
    /** Optional center-slot content (search, catalog id, etc.). */
    children?: import('svelte').Snippet;
  } = $props();

  const alert = $derived(conflict || transfers > 0);

  let clock = $state('');
  let batteryPct = $state<number | null>(null);

  function formatClock(d: Date): string {
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
  }

  onMount(() => {
    clock = formatClock(new Date());
    const timer = setInterval(() => { clock = formatClock(new Date()); }, 10_000);

    // Battery API — Chromium/WebView2 only; degrades silently elsewhere.
    (navigator as any).getBattery?.()
      .then((b: any) => {
        batteryPct = Math.round(b.level * 100);
        b.addEventListener('levelchange', () => { batteryPct = Math.round(b.level * 100); });
      })
      .catch(() => { /* not available on this platform */ });

    return () => clearInterval(timer);
  });
</script>

<div
  class="flex shrink-0 items-center gap-3 border-b border-line-1 bg-black/40"
  style:height="var(--chrome-h)"
  style:padding-inline="calc(var(--space-unit) * 4)"
>
  {#if onback}
    <button
      type="button"
      onclick={onback}
      class="inline-flex cursor-pointer items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:text-ink-0"
      style:height="var(--control-h-icon)"
      style:width="var(--control-h-icon)"
      aria-label="Back"
    >
      <ChevronLeft size={20} />
    </button>
  {/if}

  <SpoolMark size={22} color="var(--color-ink-1)" tape={accent ?? 'var(--color-spool)'} />
  <MonoLabel size={10.5}>SPOOL</MonoLabel>
  {#if sub}
    <span class="text-[10px] text-ink-3">/</span>
    <MonoLabel size={10.5} class="text-ink-1">{sub}</MonoLabel>
  {/if}

  <!-- Center slot (search, catalog id, etc.) -->
  <div class="flex-1">
    {#if children}{@render children()}{/if}
  </div>

  <!-- Sync + peers pill -->
  <div
    class={`inline-flex items-center gap-2 rounded-full border ${alert ? 'border-warn/40 bg-warn/10' : 'border-line-2 bg-bg-2'}`}
    style:padding-inline="calc(var(--space-unit) * 3)"
    style:height="calc(var(--control-h) * 0.7)"
  >
    <!-- Status dot -->
    <span
      class="rounded-full"
      style:width="7px"
      style:height="7px"
      style:background={conflict ? 'var(--color-warn)' : peers > 0 ? 'var(--color-ok)' : 'var(--color-ink-3)'}
    ></span>
    <Wifi size={13} class={conflict ? 'text-warn' : 'text-ink-2'} />
    <MonoLabel size={10}>{peers}</MonoLabel>
    {#if transfers > 0}
      <span
        class="inline-flex items-center justify-center rounded-full font-mono text-[10px] font-bold"
        style:min-width="16px"
        style:height="16px"
        style:padding="0 4px"
        style:background="var(--color-spool)"
        style:color="#0b0c0e"
      >{transfers}</span>
    {/if}
  </div>

  <!-- Battery (shown only when API is available) -->
  {#if batteryPct !== null}
    <MonoLabel size={10}>{batteryPct}%</MonoLabel>
  {/if}

  <!-- Clock -->
  {#if clock}
    <MonoLabel size={10}>{clock}</MonoLabel>
  {/if}
</div>
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: both PASS. (No consumer yet.)

- [ ] **Step 3: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/components/TouchTopBar.svelte
git commit -m "feat(touch): add TouchTopBar (sync pill, peers, battery, clock, back)

Port of the prototype's TopBar sized by density tokens. Renders at
var(--chrome-h) / var(--control-h-icon) — grows automatically at touch
density. Battery from navigator.getBattery (Chromium only, graceful
fallback). Shown via AppChrome when resolved mode is touch.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 6: Create `AppChrome.svelte`

**Files:**
- Create: `tauri/src/lib/components/AppChrome.svelte`

- [ ] **Step 1: Write the component**

Create `tauri/src/lib/components/AppChrome.svelte` with exactly:

```svelte
<script lang="ts">
  // Mode-aware chrome wrapper: renders TouchTopBar on touch, WindowChrome
  // on desktop. This is the one place allowed to branch on uiMode for
  // structure — it's a chrome substitution, not a layout branch.
  import { uiMode } from '$lib/uiMode.svelte';
  import WindowChrome from './WindowChrome.svelte';
  import TouchTopBar from './TouchTopBar.svelte';

  let {
    sub,
    accent,
    onback,
    peers = 0,
    transfers = 0,
    conflict = false,
    children,
  }: {
    sub?: string;
    accent?: string;
    /** Touch only: shown as a back button. Omit on the root library page. */
    onback?: () => void;
    /** LAN peer count — forwarded to TouchTopBar sync pill. */
    peers?: number;
    /** Active transfer count — forwarded to TouchTopBar badge. */
    transfers?: number;
    /** Sync conflict/offline flag — drives amber alert state. */
    conflict?: boolean;
    children?: import('svelte').Snippet;
  } = $props();
</script>

{#if uiMode.resolved === 'touch'}
  <TouchTopBar {sub} {accent} {onback} {peers} {transfers} {conflict}>
    {#if children}{@render children()}{/if}
  </TouchTopBar>
{:else}
  <WindowChrome {sub} {accent}>
    {#if children}{@render children()}{/if}
  </WindowChrome>
{/if}
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: both PASS.

- [ ] **Step 3: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/components/AppChrome.svelte
git commit -m "feat(touch): add AppChrome wrapper (WindowChrome or TouchTopBar)

Thin conditional wrapper — uiMode.resolved === 'touch' -> TouchTopBar,
else -> WindowChrome. All pages swap <WindowChrome> -> <AppChrome>.
The only place in shared components allowed to branch on uiMode for
structure (chrome substitution, per handoff §5 / §1 guardrail).

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 7: Swap `<WindowChrome>` → `<AppChrome>` in all five route pages

**Files:**
- Modify: `tauri/src/routes/+page.svelte`
- Modify: `tauri/src/routes/settings/+page.svelte`
- Modify: `tauri/src/routes/add/+page.svelte`
- Modify: `tauri/src/routes/browse/+page.svelte`
- Modify: `tauri/src/routes/edit/+page.svelte`

Do each sub-page (settings/add/browse/edit) individually, then the library page last since it has the live-state props.

### 7a — `settings/+page.svelte`

- [ ] **Step 1: Swap import and usage**

Find (line 22):
```ts
  import WindowChrome from '$lib/components/WindowChrome.svelte';
```
Replace with:
```ts
  import AppChrome from '$lib/components/AppChrome.svelte';
```

Find (line 243):
```svelte
  <WindowChrome sub="SETTINGS" />
```
Replace with:
```svelte
  <AppChrome sub="SETTINGS" onback={() => history.back()} />
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```
Expected: both PASS.

### 7b — `add/+page.svelte`

- [ ] **Step 3: Swap import and usage**

Find (line 22):
```ts
  import WindowChrome from '$lib/components/WindowChrome.svelte';
```
Replace with:
```ts
  import AppChrome from '$lib/components/AppChrome.svelte';
```

Find (line 172):
```svelte
  <WindowChrome sub="ADD ENTRY" />
```
Replace with:
```svelte
  <AppChrome sub="ADD ENTRY" onback={() => history.back()} />
```

- [ ] **Step 4: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```
Expected: both PASS.

### 7c — `browse/+page.svelte`

- [ ] **Step 5: Swap import and usage**

Find (line 43):
```ts
  import WindowChrome from '$lib/components/WindowChrome.svelte';
```
Replace with:
```ts
  import AppChrome from '$lib/components/AppChrome.svelte';
```

The browse page uses `<WindowChrome>` as a wrapper with children (lines 381 and 404):
```svelte
  <WindowChrome sub="BROWSE · SOURCES">
    ...
  </WindowChrome>
```
Replace opening and closing tags:
```svelte
  <AppChrome sub="BROWSE · SOURCES" onback={() => history.back()}>
    ...
  </AppChrome>
```

- [ ] **Step 6: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```
Expected: both PASS.

### 7d — `edit/+page.svelte`

- [ ] **Step 7: Swap import and usage**

Find (line 28):
```ts
  import WindowChrome from '$lib/components/WindowChrome.svelte';
```
Replace with:
```ts
  import AppChrome from '$lib/components/AppChrome.svelte';
```

Find (line 204):
```svelte
  <WindowChrome sub="EDIT · ENTRY" {accent} />
```
Replace with:
```svelte
  <AppChrome sub="EDIT · ENTRY" {accent} onback={() => history.back()} />
```

- [ ] **Step 8: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```
Expected: both PASS.

### 7e — `+page.svelte` (library, has live-state props)

The library page has `lanPeers`, `activeDownload`, and `syncOff` in scope. Read the file first to confirm their names haven't changed.

- [ ] **Step 9: Swap import**

Find (line 52):
```ts
  import WindowChrome from '$lib/components/WindowChrome.svelte';
```
Replace with:
```ts
  import AppChrome from '$lib/components/AppChrome.svelte';
```

- [ ] **Step 10: Swap the chrome element in the template**

The library chrome (around line 600) currently reads:
```svelte
  <WindowChrome sub="LIBRARY">
```
and closes around line 684:
```svelte
  </WindowChrome>
```

Replace the opening tag with:
```svelte
  <AppChrome
    sub="LIBRARY"
    peers={lanPeers.length}
    transfers={activeDownload?.status === 'starting' || activeDownload?.status === 'transferring' ? 1 : 0}
    conflict={syncOff}
  >
```

Replace the closing tag with:
```svelte
  </AppChrome>
```

- [ ] **Step 11: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Confirm: `git grep "WindowChrome" -- tauri/src/routes/` → no matches.

- [ ] **Step 12: Commit all five pages**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/routes/+page.svelte tauri/src/routes/settings/+page.svelte tauri/src/routes/add/+page.svelte tauri/src/routes/browse/+page.svelte tauri/src/routes/edit/+page.svelte
git commit -m "refactor(touch): swap <WindowChrome> -> <AppChrome> in all pages

Sub-pages (settings/add/browse/edit) pass onback={() => history.back()}
for touch in-app back navigation. Library page passes live sync + peers
+ transfer state to the touch pill. Desktop behavior unchanged.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 8: Maximize window on touch in `+layout.svelte`

**Files:**
- Modify: `tauri/src/routes/+layout.svelte`

When touch mode resolves at boot, the main window should fill the screen (Deck/Ally are always fullscreen). Sub-views route in-app in touch mode (no child windows), so this only fires once on the main window.

- [ ] **Step 1: Add the maximize call**

The current `+layout.svelte` `onMount` block (added in PR 2) reads:

```svelte
<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import ToastStack from '$lib/components/ToastStack.svelte';
  import { api } from '$lib/api';
  import { uiMode } from '$lib/uiMode.svelte';

  let { children } = $props();

  onMount(async () => {
    try {
      const config = await api.getConfig();
      await uiMode.init(config.ui_mode);
    } catch (e) {
      console.error('[layout] uiMode init failed; defaulting to auto:', e);
      await uiMode.init('auto');
    }
  });
</script>
```

Replace the entire file with:

```svelte
<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import ToastStack from '$lib/components/ToastStack.svelte';
  import { api } from '$lib/api';
  import { uiMode } from '$lib/uiMode.svelte';

  let { children } = $props();

  onMount(async () => {
    try {
      const config = await api.getConfig();
      await uiMode.init(config.ui_mode);
      if (uiMode.resolved === 'touch') {
        // Deck/Ally are always fullscreen — maximize before first paint.
        await getCurrentWindow().maximize();
      }
    } catch (e) {
      console.error('[layout] uiMode init failed; defaulting to auto:', e);
      await uiMode.init('auto');
    }
  });
</script>

{@render children()}

<!-- Global toast stack — overlaid on every route. -->
<ToastStack />
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: both PASS.

- [ ] **Step 3: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/routes/+layout.svelte
git commit -m "feat(touch): maximize window at boot when touch mode resolves

getCurrentWindow().maximize() runs once after uiMode.init() resolves
to touch. In touch mode all subviews route in-app (no child windows),
so this only fires on the main window.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 9: Full verification + guardrail self-check

**Files:** none (verification only).

- [ ] **Step 1: Full backend + frontend check suite**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri\src-tauri && cargo check && cargo clippy && cargo test
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```

Expected: all PASS.

- [ ] **Step 2: Confirm `WindowChrome` is fully replaced**

```bash
git grep "WindowChrome" -- tauri/src/routes/
```

Expected: **no matches**. (WindowChrome still lives in `lib/components/` — it's used by `AppChrome`. Routes must not reference it directly.)

- [ ] **Step 3: Guardrail self-check (handoff §1)**

- No `*Touch.svelte` created beyond `TouchTopBar`. ✅ (`TouchTopBar` is the one sanctioned touch-only component — the counterpart to desktop-only `WindowChrome`, hidden behind `AppChrome`)
- No shared component (other than `AppChrome`) reads `uiMode` for layout/structure branching. ✅
- Density lives in CSS tokens; `TouchTopBar` sizes from `--chrome-h`/`--control-h-icon`/`--space-unit`. ✅
- Every screen renders acceptably at both `data-mode` values. (Verify in Step 4–5.)
- `top-[44px]` hardcoding eliminated. ✅ (Task 4)

- [ ] **Step 4: Desktop smoke test (manual)**

`bun run tauri dev`. With `ui_mode = auto` (desktop pointer):
- Library, Settings, Add Game, Browse all open as **child windows** as before.
- `AppChrome` renders `WindowChrome` — title bar looks identical to pre-PR-3 desktop.
- Transfers panel and LAN popover appear flush below the chrome (no gap change at desktop density).

- [ ] **Step 5: Touch smoke test (manual — devtools)**

In devtools: `document.documentElement.dataset.mode = 'touch'`. Then:
- The main window chrome flips to `TouchTopBar` — back button absent (library root), sync pill shows peers and status, clock renders, battery shows if available.
- Settings: `onclick={() => openView('settings')}` — in touch mode this calls `goto('/settings')`, routing in-app. The settings page shows `TouchTopBar` with a back button. Back navigates to the library.
- Add Game, Browse: same — in-app navigation, back button works.
- Transfers panel and LAN popover open flush below the 60px chrome (no overlap).

Reset: `delete document.documentElement.dataset.mode`.

- [ ] **Step 6: Finish the branch**

Announce: "I'm using the finishing-a-development-branch skill to complete this work." Then follow **superpowers:finishing-a-development-branch**. This branch is stacked on `touch-mode-pr1-density-tokens` (which holds PRs 1+2). Do not push without explicit confirmation.

---

## Self-review notes (already reconciled)

- **Spec coverage (handoff §4–5 / TASKS.md PR 3):**
  - `lib/nav.ts` with `openView()` ✅ (Task 1)
  - Replace `new WebviewWindow(...)` callers in `+page.svelte` ✅ (Task 2)
  - Replace `openEditGame` in `GameDetail.svelte` ✅ (Task 3)
  - `TouchTopBar.svelte` with sync pill, peers, battery, clock, back ✅ (Task 5)
  - `AppChrome.svelte` swapping `WindowChrome` / `TouchTopBar` ✅ (Task 6)
  - All pages swap `<WindowChrome>` → `<AppChrome>` ✅ (Task 7)
  - Main window maximized on touch ✅ (Task 8)
  - Verify: on touch, settings/add/browse/edit open in-app with back; desktop spawns windows ✅ (Task 9)
  - Cross-PR debt (`top-[44px]`) fixed ✅ (Task 4)

- **Placeholder scan:** All code blocks are complete and exact. No "TBD" or "similar to above" entries.

- **Type/name consistency:** `AppChrome` props (`sub`, `accent`, `onback`, `peers`, `transfers`, `conflict`, `children`) are defined identically in Task 6 and used identically in Task 7. `TouchTopBar` receives those same props and is defined with matching signatures in Task 5. `openView(view: View, params?)` is defined in Task 1 and called as `openView('browse')`, `openView('settings')`, `openView('add')`, `openView('edit', { id: game.id })` in Tasks 2–3. `View` type covers all four. `syncOff` and `activeDownload` are confirmed existing names from the codebase read in pre-plan research.
