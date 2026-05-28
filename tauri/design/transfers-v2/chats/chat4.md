# Touch-Friendly Windows

_Started 2026-05-26 04:57 UTC_

---

## User

can we modify our main windows to be more touch friendly when we detect a touchscreen. im thinking bigger buttons and maybe an increased text size, do you have thoughts?

## Assistant

_[tool: list_files]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: list_files]_

_[tool: list_files]_

_[tool: read_file]_

Yeah, I have a bunch of thoughts on this — it's a great fit for Spool given how much of its audience is on Steam Decks, Allys, and Legion Gos. A few things I'd push back on or extend before we just scale everything up:

**On detection.** Pure auto-detect is risky. Handhelds almost always report *both* touch and fine pointer (most have a trackpad nub or a docked mouse), so `pointer: coarse` alone will misfire. I'd actually do three things:
1. Auto-detect as the default signal (`(pointer: coarse) and (hover: none)`, plus a runtime "last input was touch" listener that flips the mode when the user actually taps).
2. A manual override in Settings: **Touch Mode: Auto / On / Off**. This is what Windows itself does with Tablet Mode and it's the right escape hatch.
3. Remember the choice per-device (the same library.json profile already exists).

**On what actually changes.** "Bigger buttons + bigger text" is the obvious part but it's not the interesting part. The things that *really* hurt on a handheld today:
- The toolbar icon buttons (settings cog, browse games, wifi) are ~32px — finger-hostile. Bump to 48px hit targets.
- **Right-click menus.** Long-press is a fine fallback but undiscoverable. Each game card should grow a visible "⋯" overflow button in touch mode so shortcuts/folder/etc. are tappable.
- **Card spacing.** Going from 180×265 desktop to ~220×320 with more gap matters more than going from 14px → 16px text. Fat fingers need *between-element* space, not just bigger targets.
- **Sidebar nav** in Library should get taller rows.
- **Comboboxes/search field in Add Game.** Those small dropdown rows are misery on a finger. Bump row height to 56px.
- **Scrollbars.** Wider thumb, or hide them entirely in touch mode and rely on flick scrolling.

**What I'd NOT change.**
- Don't bloat font sizes beyond ~17px body / ~22px heading. Handheld screens are pixel-dense but physically tiny — going too big means less content per screen, which on a 7" display is a real cost.
- Don't add a separate "Touch Layout." Just one layout with a density variable.

**Suggested approach for mocking.** I'd build a side-by-side `design-canvas` showing **Library** and **Add Game** in Desktop vs. Touch mode, with the Tweak panel exposing a `Touch Mode` toggle plus a density slider so you can dial in the bump that feels right. Want me to do that? Or would you rather I add a runtime toggle to the existing Library window so you can flip between them in-place?

## User

ok yeah do a canvas

## Assistant

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: grep]_

_[tool: grep]_

_[tool: grep]_

Let me peek at the icons and brand mark so I know what's available, then I'll start building.

_[tool: grep]_

_[tool: update_todos]_

Solid. I've got enough context — let me build. Plan:

- `touch/tokens.js` — desktop vs. touch density tokens
- `touch/library-touch.jsx` — tokens-driven Library window 
- `touch/addgame-touch.jsx` — tokens-driven Add Game dialog
- `Touch Mode.html` — design canvas with side-by-side comparisons + a scale ruler + the Settings entry

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

## Assistant

_[tool: write_file]_

_[tool: write_file]_

_[tool: update_todos]_

_[tool: done]_

Clean. Let me hand off to the verifier.

_[tool: fork_verifier_agent]_

Done. Touch Mode canvas is up — four sections to compare:

1. **Library** desktop vs touch at 1240×800 — bigger title bar, taller sidebar rows, bigger Play CTA, more breathing room, larger toggles.
2. **Add Game** dialog desktop vs touch — radio rows go from 48→64px, dedicated tap target for the expand chevron (split off from the row so they don't fight), bigger confidence bars.
3. **Density tokens ruler** — every measurement that changes, with the % delta on each. The whole system runs off one tokens object so the mode flip is genuinely one variable.
4. **Settings entry mockup** — segmented "Auto · On · Off" control with auto-detection hint, since (as discussed) handhelds need the override.

A few open calls I made you should sanity-check:
- **Body type** only goes 13→15px, not 16. Steam Deck screens are physically small; bigger type means less library on screen.
- **Sidebar widens** 320→380 to fit the taller thumbnails (44px vs 30px) comfortably.
- **Hero grows taller** (300→340) but title size 32→40, not bigger — the Play button does most of the touch-friendliness work.
- **Hover-only chevron** in candidate rows is now a dedicated 52×64 tap target with its own button.

If you want me to push further (e.g. a fully restyled compact-touch variant for the Steam Deck where the whole window goes 1280×720 fullscreen with a controller-friendly nav scheme) — that's a bigger lift but worth a follow-up canvas.

