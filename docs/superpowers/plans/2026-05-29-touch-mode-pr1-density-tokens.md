# Touch Mode PR 1 — Density Tokens + Primitive Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce a runtime-switchable density layer (CSS custom properties with a `[data-mode='touch']` override) and rewire the app's primitive components to consume it — so the entire app becomes usable at handheld touch sizes while desktop stays visually identical.

**Architecture:** Add eight semantic sizing tokens to `app.css` whose `:root` (desktop) values equal today's hardcoded numbers, plus a `[data-mode='touch']` block that scales them up. Then mechanically replace hardcoded sizes in the primitives (`Btn`, `TextField`, `Toggle`, `Pill`, `SettingsRow`), the chrome strip, the sidebar search field, and the library top-bar icon buttons with `var(--…)` references. No component reads `uiMode`; the mode (wired in PR 2) only changes what the tokens resolve to. This is PR 1 of the 6-PR rollout in `design_handoff_touch_mode/Touch Mode - Refactor Plan.md`.

**Tech Stack:** SvelteKit 5 (runes), Tailwind v4 (`@theme` + arbitrary-value utilities + `style:` directives), TypeScript. Verified with `bun run check`, `bun run lint`, and manual visual inspection at both `data-mode` values.

---

## Why this PR is verified visually, not with unit tests

PR 1 is a pure presentational refactor — it adds no logic, no new state, no IPC. Its behavioral contract is *"a control's pixel size is driven by a token whose value flips with `data-mode`."* jsdom (the Vitest environment here) does **not** resolve Tailwind stylesheets or compute `var()` cascades, so a `getComputedStyle(...).height` assertion would not actually exercise the contract — it would be theater. The handoff's own §10 prescribes a **screenshot diff against `main`** plus a **manual gate at both `data-mode` values** for this PR. This plan follows that: every task's verification is `bun run check` + `bun run lint` (catch breakage) followed by a concrete devtools measurement. Controller unit tests arrive in PR 4, where there is real logic to test.

There is no mode UI yet (that is PR 2). To exercise touch density in PR 1, you set the attribute by hand in devtools:

```js
// In the running dev app's devtools console:
document.documentElement.dataset.mode = 'touch';   // scale up
delete document.documentElement.dataset.mode;       // back to desktop
```

## Known, intended desktop deltas (NOT regressions)

The handoff maps several distinct "today" numbers onto a single token each. Two mappings therefore shift desktop by a sub-control amount. These are expected and acceptable — flag them in the screenshot diff, do not "fix" them:

| Element | Today | After (desktop token value) | Delta |
|---|---|---|---|
| Control label text (`Btn`, `TextField`, `SettingsRow` label, sidebar search, list rows) | `12.5px` | `--text-base` = `13px` | **+0.5px** |
| Sidebar search field height | `h-[30px]` (30px) | `--control-h` = `32px` | **+2px** |

Everything else maps exactly: `h-8`→`--control-h` (32=32), `h-7 w-7`→`--control-h-icon` (28=28), chrome `h-10`→`--chrome-h` (40=40), body `13px`→`--text-base` (13=13), `px-3`→`calc(var(--space-unit)*3)` (12=12), `px-2`→`calc(var(--space-unit)*2)` (8=8).

## Out of scope for PR 1 (deliberately left alone)

- `tokens.ts` — mirrors **colors/fonts/radii only**; it has no sizing values, so it is untouched.
- `WindowChrome` min/max/close buttons (`h-full w-12`) — desktop-only chrome, replaced by `AppChrome`/`TouchTopBar` in PR 3. Only the strip *height* is tokenized here.
- `TextField` reveal button (`h-5 w-5`) and mono-input `text-[12px]` — micro sub-controls with no matching token; left as-is.
- `Toggle`/`Pill` core track geometry — micro status controls; only their **spacing/padding** is tokenized (see Task 5). Their fixed track size is preserved to protect the desktop diff; any touch-specific enlargement, if the prototype shows one, is a small follow-up once Settings is verified at touch density in PR 5.
- The `uiMode` store, `data-mode` wiring at boot, and the Settings selector — all PR 2.

## File map

| File | Change |
|---|---|
| `tauri/src/app.css` | **Modify** — add `:root` sizing tokens + `[data-mode='touch']` overrides; body `font-size` → `var(--text-base)`. |
| `tauri/src/lib/components/Btn.svelte` | **Modify** — height/text/padding → tokens. |
| `tauri/src/lib/components/TextField.svelte` | **Modify** — height/text/padding → tokens. |
| `tauri/src/lib/components/Toggle.svelte` | **Modify** — spacing only (conservative). |
| `tauri/src/lib/components/Pill.svelte` | **Modify** — padding → token. |
| `tauri/src/lib/components/SettingsRow.svelte` | **Modify** — label text + padding → tokens. |
| `tauri/src/lib/components/WindowChrome.svelte` | **Modify** — strip height `h-10` → `var(--chrome-h)`. |
| `tauri/src/routes/+page.svelte` | **Modify** — 5 icon buttons + sidebar search field → tokens. |

All commands run from `C:\Users\akinz\Git\ludusavi-wrap\tauri` unless noted.

---

## Task 0: Capture the desktop baseline (prep)

**Purpose:** You need a "before" reference to diff against, because the guardrail is *desktop visually unchanged*.

- [ ] **Step 1: Make sure the tree is clean and on a fresh branch**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git checkout master && git pull
git checkout -b touch-mode-pr1-density-tokens
```

- [ ] **Step 2: Run the app on the current code and screenshot the reference screens**

```bash
cd tauri
bun install
bun run tauri dev
```

With the app open, capture screenshots of: the **library window** (sidebar + a selected game), the **Settings** window, and the **Add Game** popup. Save them somewhere outside the repo (e.g. `~/Desktop/pr1-baseline/`). These are your "before" images. Close the dev app when done.

- [ ] **Step 3: Confirm the checks are green before you start changing anything**

Run: `bun run check && bun run lint`
Expected: both pass with no errors. (If they don't, stop — that's a pre-existing issue to resolve first, not something this PR introduced.)

---

## Task 1: Add the density token block to `app.css`

**Files:**
- Modify: `tauri/src/app.css` (after the `@theme {…}` block, ~line 57; and body `font-size`, line 67)

This task is inert on its own — no component references the new tokens yet — so desktop cannot change. It establishes the foundation every later task consumes.

- [ ] **Step 1: Insert the sizing-token block after the `@theme` block**

In `app.css`, immediately after the closing `}` of `@theme { … }` (currently line 57) and before the `/* ── Global resets / app shell ── */` comment, insert:

```css

/* ── Density tokens (runtime mode-switched — NOT part of @theme) ──────
   These are plain custom properties, not Tailwind theme colors: they are
   *sizing* and flip at runtime via `[data-mode='touch']` on <html>.
   :root values equal the historical hardcoded numbers so desktop is
   unchanged; the touch block is a single density step up. The mode is
   resolved + applied in PR 2 (lib/uiMode.svelte.ts). */
:root {
  --text-base: 13px;       /* body + control labels */
  --text-sm: 11.5px;       /* helper / secondary */
  --text-lg: 15px;         /* section headings */
  --control-h: 32px;       /* Btn, text inputs, search field */
  --control-h-icon: 28px;  /* square icon buttons */
  --tap-min: 32px;         /* minimum hit target */
  --space-unit: 4px;       /* multiply for gaps/padding */
  --chrome-h: 40px;        /* title-bar strip height */
}

[data-mode='touch'] {
  --text-base: 16px;
  --text-sm: 13px;
  --text-lg: 19px;
  --control-h: 48px;
  --control-h-icon: 48px;
  --tap-min: 48px;
  --space-unit: 6px;
  --chrome-h: 60px;
}
```

- [ ] **Step 2: Switch the body font-size to the token**

In the `html, body { … }` rule, change line 67:

```diff
-  font-size: 13px;
+  font-size: var(--text-base);
```

- [ ] **Step 3: Verify the build still compiles and styles still load**

Run: `bun run check`
Expected: PASS (no new errors). Then `bun run tauri dev`, confirm the app looks **identical** to your Task 0 baseline (body text is still 13px — the token resolves to the same value). In devtools, run `getComputedStyle(document.body).fontSize` → expect `"13px"`; then `document.documentElement.dataset.mode = 'touch'` and re-check → expect `"16px"`. Reset with `delete document.documentElement.dataset.mode`.

- [ ] **Step 4: Commit**

```bash
git add tauri/src/app.css
git commit -m "$(cat <<'EOF'
feat(touch): add density sizing tokens + data-mode override

Plain CSS custom properties (separate from the @theme color block) for
font sizes, control heights, hit targets, spacing, and chrome height.
:root values equal today's hardcoded numbers; [data-mode='touch'] scales
them one density step. Body font-size now reads var(--text-base). Inert
until primitives consume the tokens (following tasks).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Refactor `Btn.svelte` to tokens

**Files:**
- Modify: `tauri/src/lib/components/Btn.svelte:84` (the `<button>` class + add `style:` directives)

- [ ] **Step 1: Replace the hardcoded height/text/padding on the button**

The button currently (lines 84-89) reads:

```svelte
  class="inline-flex h-8 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-sm px-3 text-[12.5px] font-medium transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-50 {full
    ? 'w-full'
    : ''} {className}"
  style:background={style.background}
  style:color={style.color}
  style:border={style.border}
```

Change it to remove `h-8`, `px-3`, and `text-[12.5px]`, and drive those from tokens via `style:` directives:

```svelte
  class="inline-flex cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-sm text-[length:var(--text-base)] font-medium transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-50 {full
    ? 'w-full'
    : ''} {className}"
  style:height="var(--control-h)"
  style:padding-inline="calc(var(--space-unit) * 3)"
  style:background={style.background}
  style:color={style.color}
  style:border={style.border}
```

(`px-3` = 12px = `--space-unit`×3 desktop; `text-[12.5px]` → `--text-base` = 13px, the intended +0.5px normalization.)

- [ ] **Step 2: Verify**

Run: `bun run check && bun run lint`
Expected: both PASS. In the dev app, a `Btn` (e.g. any button in Settings) measures 32px tall at desktop; set `data-mode='touch'` in devtools → it grows to 48px and text to 16px. Visually compare against baseline — identical bar the documented +0.5px text.

- [ ] **Step 3: Commit**

```bash
git add tauri/src/lib/components/Btn.svelte
git commit -m "$(cat <<'EOF'
refactor(touch): drive Btn size from density tokens

h-8 -> var(--control-h), px-3 -> calc(var(--space-unit)*3),
text-[12.5px] -> var(--text-base). No desktop change beyond a +0.5px
control-label normalization.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Refactor `TextField.svelte` to tokens

**Files:**
- Modify: `tauri/src/lib/components/TextField.svelte:54-58` (the wrapper `<div>`)

- [ ] **Step 1: Token-drive the field wrapper height/text/padding**

The wrapper currently (lines 54-58) reads:

```svelte
<div
  class="group inline-flex h-8 items-center gap-1 rounded-sm border bg-bg-2 px-2 text-[12.5px] transition-colors {full
    ? 'w-full'
    : ''}"
  style:border-color={focused ? 'var(--color-spool)' : 'var(--color-line-2)'}
>
```

Change to:

```svelte
<div
  class="group inline-flex items-center gap-1 rounded-sm border bg-bg-2 text-[length:var(--text-base)] transition-colors {full
    ? 'w-full'
    : ''}"
  style:height="var(--control-h)"
  style:padding-inline="calc(var(--space-unit) * 2)"
  style:border-color={focused ? 'var(--color-spool)' : 'var(--color-line-2)'}
>
```

(`h-8`→32px=`--control-h`; `px-2`→8px=`--space-unit`×2; `text-[12.5px]`→`--text-base`. Leave the inner `<input>`'s mono `text-[12px]` and the reveal button `h-5 w-5` as-is — no matching tokens, micro sub-controls, see "Out of scope".)

- [ ] **Step 2: Verify**

Run: `bun run check && bun run lint`
Expected: both PASS. In Settings, the Ludusavi-path / API-key fields measure 32px tall on desktop; `data-mode='touch'` → 48px, text 16px. Masked field's reveal eye still works.

- [ ] **Step 3: Commit**

```bash
git add tauri/src/lib/components/TextField.svelte
git commit -m "$(cat <<'EOF'
refactor(touch): drive TextField size from density tokens

Wrapper h-8 -> var(--control-h), px-2 -> calc(var(--space-unit)*2),
text-[12.5px] -> var(--text-base). Mono inner input + reveal button left
as micro sub-controls.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Tokenize `Pill.svelte` padding

**Files:**
- Modify: `tauri/src/lib/components/Pill.svelte:36` (the `<span>` class)

`Pill` is a non-interactive status label (`<span>`, no `onclick`), so it has no tap-target obligation. Only its inline padding is tokenized so it breathes proportionally at touch density; its deliberate micro typography (`text-[9.5px]` uppercase mono) and `h-[18px]` track stay fixed.

- [ ] **Step 1: Swap the horizontal padding to a token**

Line 36 currently:

```svelte
  class="font-mono inline-flex h-[18px] items-center gap-1.5 whitespace-nowrap rounded-sm px-1.5 text-[9.5px] uppercase leading-none tracking-[0.1em]"
```

Change `px-1.5` (6px) to a `--space-unit`-derived value:

```svelte
  class="font-mono inline-flex h-[18px] items-center gap-1.5 whitespace-nowrap rounded-sm text-[9.5px] uppercase leading-none tracking-[0.1em]"
  style:padding-inline="calc(var(--space-unit) * 1.5)"
```

(6px = `--space-unit`×1.5 desktop → 9px at touch. Add the `style:` directive line right after the `class` attribute, before the existing `style:background` line.)

- [ ] **Step 2: Verify**

Run: `bun run check && bun run lint`
Expected: both PASS. Sync/status pills in the library look identical on desktop; slightly roomier under `data-mode='touch'`.

- [ ] **Step 3: Commit**

```bash
git add tauri/src/lib/components/Pill.svelte
git commit -m "$(cat <<'EOF'
refactor(touch): tokenize Pill horizontal padding

px-1.5 -> calc(var(--space-unit)*1.5). Micro status label; track height
and 9.5px caption intentionally fixed.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Tokenize `Toggle.svelte` (conservative)

**Files:**
- Modify: `tauri/src/lib/components/Toggle.svelte:32-38`

`Toggle` is a small fixed-geometry switch (track `h-[18px] w-8`, thumb `size-3.5`, `translateX(15px)`). Enlarging the track would require new tokens beyond the handoff's set and would change desktop. For PR 1 the track geometry is preserved verbatim; this task is a deliberate **no-op on rendered size** and exists only to record that decision in history. Skip the edit and make an empty-bodied documentation commit, OR — preferred — leave `Toggle.svelte` untouched entirely and note it in the PR description.

- [ ] **Step 1: Decision — leave `Toggle.svelte` unchanged in PR 1**

Do not edit the file. Its track is a micro-control with no matching token; forcing `--control-h` (32px) onto an 18px track would visibly change desktop and break the diff. Touch enlargement, if the prototype's `TToggle` warrants it, is a scoped follow-up after Settings is exercised at touch density (PR 5).

- [ ] **Step 2: Verify nothing regressed**

Run: `bun run check && bun run lint`
Expected: both PASS (no change). Toggles in Settings look identical to baseline.

- [ ] **Step 3: No commit for this task** — there is no change. Record the rationale in the final PR description (Task 9).

---

## Task 6: Tokenize `SettingsRow.svelte` label + padding

**Files:**
- Modify: `tauri/src/lib/components/SettingsRow.svelte:30,32`

- [ ] **Step 1: Token-drive the row padding and label text size**

Line 30 (the grid wrapper) currently:

```svelte
<div class="grid items-start gap-[18px] px-[18px] py-3" style="grid-template-columns: 180px 1fr">
```

Change the vertical padding `py-3` (12px) to a token and keep the rest:

```svelte
<div class="grid items-start gap-[18px] px-[18px]" style="grid-template-columns: 180px 1fr; padding-block: calc(var(--space-unit) * 3)">
```

Line 32 (the label) currently:

```svelte
    <div class="flex items-center gap-1.5 text-[12.5px] font-medium text-ink-0">
```

Change the label text to the base token:

```svelte
    <div class="flex items-center gap-1.5 text-[length:var(--text-base)] font-medium text-ink-0">
```

(`py-3`=12px=`--space-unit`×3 → 18px touch; label `12.5px`→`--text-base`. Leave the `gap-[18px]`, `px-[18px]`, the 180px column, and the `text-[11px]` helper as-is — those tune in a later Settings-density pass if needed; PR 1 keeps the desktop grid intact.)

- [ ] **Step 2: Verify**

Run: `bun run check && bun run lint`
Expected: both PASS. Settings rows look identical on desktop (bar the +0.5px label); rows gain vertical breathing room under `data-mode='touch'`.

- [ ] **Step 3: Commit**

```bash
git add tauri/src/lib/components/SettingsRow.svelte
git commit -m "$(cat <<'EOF'
refactor(touch): tokenize SettingsRow padding + label text

py-3 -> padding-block calc(var(--space-unit)*3), label text-[12.5px] ->
var(--text-base). Grid columns and helper text unchanged.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Tokenize the `WindowChrome.svelte` strip height

**Files:**
- Modify: `tauri/src/lib/components/WindowChrome.svelte:45-48`

The handoff's `--chrome-h` token (desktop 40px) equals the current strip height `h-10` exactly. Tokenizing it now is desktop-identical and prepares PR 3's `AppChrome`. The min/max/close buttons (`h-full w-12`) are desktop-only chrome — left as-is.

- [ ] **Step 1: Replace the strip height with the token**

The strip `<div>` (lines 45-48) currently:

```svelte
<div
  data-tauri-drag-region="deep"
  class="flex h-10 shrink-0 items-center gap-3 border-b border-line-1 bg-black/30 pl-3.5"
>
```

Change `h-10` to a token-driven height:

```svelte
<div
  data-tauri-drag-region="deep"
  class="flex shrink-0 items-center gap-3 border-b border-line-1 bg-black/30 pl-3.5"
  style:height="var(--chrome-h)"
>
```

- [ ] **Step 2: Verify**

Run: `bun run check && bun run lint`
Expected: both PASS. The title bar is still 40px on desktop (`getComputedStyle($0).height` on the strip → `"40px"`); `data-mode='touch'` → 60px. Min/max/close still click.

- [ ] **Step 3: Commit**

```bash
git add tauri/src/lib/components/WindowChrome.svelte
git commit -m "$(cat <<'EOF'
refactor(touch): drive WindowChrome strip height from --chrome-h

h-10 (40px) -> var(--chrome-h). Desktop-identical; window-control
buttons left for the AppChrome split in PR 3.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Tokenize the library top-bar icon buttons + sidebar search field

**Files:**
- Modify: `tauri/src/routes/+page.svelte` — icon buttons at lines 611, 634, 653, 670, 832; sidebar search field at lines 909-915

There are five square `h-7 w-7` (28px) icon buttons and one `h-[30px]` search field. `--control-h-icon` desktop = 28px (exact match for the icon buttons); `--control-h` desktop = 32px (the intended +2px search normalization).

- [ ] **Step 1: Tokenize the four top-bar icon buttons (lines 611, 634, 653, 670)**

Each of these four buttons has `h-7 w-7` in its class. The exact class strings differ slightly per button; the change is the same for all four — **remove `h-7 w-7` from the class and add a `style:` block sizing it from the token.** For each button, apply:

- Delete the substring `h-7 w-7 ` from its `class="…"`.
- Add these two directives to the element (alongside its existing attributes/handlers):

```svelte
  style:height="var(--control-h-icon)"
  style:width="var(--control-h-icon)"
```

Concretely, line 611 currently:

```svelte
          class="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
```

becomes:

```svelte
          class="inline-flex cursor-pointer items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
          style:height="var(--control-h-icon)"
          style:width="var(--control-h-icon)"
```

Apply the identical `h-7 w-7` → `style:height/width` treatment to lines 634, 653, and 670 (their other classes — `relative`, `border-none bg-transparent`, etc. — stay untouched).

- [ ] **Step 2: Tokenize the delete icon button (line 832)**

Line 832 currently:

```svelte
                    class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-2 transition-colors hover:border-bad/60 hover:text-bad"
```

becomes (drop `h-7 w-7`, add the directives):

```svelte
                    class="inline-flex shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-2 transition-colors hover:border-bad/60 hover:text-bad"
                    style:height="var(--control-h-icon)"
                    style:width="var(--control-h-icon)"
```

- [ ] **Step 3: Tokenize the sidebar search field (lines 909-915)**

The search wrapper at line 909 currently:

```svelte
          class="flex h-[30px] items-center gap-2 rounded-sm border border-line-1 bg-bg-2 px-2.5"
```

becomes (drop `h-[30px]`, drive height from `--control-h`; this is the intended +2px normalization):

```svelte
          class="flex items-center gap-2 rounded-sm border border-line-1 bg-bg-2 px-2.5"
          style:height="var(--control-h)"
```

Then the inner `<input>` text at line 915:

```svelte
            class="font-sans min-w-0 flex-1 bg-transparent text-[12.5px] text-ink-0 outline-none placeholder:text-ink-3"
```

becomes (text-[12.5px] → token):

```svelte
            class="font-sans min-w-0 flex-1 bg-transparent text-[length:var(--text-base)] text-ink-0 outline-none placeholder:text-ink-3"
```

- [ ] **Step 4: Verify**

Run: `bun run check && bun run lint`
Expected: both PASS. In the dev app: the Browse/sync/add/settings icon buttons in the top bar are 28px on desktop and 48px under `data-mode='touch'`; the sidebar search field is now 32px (was 30px — expected) on desktop and 48px at touch. Type in search → filtering still works. Compare against the Task 0 baseline: identical apart from the documented +2px search height and +0.5px text.

- [ ] **Step 5: Commit**

```bash
git add tauri/src/routes/+page.svelte
git commit -m "$(cat <<'EOF'
refactor(touch): tokenize library icon buttons + sidebar search

Five h-7 w-7 icon buttons -> var(--control-h-icon); sidebar search
h-[30px] -> var(--control-h) (+2px desktop, intended) and its text ->
var(--text-base).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Full-app verification + PR

**Files:** none (verification + PR creation)

- [ ] **Step 1: Run the full check + lint suite once more**

Run: `bun run check && bun run lint`
Expected: both PASS.

- [ ] **Step 2: Desktop screenshot diff against the Task 0 baseline**

`bun run tauri dev`. With `data-mode` unset (desktop), screenshot the same three screens (library, Settings, Add Game) and compare to your Task 0 baseline. Expected: **visually indistinguishable** except the two documented deltas — control text +0.5px and the sidebar search field +2px taller. Anything else is a regression to investigate before merging.

- [ ] **Step 3: Touch-density smoke test at both target resolutions**

In devtools run `document.documentElement.dataset.mode = 'touch'`. Walk every primary control: buttons, text fields, icon buttons, the search field, and Settings rows. Every interactive control should now be **≥48px** in its tappable dimension. Resize the window toward 1280×800 (Steam Deck) and 1920×1080 (ROG Ally) and confirm the controls stay large and nothing clips. Reset with `delete document.documentElement.dataset.mode`. (The library *layout* is still the desktop two-pane here — that is correct; the touch layout is PR 5.)

- [ ] **Step 4: Guardrail self-check (handoff §1)**

Confirm before opening the PR:
- No `*Touch.svelte` files were created. ✅ (none in this PR)
- No component reads `uiMode` / sniffs `window.innerWidth` / touch APIs. ✅ (PR 1 adds no mode awareness)
- Density lives only in CSS variables. ✅
- Every changed screen renders acceptably at both `data-mode` values. ✅ (Step 3)

- [ ] **Step 5: Push and open the PR**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git push -u origin touch-mode-pr1-density-tokens
gh pr create --title "Touch mode PR 1 — density tokens + primitive refactor" --body "$(cat <<'EOF'
First PR of the adaptive touch-mode rollout (design_handoff_touch_mode/).

Adds runtime density tokens (:root desktop values == today's numbers;
[data-mode='touch'] one step up) and rewires the primitives (Btn,
TextField, Pill, SettingsRow), the chrome strip height, the library
top-bar icon buttons, and the sidebar search field to consume them.

No mode awareness yet (PR 2 wires data-mode at boot + the Settings
selector). Desktop is visually unchanged apart from two intended
single-token normalizations: control label text +0.5px (12.5 -> 13px)
and the sidebar search field +2px (30 -> 32px).

Toggle is intentionally left unchanged — a micro-control with no matching
token; touch enlargement is a scoped follow-up after PR 5. tokens.ts is
untouched (it mirrors colors only).

Verified: bun run check + bun run lint pass; desktop screenshot diff vs
master shows only the two documented deltas; with data-mode='touch' set
manually, all primary controls reach >=48px at 1280x800 and 1920x1080.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-review notes (already reconciled)

- **Spec coverage:** every checkbox in `TASKS.md` PR 1 is covered — token block (Task 1), body font (Task 1), `Btn` (Task 2), icon buttons in `+page.svelte`/`WindowChrome` (Tasks 7-8), `TextField`/`Toggle`/`Pill`/`SettingsRow`/sidebar search (Tasks 3-6, 8), screenshot diff (Task 9). The one place this plan refines the handoff: it does **not** force tokens onto `Toggle`'s 18px track (no matching token; would break the desktop diff) and explicitly documents the +0.5px / +2px deltas the handoff's single-token mapping implies — so the "pixel-identical" verification has a precise, honest pass criterion.
- **Type/name consistency:** token names (`--text-base`, `--text-sm`, `--text-lg`, `--control-h`, `--control-h-icon`, `--tap-min`, `--space-unit`, `--chrome-h`) are used identically everywhere they appear and match the handoff's §3a and the README density table.
- **No placeholders:** every code step shows the exact before/after strings from the current files.
