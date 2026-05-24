# ludusavi-wrap

A Windows GUI application that wraps game executables with [ludusavi](https://github.com/mtkennerly/ludusavi) save management. Maintains a **game library** with cover art and lets you launch games directly, with automatic save restore before launch and backup on exit. Also generates standalone launcher shortcuts for [Armoury Crate](https://rog.asus.com/armoury-crate/) and Steam.

Written in C# using WPF for instant startup, high-DPI/handheld touch support, and system accent color integration.

---

## Download

Grab the latest installer `ludusavi-wrap-setup.exe` or the standalone executable from the [Releases](../../releases) page. No runtimes or external installations required.

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

Click the **Play** button on any game card. The app will:
1. Restore your saves via Ludusavi (with cloud sync if configured)
2. Launch the game and wait for it to close
3. Back up your saves automatically on exit

### Generating shortcuts

Right-click any game card for shortcut options:

* **Generate for Armoury Crate** — creates a `launcher.exe` in `%LOCALAPPDATA%\ludusavi-wrap\launchers\`. In Armoury Crate: Library → Manage Library → Add, then browse to that file.
* **Add to Steam** — writes the shortcut directly to Steam's `shortcuts.vdf` and downloads all artwork types (grid, portrait, hero, logo) from SteamGridDB.

---

## Building from Source

Requires the [.NET 9.0 SDK](https://dotnet.microsoft.com/download/dotnet/9.0) (Windows Desktop payload).

1. Clone the repository:
   ```cmd
   git clone https://github.com/aidankinzett/ludusavi-wrap
   cd ludusavi-wrap
   ```
2. Run the application from source:
   ```cmd
   dotnet run
   ```
3. Publish a standalone, self-contained single-file executable:
   ```cmd
   dotnet publish -c Release -r win-x64 --self-contained true /p:PublishSingleFile=true
   ```
   Output: `bin\Release\net9.0-windows\win-x64\publish\ludusavi-wrap.exe`

4. (Optional) Generate the Inno Setup installer:
   Ensure you have [Inno Setup 6](https://jrsoftware.org/isdl.php) installed, then run:
   ```cmd
   iscc installer.iss
   ```
   Output: `dist\ludusavi-wrap-setup.exe`
