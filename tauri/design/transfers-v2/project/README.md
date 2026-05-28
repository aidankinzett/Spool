<p align="center">
  <img src="brand/Spool-256.png" alt="Spool" width="128" height="128">
</p>

<h1 align="center">Spool</h1>

<p align="center">
  A Windows-native game library for handheld PCs and desktops.<br/>
  Cover-art shelf · cross-device save sync · LAN sharing · launcher generation.<br/>
  <em>Powered by <a href="https://github.com/mtkennerly/ludusavi">ludusavi</a> for the saves themselves.</em>
</p>

---

Spool started life as a thin wrapper around [ludusavi](https://github.com/mtkennerly/ludusavi) — restore saves before a game launches, back them up on exit. It's since grown into a full personal game shelf: cover art from SteamGridDB, LAN game-sharing between devices, a sync server that locks saves across machines, TorBox downloads, and one-click launcher shortcuts for Armoury Crate and Steam.

Written in C# / WPF for instant startup, high-DPI / handheld touch support, and system accent color integration.

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
  - [LAN Game Sharing](#lan-game-sharing)
  - [Cloud Sync & Cross-Device Lock](#cloud-sync--cross-device-lock)
  - [TorBox Downloader Integration](#torbox-downloader-integration)
  - [Browse Games Window](#browse-games-window)
  - [Windows Toast Notifications](#windows-toast-notifications)
  - [Theming](#theming)
  - [Auto-Update](#auto-update)
- [Settings](#settings)
- [Building from Source](#building-from-source)

---

## Download

Grab the latest installer `spool-setup.exe` or the standalone executable from the [Releases](../../releases) page. No runtimes or external installations required.

## Requirements

* [ludusavi](https://github.com/mtkennerly/ludusavi/releases) — the save backup tool that does the actual work.
* (Optional) A [SteamGridDB API key](https://www.steamgriddb.com/profile/preferences/api) for automatic cover art download.

## Usage

### Adding games to your library

1. **First launch** — point the app at your `ludusavi.exe` (or let it autodetect).
2. Click **Add Game**.
3. **Browse** to the game executable.
4. **Search** for the game name as Ludusavi knows it, or type it manually.
5. Click **Add to Library** — the game is saved and cover art is fetched automatically from SteamGridDB (if configured).

### Playing a game

Click the **Play** button on any game card. Spool will:
1. Restore your saves via Ludusavi (with cloud sync if configured)
2. Check that no other device is already playing the game (if a sync server is configured)
3. Launch the game and wait for it to close
4. Back up your saves automatically on exit
5. Update the save sync status badge on the card

The app hides itself during gameplay and communicates progress through **Windows toast notifications** — no modal window blocking your screen.

### Generating shortcuts

Right-click any game card for shortcut options:

* **Generate for Armoury Crate** — creates a `launcher.exe` in `%LOCALAPPDATA%\Spool\launchers\`. In Armoury Crate: Library → Manage Library → Add, then browse to that file.
* **Add to Steam** — writes the shortcut directly to Steam's `shortcuts.vdf` and downloads all artwork types (grid, portrait, hero, logo) from SteamGridDB.
* **Set Game Folder** — manually set the installation folder for a game (used by LAN sharing).

---

## Features

### Game Library

The main window displays your games as a cover-art grid (portrait cards, 180×265 px). Each card shows:

- Full cover art fetched from SteamGridDB
- Game name with text wrapping
- A **Play** button
- A **save sync status badge** (shown after the first play session):
  - Green — saves are synced across devices
  - Orange — local save is newer than cloud
  - Blue — cloud save is newer than local

Games are stored in `%LOCALAPPDATA%\Spool\library.json` with atomic writes to prevent corruption.

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
- **Administrator elevation** is supported — if the sender marked a game as requiring admin, the launcher respects that on the receiver's machine.
- Transfers use 4 parallel streams with 512 KB buffers for near-line-rate throughput. File comparison uses size and modification time to skip already-up-to-date files.
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

### Windows Toast Notifications

Game sessions no longer show a blocking progress window. Instead, Spool hides itself and uses **Windows toast notifications** to communicate:

- Save restore status before the game launches
- Save backup status after the game closes
- Any errors that occurred

This keeps your desktop clear while gaming and works naturally with handheld devices where screen real estate is limited.

### Theming

Three theme options are available in Settings:

- **System** — follows your Windows light/dark preference automatically
- **Light**
- **Dark**

The theme is applied live across all open windows with no restart required. The Settings window shows a live preview as you change the selection; cancelling reverts to the last saved theme.

### Auto-Update

When a new version is available, Spool shows a simple yes/no prompt. Accepting downloads and silently installs the update in the background — no UAC prompts or wizard windows.

---

## Settings

Open Settings (gear icon) to configure:

| Setting | Description |
|---|---|
| Ludusavi Path | Path to `ludusavi.exe`. Click Auto-detect to find it automatically. |
| SteamGridDB | API key for automatic cover art download. |
| Theme | System / Light / Dark. |
| Sync Server | URL and API key for the cloud sync / play-state-lock server. |
| Device Name | How this machine appears to peers and on the sync server. |
| LAN Share | Enable/disable, port number, and default install directory. |
| TorBox | API key and local download directory for the debrid downloader. |
| Download Sources | List of Hydra-format catalogue URLs for the Browse Games window. |

The Settings window is scrollable and resizable, making it usable on small screens and handhelds.

---

## Building from Source

Requires the [.NET 9.0 SDK](https://dotnet.microsoft.com/download/dotnet/9.0) (Windows Desktop payload).

1. Clone the repository:
   ```cmd
   git clone https://github.com/aidankinzett/spool
   cd spool
   ```
2. Run the application from source:
   ```cmd
   dotnet run
   ```
3. Publish a standalone, self-contained single-file executable:
   ```cmd
   dotnet publish -c Release -r win-x64 --self-contained true /p:PublishSingleFile=true
   ```
   Output: `bin\Release\net9.0-windows\win-x64\publish\spool.exe`

4. (Optional) Generate the Inno Setup installer:
   Ensure you have [Inno Setup 6](https://jrsoftware.org/isdl.php) installed, then run:
   ```cmd
   iscc installer.iss
   ```
   Output: `dist\spool-setup.exe`

---

## Brand assets

The Spool mark and prebuilt icon files live in [`brand/`](brand/):

| File | Use |
|---|---|
| `brand/Spool.ico` | Windows multi-resolution icon (16 – 256 px). Drop into the project as the app's `.csproj` ApplicationIcon. |
| `brand/Spool.svg` | Single-colour vector mark (`currentColor`). Drop anywhere that needs the mark inline. |
| `brand/Spool-tile.svg` | Tiled mark (dark background, white mark) — the version that appears in title bars and launcher tiles. |
| `brand/Spool-{16,32,64,128,256,512}.png` | Rasterised tiles for non-Windows contexts (web, store listings, social cards). |

