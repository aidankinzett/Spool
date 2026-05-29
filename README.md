<p align="center">
  <img src="brand/Spool-256.png" alt="Spool" width="128" height="128">
</p>

<h1 align="center">Spool</h1>

<p align="center">
  A game library for handheld PCs and desktops — Windows and Linux.<br/>
  Cover-art shelf · cross-device save sync · LAN sharing · launcher generation.<br/>
  <em>Powered by <a href="https://github.com/mtkennerly/ludusavi">ludusavi</a> for the saves themselves.</em>
</p>

---

Spool started life as a thin wrapper around [ludusavi](https://github.com/mtkennerly/ludusavi) — restore saves before a game launches, back them up on exit. It's since grown into a full personal game shelf: cover art from SteamGridDB, LAN game-sharing between devices, a sync server that locks saves across machines, TorBox downloads, and one-click launcher shortcuts for Armoury Crate and Steam.

Built with [Tauri 2](https://v2.tauri.app/) (Rust backend) and [SvelteKit 5](https://kit.svelte.dev/) — small native binary, instant startup, and a webview-rendered UI with system accent color integration.

Runs on **Windows** and **Linux** — including the gaming-handheld distros like **Bazzite**, **CachyOS**, and **SteamOS** (Steam Deck). Windows is the primary target; a couple of OS-integration extras (Armoury Crate launcher generation, run-as-administrator) are Windows-only and simply don't appear on Linux, while everything else — the library, save restore/backup, LAN sharing, cloud sync, downloads, and notifications — works the same on both.

---

## Table of Contents

- [Download](#download)
- [Requirements](#requirements)
- [Usage](#usage)
  - [Adding games to your library](#adding-games-to-your-library)
  - [Playing a game](#playing-a-game)
  - [Generating shortcuts](#generating-shortcuts)
- [Features](#features)
  - [Game Library](#game-library)
  - [Playtime Tracking](#playtime-tracking)
  - [LAN Game Sharing](#lan-game-sharing)
  - [Cloud Sync & Cross-Device Lock](#cloud-sync--cross-device-lock)
  - [TorBox Downloader Integration](#torbox-downloader-integration)
  - [Browse Games Window](#browse-games-window)
  - [Desktop Notifications](#desktop-notifications)
  - [Theming](#theming)
  - [Auto-Update](#auto-update)
- [Settings](#settings)
- [Building from Source](#building-from-source)

---

## Download

Grab the latest build from the [Releases](../../releases) page:

* **Windows** — the `spool-setup.exe` installer (NSIS) or the standalone executable.
* **Linux** — the `Spool_<version>_amd64.AppImage`. Mark it executable (`chmod +x`) and run it; it works on Bazzite, CachyOS, SteamOS, and other modern distros.

No runtimes or external installations required, and both platforms auto-update in place.

## Requirements

* [ludusavi](https://github.com/mtkennerly/ludusavi/releases) — the save backup tool that does the actual work.
* (Optional) A [SteamGridDB API key](https://www.steamgriddb.com/profile/preferences/api) for automatic cover art download.

## Usage

### Adding games to your library

1. **First launch** — point the app at your `ludusavi.exe` in Settings (or let it autodetect).
2. Click **Add Game**.
3. **Drop the game's `.exe`** onto the dialog, or click "browse for one".
4. Spool queries Ludusavi to auto-identify the game — a ranked list of candidates appears with confidence scores. If only one strong match is found it is pre-selected; if there are several, pick the right one.
5. Use **Search Manually** if auto-detection finds nothing.
6. Click **Add to Library**, **Armoury Crate**, or **Add to Steam** — cover art is fetched automatically from SteamGridDB (if configured).

### Playing a game

Select a game in the sidebar and click the **Play** button. Spool will:
1. Restore your saves via Ludusavi (with cloud sync if configured)
2. Check that no other device is already playing the game (if a sync server is configured)
3. Launch the game and wait for it to close
4. Back up your saves automatically on exit
5. Update the save sync status badge on the card

The app hides itself during gameplay and communicates progress through **native desktop notifications** — no modal window blocking your screen.

### Generating shortcuts

Select a game and use the action buttons in the detail panel:

* **Armoury Crate** *(Windows only)* — creates a `launcher.exe` in `%LOCALAPPDATA%\Spool\launchers\`. In Armoury Crate: Library → Manage Library → Add, then browse to that file.
* **Add to Steam** — writes the shortcut directly to Steam's `shortcuts.vdf` and downloads all artwork types (grid, portrait, hero, logo) from SteamGridDB.
* **Game settings → Install folder** — manually set the installation folder for a game (used by LAN sharing).

---

## Features

### Game Library

The main window uses a sidebar + detail layout:

**Sidebar (left)**
- Searchable game list with cover thumbnails, last-played time, and save-sync status badge
- Filter tabs: **All**, **Recent**, **On LAN**, **Unplayed**

**Detail panel (right)**
- Large cover art, game title, and a **Play** button
- At-a-glance stats: last played, total playtime, install size, sync status
- Action buttons: Open in Editor, Generate for Armoury Crate, Add to Steam, Refetch artwork
- Game settings: run as administrator, install folder (used for LAN sharing)
- **Remove** button

**Library stats header** shows aggregate totals: number of games, total playtime, save backup count, and total disk usage.

The **save-sync status badge** updates after each session:
- Green — saves are synced across devices
- Orange — local save is newer than cloud
- Blue — cloud save is newer than local

Games are stored in `%LOCALAPPDATA%\Spool\library.json` with atomic writes to prevent corruption.

### Playtime Tracking

Spool measures how long each play session lasts and accumulates total playtime per game.

- Playtime is displayed on each game's detail panel and in the library stats header.
- If a sync server is configured, playtime is pushed after every session and pulled on startup, merging using the higher of the local or server value — so totals stay accurate across multiple devices.

### LAN Game Sharing

Share and receive games across your local network without any internet connection — similar to Steam's local network transfers.

**How it works:**

- When LAN Share is enabled in Settings, Spool starts a lightweight HTTP file server on your local network (default port 47632) and broadcasts its presence via UDP.
- Peers on the same network are discovered automatically in the background. The WiFi icon in the main toolbar shows when peers are present.
- Peers broadcast their game list, so you can see what others have available directly in your library grid — LAN-available games appear alongside your own.
- Double-clicking a LAN game (or selecting **Download from LAN**) opens a destination picker and begins the transfer, showing live progress (files completed, bytes transferred, speed) in the main window's download bar.
- After a successful download, the game is **automatically added to your library** with its metadata (game name, run-as-admin flag, EXE path) synced from the sender.
- Cover art is shared over HTTP alongside the game files, so new games arrive with artwork already in place.
- Spool guards against duplicate downloads: if the game is already in your library, re-downloading is blocked until you remove it first.
- **Administrator elevation** *(Windows)* — if the sender marked a game as requiring admin, the receiver respects that flag too.
- Transfers download up to 4 files in parallel for near-line-rate throughput. Each file is content-verified with a blake3 hash from the sender's manifest, and interrupted transfers resume mid-file via HTTP range requests — already-complete files are skipped, so a retry is always safe.
- Your device name (shown to peers) is configurable in Settings.

**Configuring LAN Share:**

Open Settings and expand the **LAN Share** card:
- Enable the toggle
- Set the port (default: 47632)
- Set the default install directory for received games

### Cloud Sync & Cross-Device Lock

If you play the same game on multiple PCs (e.g. a desktop and a handheld), the sync server prevents save conflicts.

**Save sync server:**

Spool can connect to a sync server to coordinate saves across devices. Configure a server URL and API key in Settings under the **Sync Server** card.

**Cross-device play state lock:**

Before launching a game, Spool acquires a lock on the sync server. If another device is already playing that game, you'll be warned and the launch is blocked — preventing the classic "last device to close wins" save conflict. The lock is released automatically when the game closes. Stale locks (from a crash or lost connection) are detected and can be overridden.

**Save sync status badges:**

After each play session, the card's badge updates to reflect whether the local and cloud saves are in sync. This gives you a quick visual health check across your library without opening Ludusavi.

### TorBox Downloader Integration

[TorBox](https://torbox.app) is a torrent debrid service — it downloads torrents server-side and lets you pull the result at full speed. Spool integrates directly:

- Configure your TorBox API key and a local download directory in Settings.
- When downloading a game via the **Browse Games** window (see below), Spool can send the torrent/magnet to TorBox and then download the cached result to your machine.
- Download progress is tracked inline.

### Browse Games Window

The **Browse Games** window lets you search a catalogue of downloadable games from configured Hydra-format download sources.

- Click **Browse Games** in the toolbar (if download sources are configured in Settings).
- The window fetches all sources and presents a searchable, filterable list showing title, file size, and upload date.
- Select a game and click **Download** (or double-click) to initiate a download via TorBox or a direct link.
- After the download completes, you're prompted to add the game to your library.

**Configuring download sources:**

Open Settings → **Download Sources** — add one or more URLs pointing to Hydra-format JSON catalogues. Spool fetches and merges them on each open.

### Desktop Notifications

Game sessions no longer show a blocking progress window. Instead, Spool hides itself and uses your operating system's **native notifications** (Windows toasts / the Linux notification centre) to communicate:

- Save restore status before the game launches
- Save backup status after the game closes
- Any errors that occurred

Notifications are only shown while the main window is hidden, so you won't get a redundant toast on top of the in-app one. This keeps your desktop clear while gaming and works naturally with handheld devices where screen real estate is limited.

### Theming

Three theme options are available in Settings:

- **System** — follows your operating system's light/dark preference automatically
- **Light**
- **Dark**

The theme is applied live across all open windows with no restart required. The Settings window shows a live preview as you change the selection; cancelling reverts to the last saved theme.

### Auto-Update

When a new version is available, Spool shows a simple yes/no prompt. Accepting downloads and silently installs the update in the background — no UAC prompts or wizard windows.

---

## Settings

Open Settings (gear icon) to configure. Settings are organised into tabs:

| Tab | Setting | Description |
|---|---|---|
| General | Ludusavi executable | Path to `ludusavi.exe`. Shows "Detected" if found automatically. |
| General | Theme | System / Light / Dark — applied live across all open windows. |
| Artwork | SteamGridDB | API key for automatic cover art download. |
| Sources | Download Sources | Hydra-format catalogue URLs for the Browse Games window. |
| LAN sharing | LAN Share | Enable/disable, port number, and default install directory for received games. |
| Cloud sync | Sync Server | URL and API key for the cloud sync / play-state-lock server. |
| Cloud sync | Device Name | How this machine appears to peers and on the sync server. |
| Downloads | TorBox | API key and local download directory for the debrid downloader. |

The Settings window is resizable, making it usable on small screens and handhelds.

---

## Building from Source

Spool builds on **Windows and Linux** from the same source tree. It's a Tauri 2 app: a Rust backend (`tauri/src-tauri/`) and a SvelteKit 5 frontend (`tauri/src/`). The repo also keeps a tiny C# launcher stub (`launcher_stub.cs`) — and its prebuilt `launcher_stub.exe` — that's embedded into the Rust binary at compile time; when generating per-game Armoury Crate shortcuts (a Windows-only feature) the Rust app writes a copy of this stub with a config payload appended. Because the compiled `launcher_stub.exe` is checked into the repo, a normal build never needs to recompile it — including on Linux.

### Prerequisites

* [Rust](https://rustup.rs/) (stable toolchain — the Tauri build pulls in everything else it needs)
* [Bun](https://bun.sh/) for the SvelteKit frontend
* The [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS:
  * **Windows** — WebView2 (preinstalled on Windows 11).
  * **Linux** — the GTK/WebKit dev packages, e.g. on Debian/Ubuntu: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.
* *(Optional, Windows only)* the framework `csc.exe` that ships with .NET Framework 4.x (already present on every Windows machine) — only needed to recompile `launcher_stub.exe` after editing `launcher_stub.cs`.

### Build steps

1. Clone the repository:
   ```bash
   git clone https://github.com/aidankinzett/Spool
   cd Spool
   ```

2. *(Windows only, optional)* recompile the embedded launcher stub — **skip this unless you've edited `launcher_stub.cs`**, since the prebuilt `launcher_stub.exe` is committed to the repo:
   ```powershell
   & "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe" `
       /target:winexe /win32icon:launcher_stub.ico `
       /out:launcher_stub.exe launcher_stub.cs
   ```

3. Install frontend dependencies:
   ```bash
   cd tauri
   bun install
   ```

4. Run in development mode (hot-reload frontend + auto-rebuild backend):
   ```bash
   bun run tauri dev
   ```

5. Build a release binary + installer:
   ```bash
   bun run tauri build
   ```
   Output on **Windows**:
   * `tauri/src-tauri/target/release/spool.exe` — standalone exe
   * `tauri/src-tauri/target/release/bundle/nsis/Spool_<version>_x64-setup.exe` — NSIS installer

   Output on **Linux** (build just the AppImage with `bun run tauri build --bundles appimage`):
   * `tauri/src-tauri/target/release/bundle/appimage/Spool_<version>_amd64.AppImage`

---

## Brand assets

The Spool mark and prebuilt icon files live in [`brand/`](brand/):

| File | Use |
|---|---|
| `brand/Spool.ico` | Windows multi-resolution icon (16 – 256 px). Used by the Tauri bundle (see `tauri/src-tauri/icons/`). |
| `brand/Spool.svg` | Single-colour vector mark (`currentColor`). Drop anywhere that needs the mark inline. |
| `brand/Spool-tile.svg` | Tiled mark (dark background, white mark) — the version that appears in title bars and launcher tiles. |
| `brand/Spool-{16,32,64,128,256,512}.png` | Rasterised tiles for non-Windows contexts (web, store listings, social cards). |
