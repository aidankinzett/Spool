# ludusavi-wrap

A small Windows GUI that wraps any game executable with [ludusavi](https://github.com/mtkennerly/ludusavi) save management and generates a `.bat` launcher ready to add to [Armoury Crate](https://rog.asus.com/armoury-crate/).

The generated launcher automatically **restores your saves before the game starts** and **backs them up when you close it** — using ludusavi's built-in `wrap` command.

## Download

Grab the latest `ludusavi-wrap.exe` from the [Releases](../../releases) page. No Python required.

## Requirements

- [ludusavi](https://github.com/mtkennerly/ludusavi/releases) — the save backup tool that does the actual work
- (Optional) A [SteamGridDB API key](https://www.steamgriddb.com/profile/preferences/api) for automatic artwork download

## Usage

1. **First launch** — point the app at your `ludusavi.exe`
2. **Browse** to the game executable
3. **Search** for the game name as ludusavi knows it, or type it manually
4. **Choose** an output folder for the generated files
5. Click **Generate Wrapper** — this creates a `.bat` file (and optionally downloads a Steam horizontal grid image from SteamGridDB)
6. **Add to Armoury Crate:**
   - Library → Manage Library → find your game
   - Press X → Game Options → Game Info → Edit
   - Paste the `.bat` path into **Launch CMD**
   - Set the title and use the downloaded image for artwork

## SteamGridDB

Enable it in **Settings** and paste your API key. When enabled, generating a wrapper will automatically download a Steam horizontal grid image (460×215 / 920×430) and save it alongside the `.bat` with the same filename.

## Building from source

Requires [uv](https://docs.astral.sh/uv/).

```
git clone https://github.com/akinzett/ludusavi-wrap
cd ludusavi-wrap
uv run main.py
```

To build a standalone exe locally:

```
uv sync --dev
uv run pyinstaller --onefile --windowed --collect-all customtkinter --name ludusavi-wrap main.py
# output: dist/ludusavi-wrap.exe
```
