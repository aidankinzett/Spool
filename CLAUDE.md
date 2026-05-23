# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ludusavi-wrap** is a Windows GUI application that wraps game executables with [ludusavi](https://github.com/mtkennerly/ludusavi) save management for ASUS Armoury Crate integration. It generates a `.bat` launcher that automatically restores saves before a game launches and backs them up on exit.

## Commands

```bash
# Install dependencies
uv sync

# Run from source
uv run main.py

# Build standalone executable (requires dev deps)
uv sync --dev
uv run pyinstaller --onefile --windowed --collect-all customtkinter --add-data "themes;themes" --name ludusavi-wrap main.py
# Output: dist/ludusavi-wrap.exe
```

**Release**: Tag commits as `v*` (e.g., `v1.0.4`). GitHub Actions bumps the `VERSION` constant from the tag, builds the exe, and creates a GitHub release. Do not manually edit `VERSION` for releases.

## Architecture

The entire application lives in a single `main.py` file with four classes:

- **`Config`**: Manages persistent JSON config, auto-detects `ludusavi.exe`, and handles both frozen (PyInstaller) and dev execution contexts. Config path is `%APPDATA%\ludusavi-wrap\config.json` when frozen, `./config.json` in dev.
- **`SetupDialog`**: Modal settings window for ludusavi path and SteamGridDB API key.
- **`CopyDialog`**: Post-generation dialog showing the .bat path and game name with copy-to-clipboard buttons.
- **`App`**: Main tkinter window driving three core workflows:
  1. **Executable selection** — file browse dialog filtered to `*.exe`
  2. **Game name search** — calls `ludusavi find --api --fuzzy --multiple <query>` via subprocess, parses JSON results into a dropdown
  3. **Wrapper generation** — writes a `.bat` from `BAT_TEMPLATE` with embedded paths; optionally fetches a Steam horizontal grid image (460×215 or 920×430) from SteamGridDB API and saves it alongside the .bat

### Key Patterns

- **Thread safety**: Long-running work (ludusavi subprocess, SteamGridDB API) runs on daemon threads; UI updates are marshalled back via `self.after(0, callback)`.
- **Window resizing**: The window auto-resizes to content height after state changes.
- **BAT template**: `BAT_TEMPLATE` constant contains the full batch file with error-checking; game name and exe path are substituted at generation time.
- **Windows-only**: Uses `os.startfile()`, `TabTip.exe` for touch keyboard (ROG Ally support), and `.bat` file generation.
