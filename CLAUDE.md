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

* **`App.xaml` / `App.xaml.cs`**: Main entry point. Parses command line arguments. Routes `--run` to `RunWindow` and standard launch to `MainWindow`. Dynamically queries Windows DWM registry on start to load and override UI colors with the user's active system accent color.
* **`Theme.xaml`**: ResourceDictionary defining modern Windows 11 Fluent styling (dark mode, rounded corners, custom text box placeholders, custom toggle switch).
* **`Config.cs`**: Handles loading/saving config parameters to `%APPDATA%\ludusavi-wrap\config.json`. Implements autodetect of `ludusavi.exe` in common paths or System PATH. Uses AOT-safe Source-Generated JSON.
* **`SteamGridDbClient.cs`**: Queries the SteamGridDB API to search for game artwork and download horizontal grids to `%APPDATA%\ludusavi-wrap\covers`. Uses source-generated JSON.
* **`Switch.cs`**: Custom toggle switch control inheriting from `ToggleButton`.
* **`MainWindow.xaml` / `MainWindow.xaml.cs`**: UI for configuring and building wrappers. Automatically extracts the embedded resource `launcher_stub.exe` and appends configuration bytes to make launcher shortcuts. Launches touch keyboard if textbox focuses on handheld screens.
* **`SetupWindow.xaml` / `SetupWindow.xaml.cs`**: Dialog for managing Settings (Ludusavi path and SteamGridDB API key).
* **`SuccessWindow.xaml` / `SuccessWindow.xaml.cs`**: Dialog shown after successful generation, displaying path details and copy actions.
* **`RunWindow.xaml` / `RunWindow.xaml.cs`**: Background-friendly sync overlay which displays progress while restoring saves, waits for the game process to exit, and runs backup.

### Key Patterns

* **Async operations**: File IO, API requests, and subprocess execution run asynchronously via Tasks (`async`/`await`) to keep the WPF UI thread responsive.
* **WPF Single-File self-contained configuration**: Output is packaged as a single large binary with native libraries bundled, ensuring instant start and zero user-facing installation friction.
