# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ludusavi-wrap** is a Windows WPF GUI application (.NET 9.0) that wraps game executables with [ludusavi](https://github.com/mtkennerly/ludusavi) save management. It maintains a persistent **game library** with cover art (via SteamGridDB) and lets users launch games directly from the app — automatically restoring saves before launch and backing them up on exit, with cloud sync conflict detection. It can also generate standalone launcher shortcuts for ASUS Armoury Crate and Steam.

## Commands

```bash
# Run application in development mode
dotnet run

# Build project locally (Debug)
dotnet build

# Publish standalone single-file executable (includes .NET runtime and WPF framework)
dotnet publish -c Release -r win-x64 --self-contained true /p:PublishSingleFile=true
# Output: bin\Release\net9.0-windows\win-x64\publish\ludusavi-wrap.exe

# Compile the C# launcher stub (if changes are made to launcher_stub.cs)
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /target:winexe /out:launcher_stub.exe launcher_stub.cs

# Compile the Inno Setup installer (requires Inno Setup)
iscc installer.iss
# Output: dist/ludusavi-wrap-setup.exe
```

## Architecture

The application is structured into the following C# and XAML components:

* **`App.xaml` / `App.xaml.cs`**: Main entry point. Parses command line arguments. Routes `--run` to `RunWindow` and standard launch to `MainWindow`. Contains `ThemeManager` which swaps the active colour palette at runtime based on the user's saved preference or the Windows system theme.
* **`Config.cs`**: Handles loading/saving app-wide settings to `%LOCALAPPDATA%\ludusavi-wrap\config.json` (Ludusavi path, SteamGridDB key, theme, sync server, device identity). Implements autodetect of `ludusavi.exe`. Uses AOT-safe source-generated JSON.
* **`GameLibrary.cs`**: Manages the persistent game library. `GameEntry` holds per-game data (name, exe path, cover image path, last played). `GameLibrary` provides CRUD with atomic JSON saves (write-then-replace) to `%LOCALAPPDATA%\ludusavi-wrap\library.json`. `GameEntry` implements `INotifyPropertyChanged` on `CoverImagePath` and `LastPlayedAt` so WPF bindings update live.
* **`LauncherGenerator.cs`**: Static helper that extracts the embedded `launcher_stub.exe` resource and appends a config payload to produce a game-specific launcher `.exe`. Also contains `MakeSafeFilename`. Used by both `AddGameWindow` and `MainWindow` context menu actions.
* **`SteamGridDbClient.cs`**: Queries the SteamGridDB API to search for games and download artwork (horizontal grid, portrait, hero, logo) to `%LOCALAPPDATA%\ludusavi-wrap\covers`. Uses source-generated JSON.
* **`MainWindow.xaml` / `MainWindow.xaml.cs`**: The game library grid. Displays an `ObservableCollection<GameEntry>` as a `WrapPanel` of cover-art cards (180×265px). Each card has a Play button and a right-click context menu (Generate for Armoury Crate, Add to Steam, Open Game Folder, Remove). Shows an empty state when the library has no entries. Contains `StringToImageConverter` (loads `BitmapImage` with `CacheOption.OnLoad` + `Freeze` to avoid file handle leaks).
* **`AddGameWindow.xaml` / `AddGameWindow.xaml.cs`**: Modal dialog for adding a game. Has exe browse, Ludusavi game name search (fuzzy via CLI), and three actions: **Add to Library** (saves entry, fetches portrait cover art async), **Armoury Crate** (generates launcher stub, also adds to library), **Add to Steam** (generates launcher stub, writes `shortcuts.vdf`, downloads all artwork types, also adds to library).
* **`SetupWindow.xaml` / `SetupWindow.xaml.cs`**: Dialog for managing settings (Ludusavi path, SteamGridDB API key, theme, sync server). The theme ComboBox applies a live preview immediately on change; Cancel reverts to the last saved preference.
* **`SuccessWindow.xaml` / `SuccessWindow.xaml.cs`**: Dialog shown after shortcut generation, displaying path details, copy actions, and artwork status (updated asynchronously as SteamGridDB downloads complete).
* **`RunWindow.xaml` / `RunWindow.xaml.cs`**: Sync overlay shown during game launch. Restores saves, waits for the game process to exit, then backs up saves. Accepts `exitAppOnFinish` (default `true` for the `--run` stub path; `false` when launched directly from the library, in which case it closes itself and restores `MainWindow`).
* **`launcher_stub.cs`**: Standalone .NET executable embedded as a resource. Reads a config payload appended to its own binary (`LUDUSAVI_WRAP_CFG_START` / `LUDUSAVI_WRAP_CFG_END` markers) and launches `ludusavi-wrap.exe --run "{gameName}" "{gameExePath}"`.

### Data Files

| File | Location | Contents |
|------|----------|----------|
| `config.json` | `%LOCALAPPDATA%\ludusavi-wrap\` | App-wide settings (Ludusavi path, API keys, theme, sync server, device ID) |
| `library.json` | `%LOCALAPPDATA%\ludusavi-wrap\` | Game library — list of `GameEntry` objects |
| `covers/` | `%LOCALAPPDATA%\ludusavi-wrap\` | Downloaded SteamGridDB cover images |
| `launchers/` | `%LOCALAPPDATA%\ludusavi-wrap\` | Generated launcher `.exe` stubs |
| `debug.log` | `%LOCALAPPDATA%\ludusavi-wrap\` | App log (errors, startup events) |

### Key Patterns

* **Async operations**: File IO, API requests, and subprocess execution run asynchronously via Tasks (`async`/`await`) to keep the WPF UI thread responsive. Steam registry/VDF IO is wrapped in `Task.Run`.
* **Game library persistence**: `GameLibrary.Save()` uses an atomic write (temp file → `File.Replace`) to protect against corruption on crash. Errors are logged to `App.Log()`.
* **Live cover art updates**: `GameEntry.CoverImagePath` raises `PropertyChanged`, so the card's `Image` binding updates automatically when the async SteamGridDB download completes on the background thread — no manual UI refresh needed.
* **WPF Single-File self-contained configuration**: Output is packaged as a single large binary with native libraries bundled, ensuring instant start and zero user-facing installation friction.
* **Dynamic theming**: All XAML brush references use `DynamicResource` (not `StaticResource`) so that swapping the merged colour dictionary in `Application.Resources` propagates instantly to all open windows.

## Releasing

Releases are fully automated via `.github/workflows/release.yml` and triggered by pushing a version tag.

**Steps to release a new version:**

```powershell
# 1. Ensure all changes are committed and merged to master
git checkout master
git pull

# 2. Create an annotated tag (triggers the release workflow)
git tag v3.0.1 -m "v3.0.1"
git push origin master v3.0.1
```

**What the workflow does automatically (no manual steps needed):**

1. Stamps the assembly with the version from the tag (`/p:Version=3.0.1`)
2. Updates `update.xml` with the new version number and download URL
3. Compiles the launcher stub (`launcher_stub.cs` → `launcher_stub.exe`)
4. Publishes a self-contained single-file `ludusavi-wrap.exe`
5. Builds the Inno Setup installer (`ludusavi-wrap-setup.exe`)
6. Creates a GitHub Release with auto-generated release notes and attaches both artifacts
7. Commits the updated `update.xml` back to `master`

**Version number conventions:**
- The app version is derived entirely from the git tag — there is no hardcoded version string in source code
- Use `vMAJOR.MINOR.PATCH` format (e.g. `v3.0.1`)
- Skip patch numbers if needed — there is no strict requirement to be sequential
