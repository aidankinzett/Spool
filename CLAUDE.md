# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ludusavi-wrap** is a Windows WPF GUI application (.NET 9.0) that wraps game executables with [ludusavi](https://github.com/mtkennerly/ludusavi) save management for ASUS Armoury Crate integration. It generates a launcher `.exe` shortcut that automatically restores saves before a game launches and backs them up on exit, checking for cloud sync conflicts.

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

* **`App.xaml` / `App.xaml.cs`**: Main entry point. Parses command line arguments. Routes `--run` to `RunWindow` and standard launch to `MainWindow`. Contains `ThemeManager` which swaps the active colour palette (`Theme.Light.xaml` / `Theme.Dark.xaml`) at runtime based on the user's saved preference or the Windows system theme. Also queries Windows DWM registry to override the accent colour brush.
* **`Theme.xaml`**: Structural ResourceDictionary — control styles, padding, fonts, and custom templates (e.g. the accent-coloured `ProgressBar`). Does not contain colour values.
* **`Theme.Light.xaml` / `Theme.Dark.xaml`**: Colour palette ResourceDictionaries merged at runtime by `ThemeManager`. Defines all brush keys (`WindowBgBrush`, `CardBgBrush`, `TextBrush`, etc.).
* **`Config.cs`**: Handles loading/saving config parameters to `%APPDATA%\ludusavi-wrap\config.json`. Implements autodetect of `ludusavi.exe` in common paths or System PATH. Uses AOT-safe Source-Generated JSON.
* **`SteamGridDbClient.cs`**: Queries the SteamGridDB API to search for game artwork and download horizontal grids to `%APPDATA%\ludusavi-wrap\covers`. Uses source-generated JSON.
* **`MainWindow.xaml` / `MainWindow.xaml.cs`**: UI for configuring and building wrappers. Automatically extracts the embedded resource `launcher_stub.exe` and appends configuration bytes to make launcher shortcuts. Launches touch keyboard if textbox focuses on handheld screens.
* **`SetupWindow.xaml` / `SetupWindow.xaml.cs`**: Dialog for managing Settings (Ludusavi path, SteamGridDB API key, and theme preference). The theme ComboBox applies a live preview immediately on change; Cancel reverts to the last saved preference.
* **`SuccessWindow.xaml` / `SuccessWindow.xaml.cs`**: Dialog shown after successful generation, displaying path details and copy actions.
* **`RunWindow.xaml` / `RunWindow.xaml.cs`**: Background-friendly sync overlay which displays progress while restoring saves, waits for the game process to exit, and runs backup.

### Key Patterns

* **Async operations**: File IO, API requests, and subprocess execution run asynchronously via Tasks (`async`/`await`) to keep the WPF UI thread responsive.
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
git tag v2.0.5 -m "v2.0.5"
git push origin master v2.0.5
```

**What the workflow does automatically (no manual steps needed):**

1. Stamps the assembly with the version from the tag (`/p:Version=2.0.5`)
2. Updates `update.xml` with the new version number and download URL
3. Compiles the launcher stub (`launcher_stub.cs` → `launcher_stub.exe`)
4. Publishes a self-contained single-file `ludusavi-wrap.exe`
5. Builds the Inno Setup installer (`ludusavi-wrap-setup.exe`)
6. Creates a GitHub Release with auto-generated release notes and attaches both artifacts
7. Commits the updated `update.xml` back to `master`

**Version number conventions:**
- The app version is derived entirely from the git tag — there is no hardcoded version string in source code
- Use `vMAJOR.MINOR.PATCH` format (e.g. `v2.0.5`)
- Skip patch numbers if needed (e.g. go from `v2.0.3` to `v2.0.5`) — there is no strict requirement to be sequential
