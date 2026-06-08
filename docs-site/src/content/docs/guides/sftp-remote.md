---
title: Self-hosted SFTP Remote
description: Sync your saves to your own machine over SFTP — a Raspberry Pi, NAS, or home server — instead of a public cloud provider.
---

Spool's cloud sync runs on [rclone](https://rclone.org/), and rclone speaks
SFTP. So instead of Google Drive or Dropbox you can point Spool at any machine
you can SSH into — a Raspberry Pi, a NAS, an old PC in a cupboard. On a local
network this is usually much faster than a public cloud provider, with no
per-account storage limits, and your saves never leave your house.

This guide sets up an SFTP remote by hand with `rclone config`, then tells Spool
to use it via the **Custom** provider.

## What you need

- A machine you can reach over SSH (the "server" — e.g. a Raspberry Pi) that
  stays on when you want to sync.
- SSH access to it: its hostname or IP address, a username, and either a
  password or an SSH key.
- A folder on that machine to hold the backups.

## How Spool finds the remote

Spool runs a bundled copy of rclone, but it reads the **standard rclone config
file** — `~/.config/rclone/rclone.conf` on Linux, `%APPDATA%\rclone\rclone.conf`
on Windows. Any remote you create with the normal `rclone config` command lands
in that file, and Spool's bundled rclone picks it up automatically. You don't
need to point Spool at a separate config.

If you don't already have rclone on the device running Spool, install it from
[rclone.org/downloads](https://rclone.org/downloads/) just to run the
config wizard once. (The Linux gaming distros — Bazzite, CachyOS, SteamOS —
generally already have it, or you can `rclone config` from a desktop terminal.)

## 1. Prepare the server

On the server, make sure an SSH server is running and create a folder for the
backups. On a Raspberry Pi:

```bash
# Enable SSH if it isn't already (Raspberry Pi OS):
sudo raspi-config nonint do_ssh 0

# Make a folder to hold Spool's data:
mkdir -p ~/spool
```

Note the server's address. On a home network a static IP or a `.local`
hostname (e.g. `raspberrypi.local`) is easiest, so it doesn't change between
reboots.

## 2. Create the rclone remote

On the device running Spool, start the wizard:

```bash
rclone config
```

Then:

1. `n` — **New remote**.
2. Name it something short and memorable, e.g. `pi`. You'll type this name into
   Spool later.
3. For the storage type, choose **SFTP** (`sftp`).
4. **host** — the server's address, e.g. `raspberrypi.local` or `192.168.1.50`.
5. **user** — your SSH username on the server.
6. **port** — leave blank for the default (22).
7. Authentication — pick one:
   - **Password:** choose to enter a password (`y`) and type it. rclone stores
     it obscured in the config file.
   - **SSH key (recommended):** leave the password blank and set
     `key_file` to your private key path (e.g. `~/.ssh/id_ed25519`). This
     avoids putting a password in the config and is the smoother option for
     unattended syncing.
8. Accept the defaults for the remaining advanced options, then confirm and
   quit the wizard (`q`).

Check it works before involving Spool:

```bash
# Lists the directories under your home on the server — should connect cleanly:
rclone lsd pi:
```

If that errors, fix the connection (host, user, key/password, firewall) before
moving on — Spool can only sync once rclone itself can.

## 3. Point Spool at it

In Spool, open **Settings → Saves**:

1. Set **Provider** to **Custom (rclone remote)**.
2. In the **Remote** field, enter the remote name you chose — `pi`.
3. Set the **Base folder** to the path on the server where backups should
   live, e.g. `spool` (relative to the SSH user's home) or an absolute path
   like `/home/pi/spool`.

Spool stores your save backups under `<base>/ludusavi-backup` and a little
cross-device coordination data under `<base>/_spool`.

That's it — launch a game and Spool will restore from and back up to your own
server. Spool checks the remote's reachability and shows the status in
Settings; if the server is asleep or off the network, a sync probe fails fast
rather than hanging your launch.

## Using it across devices

Point every device — your PC and your Steam Deck — at the **same remote and
base folder**. Each device needs the rclone remote configured locally (repeat
step 2 on each), but they all read and write the same folder on the server, so
playtime, the "saves backed up" badge, and the unsynced-session warning are
pooled across them. See [Cloud Save Sync](/guides/cloud-saves/) for how a
session syncs and how conflicts are resolved.

## Tips

- **Keep the server reachable.** Sync only happens when the device can reach the
  server. On the same LAN that's instant; to sync from outside your home you'd
  need a VPN (e.g. Tailscale) or port forwarding — covering that is beyond this
  guide.
- **SSH keys beat passwords** for something that syncs in the background — no
  password sits in the config file, and there's nothing to re-enter.
- **Back up the server's folder too.** A self-hosted remote is only as durable
  as the disk it's on. If the saves on your server matter, make sure that disk
  is itself backed up.
