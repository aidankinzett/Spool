<p align="center">
  <img src="brand/Spool-256.png" alt="Spool" width="128" height="128">
</p>

<h1 align="center">Spool</h1>

<p align="center">
  A game library that keeps your saves in sync between your Steam Deck and your PC.
</p>

<!-- SCREENSHOT: main library window (desktop layout) — the hero shot for the top of the README -->

---

Spool is a game library and launcher for Windows and Linux handhelds. It launches
your games, and around each session it restores your saves before play and backs
them up on exit — using [ludusavi](https://github.com/mtkennerly/ludusavi) under
the hood. The point is to make moving between a desktop and a Steam Deck painless:
your saves follow you, and you can copy game installs straight across your network.

## What it's good at

### Reliable save sync between Steam Deck and PC

Spool wraps [ludusavi](https://github.com/mtkennerly/ludusavi) to back up and
restore saves automatically around every play session, and syncs those backups
through any cloud remote ([rclone](https://rclone.org/) is bundled — Google Drive,
Dropbox, OneDrive, WebDAV, and more). The freshest save is pulled before launch and
pushed after you quit, so you can stop on the Deck and pick up on the PC without
overwriting yourself. If both sides changed, Spool shows a conflict picker instead
of guessing.

<!-- SCREENSHOT: cloud save conflict picker (CloudConflictModal) -->

### LAN transfers between devices

Copy game installs directly between machines on your network — no internet, no
re-downloading from the store. Enable LAN sharing and your devices find each other
automatically; a peer's games show up in your library, and you pick one to transfer.
This makes getting a game onto the Steam Deck much faster than downloading it again.
Transfers verify every file, resume if interrupted, and add the game to your library
automatically when done.

<!-- SCREENSHOT: LAN tab / peer games available to transfer, with a transfer in progress -->

### Decky plugin for Game Mode

A companion [Decky Loader](https://github.com/SteamDeckHomebrew/decky-loader)
plugin brings Spool into SteamOS Game Mode, so you don't have to drop to Desktop
Mode. From the Quick Access menu you can browse and launch your library, start LAN
transfers, and see cross-device playtime on Steam's own game pages. It also backs up
your saves if Steam force-closes Spool when you quit a game, so a session is never
lost. Install it in one click from Spool's settings.

<!-- SCREENSHOT: Decky plugin in the Steam Deck Quick Access menu — library/launch view -->
<!-- SCREENSHOT: Decky plugin LAN browsing view -->

## Download

Grab the latest build from the [Releases](../../releases) page:

* **Windows** — the `Spool_<version>_x64-setup.exe` installer.
* **Linux** — the `Spool_<version>_amd64.AppImage`. Mark it executable
  (`chmod +x`) and run it. It's patched to run on Wayland sessions with modern
  Mesa (Bazzite, CachyOS, SteamOS, recent Fedora).

Both platforms auto-update in place.

## Platform support

Spool runs on **Windows** and **Linux**, including the gaming-handheld distros
(Bazzite, CachyOS, SteamOS / Steam Deck). The library, save sync, LAN sharing, and
cloud sync work on both. A few extras are platform-specific: the Linux build runs
Windows `.exe` games through **Proton** and adds the SteamOS Game-Mode splash and
Decky plugin; the Windows build adds run-as-administrator and Armoury Crate launcher
generation.

## Documentation

User guides, the full feature list, the Decky plugin docs, and developer/architecture
documentation live at **[the Spool docs site](https://aidankinzett.github.io/Spool/)**.
Start with [Getting Started](https://aidankinzett.github.io/Spool/guides/getting-started/)
to build from source.

Spool is built with [Tauri 2](https://v2.tauri.app/) (Rust) and
[SvelteKit 5](https://kit.svelte.dev/). Saves themselves are handled by
[ludusavi](https://github.com/mtkennerly/ludusavi).

## License

See [LICENSE](LICENSE).
</content>
</invoke>
