# Touch Mode PR 2 — Mode Flag + Settings Toggle + `data-mode` Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the density layer shipped in PR 1 actually switchable — add a persisted `ui_mode` setting (Auto/Desktop/Touch), resolve it once at boot to write `data-mode` on `<html>`, and expose an Auto/Desktop/Touch segmented control in Settings → Display.

**Architecture:** A typed `UiMode` enum (`Auto|Desktop|Touch`) is added to the Rust `ConfigData` and mirrored in `types.ts` (replacing the dead `touch_mode` string that was never wired to anything). A new runes store `lib/uiMode.svelte.ts` resolves the setting (`auto` → detect coarse-pointer / small panel; else the explicit choice) to a concrete `'desktop' | 'touch'`, writes it to `document.documentElement.dataset.mode`, and exposes `resolved`/`setting` as `$state`. `+layout.svelte` calls `uiMode.init(config.ui_mode)` on mount in every window. A new mode-agnostic `Segmented.svelte` primitive (sized by the PR 1 density tokens) drives the Settings control, which persists via `api.updateConfig` then re-inits the store — no restart.

**Tech Stack:** Tauri 2 (Rust, serde), SvelteKit 5 (runes, `.svelte.ts` modules), Tailwind v4 (`style:` directives + density tokens from PR 1), TypeScript. Verified with `cargo check`/`clippy`, `bun run check`, `bun run lint`, and manual inspection of the live mode switch.

This is PR 2 of the 6-PR rollout in `design_handoff_touch_mode/Touch Mode - Refactor Plan.md` (§2). It depends on PR 1's density tokens (`--text-base`, `--control-h`, `--space-unit`, `[data-mode='touch']`), which are already on the parent branch `touch-mode-pr1-density-tokens`.

---

## Design decisions reconciled before writing this plan

- **`ui_mode` enum, not the dead `touch_mode` string.** `config.rs`/`types.ts` carry an inert `touch_mode: "auto"|"on"|"off"` field added in the first settings commit (`a01fc86`) and read by **nothing** (verified via `git grep`). The Refactor Plan §2a designs a typed `ui_mode: UiMode (Auto|Desktop|Touch)`, and PR 1 already shipped `[data-mode='touch']` with an `app.css` comment pointing to `lib/uiMode.svelte.ts` in PR 2 — so the `uiMode`/`'desktop'|'touch'` naming is already baked in. This plan **adds `ui_mode` and removes the dead `touch_mode`** (user-confirmed). Removal is zero-migration: serde ignores unknown JSON keys by default (no `deny_unknown_fields`), so existing `config.json` files with `"touch_mode"` load fine and simply stop re-serializing it.
- **Settings labels: Auto / Desktop / Touch** (per Refactor Plan §2c), not the prototype mock's "Auto / On / Off". The underlying values are `auto`/`desktop`/`touch`.
- **`api.ts` needs no new method.** The handoff mentions "add `setConfig`/getter coverage if not already present" — it *is* present: `api.getConfig()` → `get_config`, `api.updateConfig(data)` → `update_config`. We reuse `updateConfig`.

## Why this PR is verified with checks + manual gate, not new unit tests

PR 2's only non-trivial logic is `uiMode.detect()`, which calls `matchMedia('(pointer: coarse)')` and the Tauri `getCurrentWindow().innerSize()` IPC — neither resolves meaningfully under jsdom (Vitest), so a unit test would mock both sides and assert the mock, i.e. theater. The non-`auto` path is a trivial identity (`setting === 'auto' ? detect() : setting`). Per handoff §10, real controller unit tests arrive in **PR 4** (when `library.svelte.ts` is extracted and has view-independent logic). This PR follows PR 1's precedent: `cargo check`/`clippy`, `bun run check`/`lint` to catch breakage, plus a concrete manual switch test. The manual gate (handoff §1) is the real contract here: *flipping the Settings control changes every control's density live, no restart.*

## Out of scope for PR 2 (deliberately deferred)

- **`nav.ts`, `AppChrome`, `TouchTopBar`** — PR 3. On touch, Settings/Add/Browse/Edit still spawn child windows in PR 2; that's fine because each window runs `+layout` and resolves its own `data-mode`.
- **Cross-window live propagation.** Changing the mode in the Settings window updates *that* window's `data-mode` immediately; the main library window updates on its next open. The handheld target is single-window, and PR 3's nav rework makes subviews in-app, so this is a non-issue in practice. Not solved here.
- **Fullscreen/maximize on touch** — PR 3 (§5).
- **The touch density of `Toggle` / segmented track tuning** — `Segmented` is sized by `--control-h` so it already reaches 48px at touch; no extra work.
- **Auto-detection refinement** (physical vs logical pixels, chassis allowlist) — the handoff's `innerSize() <= 900 || coarse-pointer` heuristic is used verbatim; refine later if a docked Ally misdetects.

## File map

| File | Change |
|---|---|
| `tauri/src-tauri/src/config.rs` | **Modify** — add `UiMode` enum + `ui_mode` field to `ConfigData`; remove dead `touch_mode` field + its default. |
| `tauri/src/lib/types.ts` | **Modify** — add `UiMode` union + `ui_mode`; remove `touch_mode`. |
| `tauri/src/lib/uiMode.svelte.ts` | **Create** — runes store: `resolved`/`setting` `$state`, `init()`, `detect()`. |
| `tauri/src/routes/+layout.svelte` | **Modify** — `onMount` → `getConfig()` → `uiMode.init(config.ui_mode)`. |
| `tauri/src/lib/components/Segmented.svelte` | **Create** — mode-agnostic segmented control, density-token sized. |
| `tauri/src/routes/settings/+page.svelte` | **Modify** — add `Display` nav group + section with the `ui_mode` `Segmented`; persist + re-init on change. |

All `bun`/`cargo` commands run from `C:\Users\akinz\Git\ludusavi-wrap\tauri` (frontend) or `C:\Users\akinz\Git\ludusavi-wrap\tauri\src-tauri` (backend) as noted.

---

## Task 0: Confirm baseline (prep)

The branch `touch-mode-pr2-mode-flag` already exists, cut from `touch-mode-pr1-density-tokens`.

- [ ] **Step 1: Confirm the branch and a clean-enough tree**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git branch --show-current   # expect: touch-mode-pr2-mode-flag
git status --short          # the only pre-existing noise: a CRLF-only mod on tauri/src-tauri/Cargo.toml + untracked .serena/ and design_handoff_touch_mode/ — leave them
```

- [ ] **Step 2: Confirm `touch_mode` truly has no readers before removing it**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git grep -n "touch_mode\|touchMode" -- tauri server
```
Expected: matches **only** in `tauri/src-tauri/src/config.rs` (field decl + default) and `tauri/src/lib/types.ts` (field decl). If anything else references it, STOP and reassess — the removal assumption is wrong.

- [ ] **Step 3: Confirm the checks are green before changing anything**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri\src-tauri && cargo check
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```
Expected: all pass. If not, that's pre-existing — resolve/flag before starting.

---

## Task 1: Add `UiMode` + `ui_mode` to Rust `ConfigData`, remove dead `touch_mode`

**Files:**
- Modify: `tauri/src-tauri/src/config.rs` (the `UiMode` enum near the struct; `ConfigData` field ~line 54-55; `Default` impl ~line 85)

- [ ] **Step 1: Define the `UiMode` enum above `ConfigData`**

Immediately **before** the `#[derive(Debug, Clone, Serialize, Deserialize)]` line on `pub struct ConfigData` (currently line 19), insert:

```rust
/// UI density / layout mode. `Auto` resolves at runtime (frontend) to
/// `desktop` or `touch` from pointer + panel size; `Desktop`/`Touch` force
/// it. Serialized lowercase (`"auto"`/`"desktop"`/`"touch"`) to match the
/// `UiMode` union in types.ts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiMode {
    #[default]
    Auto,
    Desktop,
    Touch,
}

```

- [ ] **Step 2: Replace the dead `touch_mode` field with `ui_mode`**

In `ConfigData`, the field block currently (lines 54-55) reads:

```rust
    /// `"auto"`, `"on"`, or `"off"`. Touch-optimised UI mode (handheld).
    pub touch_mode: String,
```

Replace it with:

```rust
    /// Touch-optimised UI mode (handheld). Resolved to a concrete
    /// desktop/touch density at boot by `lib/uiMode.svelte.ts`.
    pub ui_mode: UiMode,
```

- [ ] **Step 3: Update the `Default` impl**

In `impl Default for ConfigData`, the line (currently line 85) reads:

```rust
            touch_mode: "auto".to_string(),
```

Replace it with:

```rust
            ui_mode: UiMode::default(),
```

- [ ] **Step 4: Verify the backend compiles and is clippy-clean**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri\src-tauri
cargo check
cargo clippy
cargo test
```
Expected: all PASS, no new warnings. `UiMode::default()` is `Auto`; serde emits `"ui_mode": "auto"`. Existing `config.json` files with a stray `"touch_mode"` key still load (unknown keys ignored) and drop it on next save.

- [ ] **Step 5: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src-tauri/src/config.rs
git commit -m "$(cat <<'EOF'
feat(touch): add ui_mode (Auto|Desktop|Touch) to ConfigData

Typed UiMode enum (serde lowercase) replaces the dead touch_mode string
that was never read anywhere. Zero migration: serde ignores the legacy
touch_mode key on load and stops writing it. Resolved to a concrete
desktop/touch density at boot by lib/uiMode.svelte.ts (next tasks).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Mirror `UiMode` + `ui_mode` in `types.ts`, remove `touch_mode`

**Files:**
- Modify: `tauri/src/lib/types.ts` (`ConfigData` type, line 28; add the `UiMode` union)

- [ ] **Step 1: Add the `UiMode` union and swap the field**

In `types.ts`, line 28 currently reads:

```ts
  touch_mode: string;
```

Replace that single line with:

```ts
  ui_mode: UiMode;
```

Then, immediately **above** the `export type ConfigData = {` line (currently line 3), add the union:

```ts
/** Mirror of the Rust `UiMode` enum (serde rename_all = "lowercase"). */
export type UiMode = 'auto' | 'desktop' | 'touch';

```

- [ ] **Step 2: Verify types**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run check
```
Expected: PASS. (No consumer of `touch_mode` exists, so removing it can't break callers — confirmed in Task 0 Step 2.)

- [ ] **Step 3: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/types.ts
git commit -m "$(cat <<'EOF'
refactor(touch): mirror UiMode + ui_mode in types.ts

Adds the UiMode union ('auto'|'desktop'|'touch') and ui_mode field to
ConfigData; drops the dead touch_mode string. Keeps the TS mirror in
lockstep with the Rust ConfigData.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Create the `uiMode` runes store

**Files:**
- Create: `tauri/src/lib/uiMode.svelte.ts`

- [ ] **Step 1: Write the store**

Create `tauri/src/lib/uiMode.svelte.ts` with exactly:

```ts
// Single source of truth for the resolved UI density/layout mode.
//
// `init()` is called once per window at boot (in +layout.svelte) and again
// whenever the user changes the Settings control. It resolves the persisted
// `ui_mode` setting to a concrete `'desktop' | 'touch'` and writes it to
// `<html data-mode>`, which is what the PR 1 density tokens key off
// (`[data-mode='touch']` in app.css). Only the nav layer, the chrome
// wrapper, and the one library layout branch read `resolved` directly;
// everything else just inherits density from `data-mode`.
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { UiMode } from './types';

class UiModeStore {
  /** Concrete mode the UI renders at. Defaults to desktop so first paint
   *  matches the PR 1 :root token values before init() resolves. */
  resolved = $state<'desktop' | 'touch'>('desktop');
  /** The persisted user choice (Auto/Desktop/Touch). */
  setting = $state<UiMode>('auto');

  async init(setting: UiMode) {
    this.setting = setting;
    this.resolved = setting === 'auto' ? await this.detect() : setting;
    document.documentElement.dataset.mode = this.resolved;
  }

  private async detect(): Promise<'desktop' | 'touch'> {
    const coarse = matchMedia('(pointer: coarse)').matches;
    let small = false;
    try {
      const size = await getCurrentWindow().innerSize();
      small = Math.min(size.width, size.height) <= 900; // Deck/Ally panels
    } catch {
      // innerSize() can reject outside a Tauri window (e.g. browser dev) —
      // fall back to the pointer signal alone.
    }
    return coarse || small ? 'touch' : 'desktop';
  }
}

export const uiMode = new UiModeStore();
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run check && bun run lint
```
Expected: both PASS. (The store has no consumers yet, so this is inert — it just type-checks.)

- [ ] **Step 3: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/uiMode.svelte.ts
git commit -m "$(cat <<'EOF'
feat(touch): add uiMode runes store (resolve + data-mode)

Resolves the persisted ui_mode setting to a concrete desktop/touch value
(auto -> coarse-pointer / <=900px panel detection) and writes it to
<html data-mode>, which the PR 1 density tokens key off. Inert until
+layout calls init() (next task).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Resolve the mode at boot in `+layout.svelte`

**Files:**
- Modify: `tauri/src/routes/+layout.svelte` (whole file — currently 12 lines)

- [ ] **Step 1: Add the boot-time init**

Replace the entire contents of `tauri/src/routes/+layout.svelte` with:

```svelte
<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import ToastStack from '$lib/components/ToastStack.svelte';
  import { api } from '$lib/api';
  import { uiMode } from '$lib/uiMode.svelte';

  let { children } = $props();

  // Resolve the UI mode once per window at boot, after config loads, so
  // <html data-mode> is set before the user interacts. Every window runs
  // this layout, so each resolves its own data-mode.
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

{@render children()}

<!-- Global toast stack — overlaid on every route. -->
<ToastStack />
```

- [ ] **Step 2: Verify (checks)**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run check && bun run lint
```
Expected: both PASS.

- [ ] **Step 3: Verify (manual, live app)**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run tauri dev
```
With the app open, in devtools:
- `document.documentElement.dataset.mode` → expect `"desktop"` on a normal monitor (config defaults to `auto`, mouse = fine pointer, window > 900px). The app looks **identical** to PR 1 desktop.
- No console error from the layout init.

(If you're on a coarse-pointer / small panel, it may resolve `"touch"` — that's correct. Close the dev app when done.)

- [ ] **Step 4: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/routes/+layout.svelte
git commit -m "$(cat <<'EOF'
feat(touch): resolve ui_mode at boot in +layout

onMount loads config and calls uiMode.init(config.ui_mode), setting
<html data-mode> before first interaction. Falls back to auto-detect if
config load fails. Runs in every window.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Create the `Segmented` primitive

**Files:**
- Create: `tauri/src/lib/components/Segmented.svelte`

A mode-agnostic segmented control, sized entirely by the PR 1 density tokens (so it reaches 48px buttons at touch automatically — guardrail-compliant, no `uiMode` read). Visual design ported from the prototype's `TSegmented` (`design_handoff_touch_mode/prototype/redesign/touch_kit.jsx:268`): subtle inset track, accent-filled active button with dark text.

- [ ] **Step 1: Write the component**

Create `tauri/src/lib/components/Segmented.svelte` with exactly:

```svelte
<script lang="ts">
  // Generic segmented control. Mode-agnostic: sized by density tokens, so
  // it grows to touch targets at [data-mode='touch'] without reading uiMode.
  type Option = { value: string; label: string };
  let {
    options,
    value,
    onchange,
  }: { options: Option[]; value: string; onchange: (value: string) => void } = $props();
</script>

<div
  class="inline-flex gap-1 rounded-sm border border-line-2 bg-white/5"
  style:padding="calc(var(--space-unit) * 1)"
>
  {#each options as o (o.value)}
    {@const active = o.value === value}
    <button
      type="button"
      onclick={() => onchange(o.value)}
      class="cursor-pointer whitespace-nowrap rounded-sm border-none text-[length:var(--text-base)] transition-colors"
      style:height="var(--control-h)"
      style:padding-inline="calc(var(--space-unit) * 3)"
      style:font-weight={active ? 600 : 500}
      style:background={active ? 'var(--color-spool)' : 'transparent'}
      style:color={active ? '#0b0c0e' : 'var(--color-ink-1)'}
    >
      {o.label}
    </button>
  {/each}
</div>
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run check && bun run lint
```
Expected: both PASS. (No consumer yet — inert.)

- [ ] **Step 3: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/components/Segmented.svelte
git commit -m "$(cat <<'EOF'
feat(touch): add Segmented primitive (density-driven)

Generic segmented control sized by --control-h / --space-unit / --text-base
so it reaches 48px buttons under [data-mode='touch'] with no mode
awareness. Visual port of the prototype's TSegmented. Used by the
Settings Display control next.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Add the Settings → Display control

**Files:**
- Modify: `tauri/src/routes/settings/+page.svelte` (imports ~lines 3-28; `NAV_GROUPS` ~line 196; group-icon block ~line 249; a new persist handler near `persist`; a new `<section>` in the scroll body after the page header ~line 298)

- [ ] **Step 1: Add imports (icon, `Segmented`, `uiMode`)**

In the `@lucide/svelte` import (lines 3-14), add `MonitorSmartphone` to the alphabetised list (between `Library` and `Plus`):

```ts
    Library,
    MonitorSmartphone,
    Plus,
```

After the `import SettingsRow ...` line (line 28), add:

```ts
  import Segmented from '$lib/components/Segmented.svelte';
  import { uiMode } from '$lib/uiMode.svelte';
```

- [ ] **Step 2: Add the persist-and-reinit handler**

Immediately after the `persist()` function (which ends at line 109 with its closing `}`), add:

```ts
  async function setUiMode(mode: ConfigData['ui_mode']) {
    if (!config) return;
    config.ui_mode = mode;
    await persist();
    await uiMode.init(mode); // applies <html data-mode> live in this window
  }
```

- [ ] **Step 3: Add the `Display` nav group (first group)**

In `NAV_GROUPS` (starts line 196), insert a new group object as the **first** array element, before the `library` group:

```ts
    {
      id: 'display',
      title: 'Display',
      items: [
        { id: 'display', title: 'Display & touch', sub: 'Density & handheld mode' },
      ],
    },
```

- [ ] **Step 4: Give the `display` group its sidebar icon**

In the group-header icon block (lines 249-255), add a `display` branch as the first condition:

```svelte
              {#if group.id === 'display'}
                <MonitorSmartphone size={13} class="text-ink-2" />
              {:else if group.id === 'library'}
                <Library size={13} class="text-ink-2" />
              {:else if group.id === 'sharing'}
                <Wifi size={13} class="text-ink-2" />
              {:else}
                <Layers size={13} class="text-ink-2" />
              {/if}
```

- [ ] **Step 5: Add the `Display` section to the scroll body**

In the scroll body, **immediately after** the page-header `</div>` (the block that closes at line 297, right before the `<!-- ════════ LIBRARY GROUP ════════ -->` comment on line 299), insert:

```svelte
          <!-- ════════════════ DISPLAY GROUP ════════════════ -->
          <section class="mb-9">
            <div class="mb-3.5 border-b border-line-1 pb-2.5">
              <h2 class="font-display text-[20px] font-semibold tracking-[-0.01em] text-ink-0">Display</h2>
              <div class="mt-[3px] text-[12px] text-ink-2">How big the controls are and whether Spool runs in handheld mode.</div>
            </div>
            <div class="flex flex-col gap-4">
              <div id="display">
                <SettingsCard title="Display & touch" helper="Auto detects a touchscreen and grows targets for handhelds. Override it for a Deck/Ally docked to a monitor.">
                  <SettingsRow
                    label="Touch mode"
                    helper={`Larger buttons, taller rows, tap-friendly spacing. Currently rendering: ${uiMode.resolved}.`}
                  >
                    {#snippet extras()}
                      <Segmented
                        value={config.ui_mode}
                        onchange={(v) => setUiMode(v as ConfigData['ui_mode'])}
                        options={[
                          { value: 'auto', label: 'Auto' },
                          { value: 'desktop', label: 'Desktop' },
                          { value: 'touch', label: 'Touch' },
                        ]}
                      />
                    {/snippet}
                  </SettingsRow>
                </SettingsCard>
              </div>
            </div>
          </section>

```

- [ ] **Step 6: Verify (checks)**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run check && bun run lint
```
Expected: both PASS.

- [ ] **Step 7: Verify (manual — the core contract)**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run tauri dev
```
Open Settings (from the library top bar). The **Display** group is first in the sidebar nav, with a Touch-mode segmented control reading **Auto** (the default). Then:
- Click **Touch** → the entire Settings window grows to touch density **live, no restart**: buttons/fields ~48px, bigger text, roomier rows. `document.documentElement.dataset.mode` → `"touch"`. The helper text under the control updates to "Currently rendering: touch."
- Click **Desktop** → snaps back to desktop density; `data-mode` → `"desktop"`.
- Click **Auto** → resolves by detection (desktop on a normal monitor).
- Reopen Settings (close + reopen the window) → the control still reads your last choice (persisted to `config.json`). Confirm `%LOCALAPPDATA%\Spool\config.json` now has `"ui_mode": "<choice>"` and **no** `"touch_mode"` key.

Close the dev app when done.

- [ ] **Step 8: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/routes/settings/+page.svelte
git commit -m "$(cat <<'EOF'
feat(touch): add Settings Display control for ui_mode

New Display nav group + section with an Auto/Desktop/Touch Segmented
control bound to config.ui_mode. On change: persist via updateConfig then
uiMode.init() to apply data-mode live (no restart). Helper shows the
currently-resolved mode.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Full verification + guardrail self-check

**Files:** none (verification only).

- [ ] **Step 1: Full backend + frontend check suite**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri\src-tauri && cargo check && cargo clippy && cargo test
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint
```
Expected: all PASS.

- [ ] **Step 2: Desktop no-regression**

`bun run tauri dev`. With `ui_mode` = Auto/Desktop on a monitor, the library, Settings, and Add Game windows look **identical to PR 1** (which was already desktop-identical to master bar the two documented PR 1 deltas). No new visual change from PR 2 at desktop density.

- [ ] **Step 3: Touch density across the app**

Set the Settings control to **Touch**, then open each window (library, Settings, Add Game, Browse). Every primary control (`Btn`, `TextField`, icon buttons, search field, `Segmented`, Settings rows) is **≥48px** in its tappable dimension. Resize toward 1280×800 and 1920×1080 — controls stay large, nothing clips. (The library *layout* is still desktop two-pane — correct; the touch layout is PR 5. Subviews still open as child windows — correct; in-app routing is PR 3.)

- [ ] **Step 4: Guardrail self-check (handoff §1)**

Confirm before finishing:
- No `*Touch.svelte` files created. ✅ (`Segmented` is mode-agnostic; `TouchTopBar` is PR 3.)
- No shared component reads `uiMode` to pick its own sizes — `Segmented` sizes from tokens only; only `+layout` (boot) and the Settings helper read `uiMode`, both legitimate. ✅
- Density lives in CSS variables; mode is one persisted, overridable flag. ✅
- Every changed screen renders acceptably at both `data-mode` values (Step 2 + 3). ✅

- [ ] **Step 5: Finish the branch**

Announce: "I'm using the finishing-a-development-branch skill to complete this work." Then follow **superpowers:finishing-a-development-branch** to present integration options (this branch is stacked on the un-merged `touch-mode-pr1-density-tokens`; PR 1 must merge before PR 2 can target `master` cleanly — surface that). Do not push or open a PR without explicit confirmation (PR 1 was deliberately kept local pending human visual verification; treat PR 2 the same unless told otherwise).

---

## Self-review notes (already reconciled)

- **Spec coverage (handoff §2 / TASKS.md PR 2):** `ui_mode: UiMode` + enum in `config.rs` (Task 1) ✅; `UiMode` + `ui_mode` in `types.ts` (Task 2) ✅; `api.ts` coverage — already present (`getConfig`/`updateConfig`), no change needed, documented above ✅; `lib/uiMode.svelte.ts` resolve + `data-mode` (Task 3) ✅; `uiMode.init(config.ui_mode)` in `+layout` (Task 4) ✅; Settings Auto/Desktop/Touch segmented, persist + re-init (Tasks 5-6) ✅; verify touch density app-wide on Deck/Ally (Task 7) ✅.
- **Refinement over the raw handoff:** retires the dead `touch_mode` field rather than leaving two competing mode fields (user-confirmed); adds a reusable `Segmented` primitive instead of inlining markup (the Settings file is already ~800 lines); documents the cross-window-propagation limitation that PR 3 dissolves.
- **Type/name consistency:** Rust `UiMode {Auto,Desktop,Touch}` ↔ serde lowercase ↔ TS `UiMode = 'auto'|'desktop'|'touch'` ↔ store `setting: UiMode` / `resolved: 'desktop'|'touch'` ↔ `data-mode` values `desktop`/`touch` (matching PR 1's `[data-mode='touch']`). `api.updateConfig` (not the handoff's loose `setConfig`) is the real method. `Segmented` props (`options`/`value`/`onchange`) are used identically in Task 5 and Task 6.
- **No placeholders:** every step shows exact before/after strings or full file contents against the current code.
