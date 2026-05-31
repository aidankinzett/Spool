<p align="center">
  <img src="brand/Spool-256.png" alt="Spool" width="128" height="128">
</p>

<h1 align="center">Spool</h1>

<p align="center">
  A game library and save manager for handheld PCs and desktops — Windows and Linux.<br/>
  Cover-art shelf · Proton-powered Windows games on Linux · cloud + LAN save sync · cross-device play locks.<br/>
  <em>Saves themselves are handled by <a href="https://github.com/mtkennerly/ludusavi">ludusavi</a>.</em>
</p>

---

Spool started life as a thin wrapper around [ludusavi](https://github.com/mtkennerly/ludusavi) — restore saves before a game launches, back them up on exit. It has since grown into a full personal game shelf: cover art from SteamGridDB, your saves synced to any cloud provider (or your own server), LAN game-sharing between devices, cross-device play-state locks, Proton-powered launching of Windows games on Linux, and one-click launcher shortcuts for Steam and ASUS Armoury Crate.

Built with [Tauri 2](https://v2.tauri.app/) (Rust backend) and [SvelteKit 5](https://kit.svelte.dev/) — a small native binary, instant startup, and a webview-rendered UI that adapts between a desktop layout and a big-target **touch layout** for handhelds.

### Platform support

Spool runs on **Windows** and **Linux**, including the gaming-handheld distros — **Bazzite**, **CachyOS**, and **SteamOS** (Steam Deck). Both are primary targets: the Linux build adds a Proton runner for Windows games, SteamOS Game-Mode integration, and a Steam Deck backup plugin. A couple of OS-integration extras are platform-specific and simply don't appear where they don't apply:

| | Windows | Linux |
|---|:---:|:---:|
| Game library, playtime, cover art | ✅ | ✅ |
| Save restore / backup (ludusavi) | ✅ | ✅ |
| Cloud save sync (rclone) | ✅ | ✅ |
| Cross-device play-state lock | ✅ | ✅ |
| LAN game sharing | ✅ | ✅ |
| Add to Steam (non-Steam shortcut) | ✅ | ✅ |
| Run Windows games via **Proton** | — | ✅ |
| **Run as administrator** | ✅ | — |
| **Armoury Crate** launcher generation | ✅ | — |
| SteamOS Game-Mode splash + **Decky backup plugin** | — | ✅ |

---

## Table of Contents

- [Download](#download)
- [Requirements](#requirements)
- [Usage](#usage)
  - [First launch](#first-launch)
  - [Adding games to your library](#adding-games-to-your-library)
  - [Playing a game](#playing-a-game)
  - [Running Windows games on Linux (Proton)](#running-windows-games-on-linux-proton)
  - [Steam Deck & SteamOS Game Mode](#steam-deck--steamos-game-mode)
  - [Generating shortcuts](#generating-shortcuts)
- [Features](#features)
  - [Game Library](#game-library)
  - [Touch & desktop layouts](#touch--desktop-layouts)
  - [Playtime tracking](#playtime-tracking)
  - [Cloud save sync](#cloud-save-sync)
  - [Cross-device play-state lock](#cross-device-play-state-lock)
  - [LAN game sharing](#lan-game-sharing)
  - [Launch splash & desktop notifications](#launch-splash--desktop-notifications)
  - [Auto-update](#auto-update)
- [Settings](#settings)
- [Self-hosted server](#self-hosted-server)
- [Building from Source](#building-from-source)
- [Brand assets](#brand-assets)

---

## Download

Grab the latest build from the [Releases](../../releases) page:

* **Windows** — the `Spool_<version>_x64-setup.exe` installer (NSIS).
* **Linux** — the `Spool_<version>_amd64.AppImage`. Mark it executable (`chmod +x`) and run it; it's patched to run on Wayland sessions with modern Mesa (Bazzite, CachyOS, SteamOS, recent Fedora).

Both platforms **auto-update in place** — Spool checks for new releases and offers a one-click update.

## Requirements

Spool **bundles ludusavi and rclone** — you don't need to install them separately. Out of the box you only need the app itself.

Optional / platform extras:

* **Linux + Windows games** — to launch Windows `.exe` games on Linux you need [**umu-launcher**](https://github.com/Open-Wine-Components/umu-launcher) (`umu-run`) plus a Proton build. Most handheld distros (Bazzite, SteamOS, CachyOS) ship these, or any Steam-installed Proton / GE-Proton is auto-detected. Settings → **Compatibility** has a dependency doctor that checks what's present and gives per-distro install hints.
* **Cover art** — a free [SteamGridDB API key](https://www.steamgriddb.com/profile/preferences/api) enables automatic cover/art downloads.
* **Cloud save sync** — works with any rclone-supported remote (Google Drive, OneDrive, Dropbox, Box, FTP, SMB, WebDAV…) or your own [self-hosted server](#self-hosted-server). The bundled rclone does the transferring.
* **Cross-device play locks** — require a [self-hosted server](#self-hosted-server) (a tiny container you run on a Pi or home server).

## Usage

### First launch

Spool runs as a **tray-resident app**: closing the window hides it to the system tray rather than quitting (quit from the tray menu). On first run it sets up its own ludusavi configuration automatically — there's nothing to point at. Open **Settings** (gear icon) to add your SteamGridDB key, configure cloud saves, or enable LAN sharing.

### Adding games to your library

1. Click **Add Game**.
2. **Drop the game's executable** onto the dialog, or browse for one.
3. Spool queries ludusavi to auto-identify the game and shows a ranked list of candidates with confidence scores. A single strong match is pre-selected; otherwise pick the right one.
4. If auto-detection finds nothing, type a name to **search manually**.
5. Click **Add as &lt;name&gt;** (or **Add without save tracking** if it's not a ludusavi-known title). Cover art is fetched automatically from SteamGridDB when configured.

### Playing a game

Select a game and click **Play**. Spool runs a five-phase workflow:

1. **Restore** your saves via ludusavi — pulling the latest from the cloud first if cloud sync is on, and surfacing a [conflict picker](#cloud-save-sync) if both sides changed.
2. **Acquire a play-state lock** (if a server is configured) so no other device can launch the same game at once.
3. **Launch** the game and wait for it to exit (via Proton on Linux for Windows games).
4. **Back up** your saves on exit and push them to the cloud.
5. **Update** the save-sync status and accumulated playtime.

On the desktop, Spool hides its window during play and reports progress through **native OS notifications**. On a Steam Deck in Game Mode it shows a full-screen [launch splash](#launch-splash--desktop-notifications) instead.

### Running Windows games on Linux (Proton)

On Linux, Windows `.exe` games launch through **Proton via umu-launcher**. Each game gets its own isolated Wine prefix (under `~/.local/share/Spool/prefixes/`), mirroring Steam's per-game compatdata model. Spool auto-detects installed Proton builds (stock Steam Proton, GE-Proton, UMU-Proton) and uses the newest by default; you can pin a specific version per game in the game's editor or set a global default in **Settings → Compatibility**. The editor also has a **winetricks** helper for installing Windows runtime dependencies (e.g. `vcrun2022`, `dotnet48`) into a game's prefix.

### Steam Deck & SteamOS Game Mode

Spool is built to live in SteamOS **Game Mode**:

* **Add Spool to Steam** — Settings → **Steam** creates a non-Steam shortcut so you can launch Spool from the Steam library (restart Steam to see it).
* **Big-target touch UI** — Spool auto-switches to a [touch layout](#touch--desktop-layouts) on handhelds.
* **Game-Mode splash** — launching a game shows a clean full-screen splash with restore/backup progress, and Spool exits when the game closes so Steam correctly registers the game as stopped.
* **Decky backup plugin** — Game Mode force-kills external apps when you exit a game via Quick Access, which can cut off the post-session backup *and* leave the multi-device play lock dangling. Settings → **Steam Deck Backup Plugin** installs a small companion [Decky Loader](https://github.com/SteamDeckHomebrew/decky-loader) plugin (one click, from Desktop Mode) that runs `spool --release-lock` + `spool --backup` as a safety net so your saves always get backed up and the lock is released for your other devices.

### Generating shortcuts

From a game's detail panel or editor:

* **Add to Steam** — writes a non-Steam shortcut directly to Steam's `shortcuts.vdf` and downloads all artwork types (grid, portrait, hero, logo) from SteamGridDB, so the tile looks right in both desktop and Big Picture / Game Mode.
* **Armoury Crate** *(Windows only)* — generates a launcher `.exe` in `%LOCALAPPDATA%\Spool\launchers\`. In Armoury Crate: Library → Manage Library → Add, then browse to that file.

---

## Features

### Game Library

The library is the heart of Spool. Each game tracks cover art, an accent color extracted from the cover, last-played time, total playtime, install size, and save-sync status. The library lives in `%LOCALAPPDATA%\Spool\library.json` (`~/.local/share/Spool/` on Linux) with atomic, crash-safe writes.

The detail view shows large cover art, a **Play** button, an at-a-glance stats strip (last played, total playtime, install size, sync status), and action buttons (edit, refetch artwork, Add to Steam, Armoury Crate, remove). A header strip shows aggregate totals across the whole library.

The **save-sync status badge** gives a quick health check per game without opening ludusavi — showing whether your local saves are in sync with the cloud, or which side is newer.

### Touch & desktop layouts

Spool renders one of two layouts depending on the device:

* **Desktop** — a sidebar list with search and filter tabs (All / Recent / Played), right-click context menus, and separate child windows for Add / Edit / Settings.
* **Touch** — a shelf-based layout with large tappable tiles, long-press context menus, a Continue/All/LAN tab bar, and full-screen overlays instead of child windows — sized for a Deck or ROG Ally.

The mode is **auto-detected** from a coarse-pointer (touchscreen) device, and you can force Desktop or Touch in **Settings → Display & touch** for, say, a handheld docked to a monitor.

### Playtime tracking

Spool times each play session and accumulates total playtime per game, shown on the detail panel and in the library header. With a server configured, playtime and last-played are synced across devices — merged using the higher value — so totals stay accurate whether you played on your desktop or your handheld.

### Cloud save sync

Spool syncs your **save files** to the cloud through ludusavi's rclone backend (rclone is bundled). Configure it in **Settings → Cloud saves**:

* **Providers** — Google Drive, OneDrive, Dropbox, Box, FTP, SMB, WebDAV, or any **custom rclone remote** you've already configured. You can also point it at your own [Spool server's built-in WebDAV storage](#self-hosted-server) in one click.
* **How it flows** — saves are pulled from the cloud before a game launches and pushed back after it closes, so the freshest save follows you between machines.
* **Conflict resolution** — if both your local and cloud saves changed since the last sync, Spool pops a **conflict picker** before launch: it shows each side's modified time and size, you choose which to keep, and Spool applies it (with an "Open ludusavi" escape hatch if anything looks off). A cloud copy that's simply newer is fast-forwarded automatically without bothering you.

Cloud sync is entirely optional — without it, Spool still backs up saves locally on every session.

### Cross-device play-state lock

If you play the same game on multiple PCs (e.g. a desktop and a handheld), a [self-hosted server](#self-hosted-server) prevents save conflicts. Before launching, Spool acquires a per-game lock; if another device is already playing it, the launch is blocked with a warning — no more "last device to close wins" save clobbering. The lock is released when the game closes, and stale locks (from a crash or lost connection) are detected and can be overridden. Configure the server URL and API key in **Settings → Sync server**.

### LAN game sharing

Share and receive game installs across your local network — no internet required, similar to Steam's local network transfers.

* When **LAN sharing** is enabled, Spool broadcasts its presence over UDP and runs a lightweight HTTP file server (default port **47632**). Peers on the same network are discovered automatically; a WiFi indicator shows when peers are present.
* Peers' games appear right in your library alongside your own. Pick one and choose a destination to start a transfer, with live progress (files done, bytes, speed) in the download bar.
* Transfers pull up to **4 files in parallel**, each **content-verified with a blake3 hash** from the sender's manifest. Interrupted transfers **resume mid-file** via HTTP range requests, so a retry is always safe. You can cap throughput with a download speed limit.
* After a successful download the game is **automatically added to your library** with its metadata (name, run-as-admin flag, exe path) and cover art carried over from the sender. Re-downloading a game you already have is blocked until you remove it first.

Set the port, default install directory, speed limit, and your device name (as shown to peers) in **Settings → LAN sharing**.

### Launch splash & desktop notifications

Game sessions never show a blocking progress window:

* **Desktop** — Spool hides its window and reports restore status, backup status, and any errors through your OS's **native notifications** (Windows toasts / the Linux notification centre). Notifications only fire while the window is hidden, so you never get a redundant toast on top of the in-app one.
* **SteamOS Game Mode** — a full-screen **launch splash** shows the game's cover and per-phase progress, then hands off to the game and exits cleanly.

### Auto-update

When a new version is available, Spool shows a simple yes/no prompt. Accepting downloads and silently installs the update in the background — no UAC prompts or wizard windows. Both the Windows installer and the Linux AppImage update in place.

---

## Settings

Open Settings (gear icon). Options are grouped into three sections:

### Display

| Setting | Description |
|---|---|
| Display & touch | UI density: **Auto** (detects a touchscreen), **Desktop**, or **Touch** (big targets for handhelds). |

### Library

| Setting | Description |
|---|---|
| Ludusavi | Path to the ludusavi binary (bundled by default; override + autodetect available). |
| Compatibility *(Linux)* | Proton runner: a **dependency doctor** for `umu-run` / `ludusavi` / `rclone` with install hints, the `umu-run` path, and a default Proton version. |
| Steam *(Linux)* | **Add Spool to Steam** as a non-Steam shortcut. |
| Steam Deck Backup Plugin *(Linux)* | Install / update the companion **Decky** backup plugin. |
| Cloud saves | rclone-based save sync: provider, remote/path, WebDAV credentials, rclone binary + args. |
| Cover artwork | Enable **SteamGridDB** and enter your API key. |

### Sharing & Sync

| Setting | Description |
|---|---|
| LAN sharing | Enable sharing, set the port (default 47632), install directory, download speed limit, and view discovered peers. |
| Sync server | Enable a self-hosted server for play-state locks: URL, API key, and one-time account registration. |
| This device | The device name shown to LAN peers and on the server. |

All settings save live on commit — there's no Save button. The Settings window is resizable for small screens and handhelds.

---

## Self-hosted server

Cloud-coordinated features — cross-device play-state locks, playtime/last-played sync, and optional server-hosted save storage — are powered by a small self-hostable server in [`server/`](server/). It's a [Hono](https://hono.dev/) (TypeScript/Node) app backed by SQLite, with an optional bundled WebDAV save-storage service. Run it on a Raspberry Pi, home server, or any Linux box with Docker.

See [`server/README.md`](server/README.md) for the full quick-start (it boils down to downloading the compose file, setting an admin secret, and `docker compose up -d`). You register **one account** and reuse its API key on every device.

---

## Building from Source

Spool builds on **Windows and Linux** from the same source tree. It's a Tauri 2 app: a Rust backend (`tauri/src-tauri/`) and a SvelteKit 5 frontend (`tauri/src/`). The bundled `ludusavi` and `rclone` binaries ship as Tauri sidecars. The repo also keeps a tiny C# launcher stub (`launcher_stub.cs`) and its prebuilt `launcher_stub.exe`, embedded into the Rust binary at compile time for Windows-only Armoury Crate launcher generation — a normal build never recompiles it.

### Prerequisites

* [Rust](https://rustup.rs/) (stable toolchain)
* [Bun](https://bun.sh/) for the SvelteKit frontend
* The [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS:
  * **Windows** — WebView2 (preinstalled on Windows 11).
  * **Linux** — the GTK/WebKit dev packages, e.g. on Debian/Ubuntu: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.
* *(Optional, Windows only)* the framework `csc.exe` shipped with .NET Framework 4.x — only needed to recompile `launcher_stub.exe` after editing `launcher_stub.cs`.

### Build steps

1. Clone the repository:
   ```bash
   git clone https://github.com/aidankinzett/Spool
   cd Spool
   ```

2. Install frontend dependencies:
   ```bash
   cd tauri
   bun install
   ```

3. Run in development mode (hot-reload frontend + auto-rebuild backend):
   ```bash
   bun run tauri dev
   ```

4. Build a release binary + installer:
   ```bash
   bun run tauri build
   ```
   Output on **Windows**:
   * `tauri/src-tauri/target/release/spool.exe` — standalone exe
   * `tauri/src-tauri/target/release/bundle/nsis/Spool_<version>_x64-setup.exe` — NSIS installer

   Output on **Linux** (build just the AppImage with `bun run tauri build --bundles appimage`):
   * `tauri/src-tauri/target/release/bundle/appimage/Spool_<version>_amd64.AppImage`

For a deeper tour of the architecture, see [`CLAUDE.md`](CLAUDE.md).

---

## Brand assets

The Spool mark and prebuilt icon files live in [`brand/`](brand/):

| File | Use |
|---|---|
| `brand/Spool.ico` | Windows multi-resolution icon (16 – 256 px). Used by the Tauri bundle (see `tauri/src-tauri/icons/`). |
| `brand/Spool.svg` | Single-colour vector mark (`currentColor`). Drop anywhere that needs the mark inline. |
| `brand/Spool-tile.svg` | Tiled mark (dark background, white mark) — the version that appears in title bars and launcher tiles. |
| `brand/Spool-{16,32,64,128,256,512}.png` | Rasterised tiles for non-Windows contexts (web, store listings, social cards). |
