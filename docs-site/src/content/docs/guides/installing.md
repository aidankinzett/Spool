---
title: Install Spool
description: Download and install Spool on Windows or a Linux handheld, and add your first game.
---

Spool runs on **Windows** and **Linux**, including the gaming-handheld distros
(Bazzite, CachyOS, SteamOS / Steam Deck). The library, save sync, LAN sharing,
and cloud sync work the same on both.

## Download

Grab the latest build from the
[Releases page](https://github.com/aidankinzett/Spool/releases):

- **Windows** — the `Spool_<version>_x64-setup.exe` installer.
- **Linux** — the `Spool_<version>_amd64.AppImage`.

Both platforms update themselves in place — when a new version is released,
Spool prompts to download and apply it on the next launch.

## Install

### Windows

Run the `…-setup.exe` installer and follow the prompts. Spool installs per-user
and adds a Start Menu entry.

### Linux

Mark the AppImage executable and run it:

```bash
chmod +x Spool_*_amd64.AppImage
./Spool_*_amd64.AppImage
```

On a Steam Deck or other handheld, do this from Desktop Mode the first time. To
launch your library from Game Mode without dropping to the desktop, install the
[Decky plugin](/decky/overview/).

:::note[Running Windows games on Linux]
The Linux build launches Windows `.exe` games through **Proton** using
[umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) (`umu-run`).
It's the one dependency Spool doesn't bundle — install it from your distro's
package manager. Settings → Compatibility checks whether it's present and gives
per-distro install hints.
:::

## How Spool runs

Spool lives in your system tray. Closing the library window hides it to the tray
rather than quitting, so it stays ready to launch games with no cold-start delay.
Quit from the tray menu's **Quit Spool** item.

## Add your first game

1. Open Spool and choose **Add Game**.
2. Drop in (or browse for) the game's `.exe`.
3. Spool identifies the game, suggests cover art, and shows ranked matches so its
   saves can be tracked. Pick the right match and add it — or add it without save
   tracking if you'd rather not.

Once a game is in your library, launching it from Spool restores the latest save
before play and backs it up when you quit.

## Next steps

- [Set up cloud save sync](/guides/cloud-saves/) so your saves follow you between
  devices.
- [Transfer games over your LAN](/guides/lan-transfers/) instead of
  re-downloading them.
