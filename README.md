# ludusavi-wrap

A Windows GUI application that wraps game executables with [ludusavi](https://github.com/mtkennerly/ludusavi) save management and generates a standalone launcher shortcut `.exe` ready to add to [Armoury Crate](https://rog.asus.com/armoury-crate/).

The generated launcher automatically **restores your saves before the game starts** and **backs them up when you close it**, with cloud sync support. If a cloud conflict is detected at launch, a dialog lets you open Ludusavi to resolve it before the game runs.

Written in C# using WPF for instant startup, high-DPI/handheld touch support, and system accent color integration.

---

## Download

Grab the latest installer `ludusavi-wrap-setup.exe` or the standalone executable from the [Releases](../../releases) page. No runtimes or external installations required.

## Requirements

* [ludusavi](https://github.com/mtkennerly/ludusavi/releases) — the save backup tool that does the actual work.
* (Optional) A [SteamGridDB API key](https://www.steamgriddb.com/profile/preferences/api) for automatic artwork download.

## Usage

1. **First launch** — point the app at your `ludusavi.exe` (or let it autodetect).
2. **Browse** to the game executable.
3. **Search** for the game name as Ludusavi knows it, or type it manually.
4. **Choose** an output folder for the generated files (defaults to `%LOCALAPPDATA%\ludusavi-wrap\launchers\`).
5. Click **Generate Wrapper** — this creates a launcher shortcut `.exe` in your `%LOCALAPPDATA%\ludusavi-wrap\launchers\` directory (and optionally downloads a Steam horizontal grid image from SteamGridDB).
5. **Add to Armoury Crate:**
   * Open Armoury Crate → Library → Manage Library → click Add.
   * Browse and select the generated launcher `.exe` file.
   * (Optional) Assign the downloaded SteamGridDB image as the game artwork.

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
   The compiled single-file binary will be located under:
   `bin\Release\net9.0-windows\win-x64\publish\ludusavi-wrap.exe`

4. (Optional) Generate the Inno Setup installer:
   Ensure you have [Inno Setup 6](https://jrsoftware.org/isdl.php) installed, then run:
   ```cmd
   iscc installer.iss
   ```
   The installer will be generated at `dist\ludusavi-wrap-setup.exe`.
