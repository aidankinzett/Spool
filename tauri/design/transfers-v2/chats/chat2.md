# App Rebranding

_Started 2026-05-25 10:57 UTC_

---

## User

can we explore some new branding for this app? it has grown a bit from a simple wrapper around ludusavi. we also need an icon for it

## Assistant

_[tool: list_files]_

_[tool: local_ls]_

_[tool: local_read]_

_[tool: local_read]_

_[tool: read_file]_

_[tool: questions_v2]_

I read through the README and CLAUDE.md — got a good picture of how the app has grown. It's no longer just a ludusavi wrapper: it's a full personal game library with cover art, LAN sharing, cross-device sync/locking, TorBox downloads, and shortcut generation for Armoury Crate / Steam. Before I start exploring directions, a few questions:

## User

Questions answered:
- name_direction: Want a totally new name — explore options
- core_identity: Sleek, modern, Windows 11-native, Save-keeper, archive, preservation, Network/peer-to-peer sharing focus, Personal game shelf / collection, Handheld-first launcher (Steam Deck / ROG Ally vibes)
- audience: Open-source community at large — handheld PC owners
- visual_temperature: Minimal/neutral — let the cover art speak
- icon_style: Abstract mark / monogram (letter or geometric), Should look good on Steam Deck / Armoury Crate tile, Plays well as a tiny taskbar/16px icon, Should feel at home in Windows 11 dock
- color_direction: Pick up Windows 11 accent color (no fixed brand color)
- options_count: Decide for me
- deliverable_format: Decide for me

## Assistant

_[tool: copy_starter_component]_

_[tool: list_files]_

_[tool: list_files]_

Got it. I'll explore **5 directions**, each with a distinct facet of what this app does, presented on a single design canvas so you can compare side-by-side.

Plan:
- **Keep** — preservation/save-keeper angle (most literal)
- **Trove** — personal collection / treasure
- **Spool** — save-state + cross-device tape metaphor
- **Beacon** — LAN discovery / network-first
- **Cairn** — quiet preservation marker (most abstract)

For each: a name + tagline, app icon at large/medium/tiny sizes, a tiny color note, and a slice of the Library applied with the new mark. Chrome stays neutral so the Win11 accent does the coloring, per your direction.

_[tool: read_file]_

_[tool: read_file]_

_[tool: write_file]_

Building the canvas now. One quick refinement: swapping "Keep" → "Hold" since Google Keep would collide. Final five: **Hold**, **Trove**, **Spool**, **Beacon**, **Cairn**.

_[tool: write_file]_

_[tool: write_file]_

_[tool: grep]_

_[tool: str_replace_edit]_

_[tool: write_file]_

_[tool: done]_

_[tool: list_files]_

_[tool: done]_

_[tool: read_file]_

_[tool: copy_files]_

_[tool: delete_file]_

_[tool: str_replace_edit]_

_[tool: done]_

_[tool: fork_verifier_agent]_

Five branding directions are laid out on a single canvas in **Branding.html** — pan/zoom and drag artboards to compare.

- **Hold** — preservation/vault angle (most utilitarian)
- **Trove** — collector treasure (warmest)
- **Spool** — save-state tape metaphor (most technical)
- **Beacon** — LAN/discovery (network-forward)
- **Cairn** — abstract marker stack (most distinctive)

Each direction has an **Identity** card (wordmark, tagline, voice samples), a **Mark studies** card (140px tile → 56 → 32 → 16, light + handheld-dark contexts, plus the same mark filled with four Windows 11 system accents), and an **In context** card (a slice of the Library titlebar + game cards with the mark applied).

The whole system stays neutral graphite/paper with a single-color monogram so cover art keeps the spotlight, and the icon picks up the Win11 accent rather than fighting it.Tell me which directions resonate (or none of them — I can iterate on tone or push further on a specific facet). Once we pick a direction, next steps would be a proper icon export (.ico with all the Windows tray sizes) and applying the wordmark across Library + Settings.

## User

I like spool, it looks a little like the underground logo, and other logo ideas there?

## Assistant

_[tool: str_replace_edit]_

Going to add a focused "Spool — mark studies" section with 8 variants on the underground-roundel theme, so we can find the sweet spot.

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

Now wire the new section into the canvas:

_[tool: str_replace_edit]_

_[tool: done]_

Added a **Spool · alternate marks** section at the bottom of the canvas with eight variations on the reel theme:

1. **Roundel** — the original (your underground-like one)
2. **Reel-to-reel** — two reels linked by tape; the most literal "spooling between devices"
3. **Cassette** — cassette window with two visible reel hubs; nostalgic save-tape
4. **Concentric** — top-down wound tape; archive-y, no horizontal cut
5. **Unspooling** — reel with a tape tail trailing off; suggests motion / transfer in progress
6. **Tape-S** — a lowercase 's' drawn as one ribbon; doubles as monogram
7. **Bobbin** — side-on thread bobbin between two pegs; hand-crafted feel
8. **Beads** — three dots on a line; the most minimal abstraction

Each shows up on a tile, in the **Tiny-size test** card (32 / 20 / 16 px plus a taskbar mockup so you can sanity-check legibility at small sizes), and across the four Win11 accent colors.

_[tool: fork_verifier_agent]_

My hunch is Reel-to-reel or Roundel for impact at 16px — but tell me which clicks and we'll lock the name + mark in and apply it across the Library and Settings.

## User

i like reel to reel or cassette I think. just thinking with reel to reel, it usually comes out at the bottom to go to the other reel. i dont know. i do like it though

## Assistant

_[tool: str_replace_edit]_

Good catch — anatomically, tape exits the *bottom* of each reel and runs along a lower path (often past a read head). Let me iterate on that.

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

_[tool: done]_

Added a **Reel-to-reel & Cassette · refinements** section at the bottom. Ten takes total — the two originals (marked "orig") plus eight new ones:

- **Underslung** — tape leaves the bottom of each reel and runs along a lower U-path (anatomically correct)
- **With head** — underslung path + a tiny read-head pickup at center
- **Sagging** — single curve drooping between reels, looser/more organic
- **Asymmetric** — left reel filled, right reel outlined; implies direction of transfer
- **Cassette · tape** — cassette shell with two hubs + a thin tape strip exposed at the bottom
- **Cassette · band** — cassette with a top label band
- **Cassette · spokes** — most detailed; spoked reels visible through the shell
- **Cassette · tilt** — slightly rotated for character

Tiny-size test card stacks all ten at 32 / 20 / 16 next to a taskbar mock so the bottom-tape detail is read against actual handheld/Windows scale. Lockup card shows three favorites paired with the wordmark.

_[tool: fork_verifier_agent]_

My pick of the bunch: **Underslung** for the cleanest reel-to-reel read, or **Cassette · tape** if you want the strip-of-tape detail to add character. Let me know which lands.

## User

yeah i like cassette tape

## Assistant

_[tool: read_file]_

_[tool: read_file]_

Locking in: **Spool**, with the cassette-tape mark. Now let me apply it across the actual Library and Settings screens so we can see it in context.

_[tool: write_file]_

Now I'll create a shared `SpoolMark` component and swap it into both title bars.Now wire it into Library and Settings, and replace the placeholder gradient logo + old name in both title bars:

_[tool: str_replace_edit]_

_[tool: read_file]_

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

Now swap the title bar brand chrome in both apps:

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

_[tool: grep]_

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

_[tool: done]_

Spool is now live in both apps — the cassette mark + wordmark replace the old gradient square + "Ludusavi Wrap" in the Library title bar, the Settings title bar reads "Spool — Settings", and the About row + LAN-share copy reference Spool too.

_[tool: fork_verifier_agent]_

Open to next steps when you are — possibilities include:
- An About / splash treatment for the app (where the mark gets to be big)
- A SteamGridDB-style hero banner / "logo" version (mark + wordmark horizontal lockup for Steam shortcuts)
- A proper `.ico` export with all the Windows sizes baked in
- README header refresh

## User

Verifier agent check completed

## Assistant

All checks green. Ready when you are — say the word on which next step (About splash, hero lockup, `.ico` export, README, or something else) and I'll dive in.

## User

ico export, readme sounds good

## Assistant

_[tool: update_todos]_

_[tool: write_file]_

_[tool: write_file]_

Doing both. I'll:
1. Write standalone `Spool.svg` files (transparent for README, plus a tiled version that matches the app icon look)
2. Rasterize the mark at every Windows icon size and pack them into a real `Spool.ico` binary
3. Rewrite `README.md` for the rebrand

_[tool: run_script]_

_[tool: run_script]_

_[tool: run_script]_

_[tool: run_script]_

_[tool: view_image]_

_[tool: view_image]_

_[tool: view_image]_

_[tool: update_todos]_

_[tool: write_file]_

16 and 256 look clean. Now the README:

_[tool: update_todos]_

_[tool: register_assets]_

_[tool: present_fs_item_for_download]_

Both done. `brand/Spool.ico` packs 9 sizes (16, 20, 24, 32, 40, 48, 64, 128, 256) — small sizes use thicker strokes so the cassette geometry stays legible at tray scale. Drop it next to your existing `ludusavi-wrap.ico` and point `LudusaviWrap.csproj`'s `<ApplicationIcon>` at it. **`README.md`** is rewritten with a centered tile + new tagline, with paths/installer names rewritten and the Ludusavi heritage acknowledged up front.

## User

can we redesign the add game popup, currently its a bit unclear that you need to search for the game name to get ludusavi to work.

