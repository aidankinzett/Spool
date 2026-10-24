---
title: Installing umu-launcher
description: Install umu-launcher (umu-run) so Spool can run Windows games through Proton on Linux, including Steam Deck / SteamOS.
---

On Linux, Spool launches Windows `.exe` games through **Proton** using
[umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) (`umu-run`).
It's the one runtime dependency Spool doesn't bundle — ludusavi and rclone ship
inside the app, but umu-launcher is a Python application that runs games inside
Valve's Steam Linux Runtime container, so it's installed from your distribution
instead.

Spool only needs `umu-run` on your `PATH` (or pointed at directly in
**Settings → Compatibility → umu-run path**). On first launch umu downloads the
Steam Runtime it needs into `~/.local/share/umu` — this is normal and happens
once.

**Settings → Compatibility** runs a dependency check that tells you whether
`umu-run` was found and shows the right command for your distribution.

## Using the Spool install script

The Linux install script downloads umu-launcher for you when it can't find an
existing `umu-run` (in `/usr/bin` or Heroic's tools folder). It fetches
upstream's zipapp release — a single pure-Python file, so the same download
works on x86_64 and ARM64 — into `~/.local/share/Spool/tools/umu/umu-run`,
where Spool looks for it. No root access is needed, so this also works on
SteamOS and other read-only systems. It needs `python3` 3.10 or newer.

```bash
curl -fsSL https://raw.githubusercontent.com/aidankinzett/Spool/master/scripts/install-appimage.sh | bash
```

Re-running the script on an existing install also updates that copy of umu.
To install only umu-launcher and leave your Spool install alone (for example if
you installed Spool from the AUR), pass `--umu-only`:

```bash
curl -fsSL https://raw.githubusercontent.com/aidankinzett/Spool/master/scripts/install-appimage.sh | bash -s -- --umu-only
```

Pass `--no-umu` to skip the umu step instead. A package
from your distribution, below, gets updated with the rest of the system
instead.

## Per-distribution install

### Bazzite

Nothing to do — **Bazzite ships `umu-run` preinstalled**. If the dependency
check still reports it missing, your install predates the package; update the
system (`ujust update` / `rpm-ostree upgrade`) and reboot.

### Arch, CachyOS, Manjaro, EndeavourOS

```bash
paru -S umu-launcher        # or: yay -S umu-launcher
```

It's in the `multilib` repository, so `sudo pacman -S umu-launcher` works too
once `multilib` is enabled.

### Fedora / Nobara / RHEL

```bash
sudo dnf install umu-launcher
```

### openSUSE

```bash
sudo zypper install umu-launcher
```

### Debian / Ubuntu / Pop!_OS

umu-launcher isn't in `apt` yet. Grab a build from the
[releases page](https://github.com/Open-Wine-Components/umu-launcher/releases),
or use the home-directory build described under SteamOS below (it isn't
SteamOS-specific).

## SteamOS / Steam Deck

SteamOS does **not** ship `umu-run` by default, and the usual package-manager
route doesn't apply: the system partition is **read-only**, `pacman`/AUR aren't
available, and anything written into the system image is wiped on the next
SteamOS update.

The [install script](#using-the-spool-install-script) handles this. To build
from source instead, build umu into your **home directory**, which is writable and survives
system updates. Do this from **Desktop Mode**:

```bash
git clone https://github.com/Open-Wine-Components/umu-launcher
cd umu-launcher
./configure.sh --user-install
make
make install
```

This installs `umu-run` under `~/.local/bin`. Make sure that directory is on
your `PATH`:

```bash
echo $PATH | tr ':' '\n' | grep -q "$HOME/.local/bin" && echo "on PATH" || \
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bash_profile
```

Then either restart Spool so it re-scans, or set the path explicitly in
**Settings → Compatibility → umu-run path** to `~/.local/bin/umu-run`. The
dependency check should flip to found.

:::note
Because the build lives in your home directory it persists across SteamOS
updates — you don't have to reinstall it after each update the way a
system-partition install would require.
:::

## Verifying

Once installed, confirm `umu-run` is reachable:

```bash
umu-run --help
```

In Spool, open **Settings → Compatibility** and use **Rescan** — `umu-run`
should show as found, with the path it resolved to.
