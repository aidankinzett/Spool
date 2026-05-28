# Add Game Popup

_Started 2026-05-26 01:11 UTC_

---

## User

can we redesign the add game popup, currently its a bit unclear that you need to search for the game name to get ludusavi to work.

## Assistant

_[tool: list_files]_

_[tool: local_ls]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: list_files]_

_[tool: list_files]_

_[tool: list_files]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: view_image]_

_[tool: view_image]_

_[tool: view_image]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: copy_starter_component]_

_[tool: read_file]_

I'll redesign the Add Game popup with several variations. Looking at the screenshots, the core UX problem is clear: three input fields look equivalent, the "Search" button reads as an optional action rather than required, and there's no signal that "Game Name" must match ludusavi's database (otherwise save backup silently fails).

Let me build a design canvas with 4 takes that all solve this differently, then you can pick. I'll reuse your existing Fluent dark system (the same TextField/Button/Toggle look as Library + Settings).

_[tool: write_file]_

Now let me build the 4 variations. I'll write them as compact JSX modules.

_[tool: write_file]_

Now the 4 variations. Variation 1 — search-first reorder (clear priority):

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

Now the baseline reproduction (so you can compare side-by-side) and the host HTML:

_[tool: read_file]_

_[tool: grep]_

_[tool: grep]_

I need to use the children-based API. Let me rewrite the host:

_[tool: write_file]_

_[tool: done]_

_[tool: fork_verifier_agent]_

Here are 4 redesigns of the Add Game dialog laid out on a canvas for comparison, all in your existing dark Fluent system:

- **V0 — Current** — baseline for reference
- **V1 — Identify-first** — minimal change: Game Name moves to top, marked **Required**, with a "matched to ludusavi" chip and an inline explainer
- **V2 — Live combobox** — single search field, no separate Search button; ludusavi results drop down as you type with "best match" + save-file counts
- **V3 — Wizard** — splits into two steps so step 1 is *only* picking the game; can't skip it
- **V4 — Drop the .exe** — flips the flow: drop an executable, Spool runs the filename through ludusavi and shows ranked candidates with confidence scores

My picks: **V2** if you want minimal disruption (one new dropdown, no new screens), **V4** if you're up for genuinely changing the mental model — it makes Spool feel smart rather than asking the user to do the matching by hand.Let me know which direction resonates, or if you want me to push one further (e.g. fold V2's combobox idea into V4's auto-detect, or strip V3 down to a single screen with a more aggressive search prompt).

## User

v4 looks good

## Assistant

Want me to take V4 further? A few directions I'd consider:

1. **Build the missing states** — empty drop-zone, "no match found → manual search" fallback, and confirmation toast when added. Right now you only see the happy path.
2. **Polish details** — drag-hover affordance, a tiny per-candidate preview of *what* saves get tracked, and the executable card showing detected install size / version.
3. **Make it the canonical file** — pull V4 out of the canvas into a standalone `Add Game.html` (the dialog centered on a desktop) so it's the working spec.

Or all three. Which do you want?

## User

yeah ok all three please

## Assistant

_[tool: copy_files]_

Let me move the exploration canvas to a separate file, then build the polished V4 with all states.

_[tool: write_file]_

Now the canonical Add Game.html, framed in a desktop:

_[tool: write_file]_

_[tool: done]_

_[tool: save_screenshot]_

_[tool: view_image]_

_[tool: multi_screenshot]_

Looking clean. Let me spot-check the other key states:

_[tool: fork_verifier_agent]_

The dialog is in place. Quick rundown of what landed:

- **`Add Game.html`** — the canonical V4, centered on a faintly-blurred desktop with a ghosted Library window behind it
- **`Add Game — Explorations.html`** — preserves the original 5-up canvas (V0 through V4)
- **`addgame/final.jsx`** — polished V4 with every state wired up

The states are reachable via the Tweaks toggle in the toolbar — flip **Demo → State** through:

1. **Empty** — drop-zone CTA with browse link
2. **Dragging** — accent glow under the cursor while a file's held over it
3. **Detecting** — spinner card while ludusavi looks it up
4. **Multi-match** — 3 ranked candidates with confidence bars; click the chevron on any row to **preview the actual save paths** ludusavi will track
5. **Single match** — high-confidence hit auto-confirms inline with "Preview saves" and "Show 2 other matches" affordance
6. **No match** — amber-tinged warning + prominent ludusavi search + escape hatch ("Add anyway without save management")

Each match row now shows the EXE detected (path · size · version · arch) and per-candidate save-path preview, so the user sees *what* gets backed up before they commit.Toggle Tweaks on and step through the demo states to see them all — if any state needs more work, point at it.

