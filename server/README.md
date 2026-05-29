# Spool Lock Server

Lightweight sync server that prevents two devices from playing the same game simultaneously, and (optionally) stores your saves so they sync between devices. Self-host it on a Raspberry Pi, home server, or any Linux box.

## Quick start

**Prerequisites:** Docker and Docker Compose (or Podman Compose).

```bash
# 1. Download the compose file
curl -O https://raw.githubusercontent.com/aidankinzett/Spool/master/server/docker-compose.yml

# 2. Generate a strong admin secret and set it in docker-compose.yml
openssl rand -base64 32
#    Copy the output, then replace "changeme" in docker-compose.yml with it.

# 3. Start the server
docker compose up -d

# 4. Verify it's running
curl http://localhost:47633/health
# → {"ok":true}
```

The database is persisted in `./data/ludusavi.db` on your host machine.

## Register an account

Open Spool on any of your PCs, go to **Settings → Sync Server**, enable the toggle, enter the server URL (e.g. `http://raspberrypi.local:47633`), and click **Register...**. Enter your admin secret and a username, then click **Register** — the API key is filled in automatically.

Alternatively, via curl:

```bash
curl -X POST http://your-server:47633/auth/register \
  -H "X-Admin-Secret: your-admin-secret" \
  -H "Content-Type: application/json" \
  -d '{"username": "mypc"}'
# → {"api_key":"xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"}
```

Copy the `api_key` into the API Key field in Settings.

## Connect from Spool

1. Open **Settings → Sync Server**
2. Toggle it on
3. Click **Scan LAN** — if the server is on your local network, the URL is filled automatically
4. Or enter the URL manually: `http://raspberrypi.local:47633` (or the Pi's IP address)
5. Paste your API key and save

## Save storage (turnkey cloud saves)

The bundled `docker-compose.yml` includes an optional **`spool-storage`** service:
an [rclone](https://rclone.org/) WebDAV server over the same `./data` volume.
With it enabled, Spool syncs each account's saves to your server — no third-party
cloud, no per-device `rclone config`. Under the hood Spool runs
`ludusavi cloud set webdav` for you, pointing ludusavi at this server.

How it fits together:

- `spool-storage` runs `rclone serve webdav` on port **47634**.
- Authentication is delegated back to `spool-lock`: rclone's `--auth-proxy` calls
  the lock server's internal `/internal/webdav-auth` endpoint, which validates the
  account's API key and confines each login to its own directory
  (`/data/saves/<account-id>`). Your WebDAV username/password are simply your
  Spool account username and API key — no separate credentials.

To enable it:

1. In `docker-compose.yml`, set **`WEBDAV_PUBLIC_URL`** on `spool-lock` to the URL
   clients should reach the store at (e.g. `https://myserver.example.com` behind a
   TLS reverse proxy, or `http://raspberrypi.local:47634` on a trusted LAN).
2. Set a strong **`WEBDAV_AUTH_SECRET`** and make it **identical** on both
   `spool-lock` and `spool-storage`.
3. `docker compose up -d --build` (the storage image is built from
   `Dockerfile.rclone`).
4. In Spool: **Settings → Cloud saves → Use my Spool server for save storage**.

Leaving `WEBDAV_PUBLIC_URL` blank disables save storage — the lock server still
runs and `GET /storage` returns 404.

> **Security:** `/internal/webdav-auth` is served on the lock server's public port
> and protected only by `WEBDAV_AUTH_SECRET`. Use a long random value. For WAN
> access, terminate TLS at a reverse proxy and don't expose 47634 unencrypted.

Saves live under `./data/saves/<account-id>/` on the host — include it in backups.

## Updating

```bash
docker compose pull
docker compose up -d --build
```

## Networking notes

- **LAN access:** The server listens on port 47633. On the host machine, ensure port 47633 is not blocked by the firewall.
- **mDNS:** The container hostname is `ludusavi-lock`. On Raspberry Pi OS (which runs Avahi), the host's `.local` name (`raspberrypi.local` or similar) is what clients will use to reach it — mDNS does not automatically advertise the container's hostname.
- **WAN access:** For access outside your LAN, put the server behind a reverse proxy (e.g. Caddy or nginx) with HTTPS. Do not expose port 47633 directly to the internet without TLS.
- **Backup the database:** The entire state lives in `./data/ludusavi.db`. Copy this file periodically to back it up.

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `ADMIN_SECRET` | *(required)* | Secret used to create new user accounts |
| `PORT` | `47633` | HTTP listen port |
| `DATABASE_PATH` | `/data/spool.db` | Path to the SQLite database file |
| `WEBDAV_PUBLIC_URL` | *(empty)* | Public URL of the WebDAV save store. Empty = save storage disabled |
| `WEBDAV_AUTH_SECRET` | `ADMIN_SECRET` | Shared secret for the rclone auth callback. Must match on both services |
| `SAVES_DIR` | `/data/saves` | Root directory for stored saves (per-account subdirs) |

### `spool-storage` service

| Variable | Default | Description |
|---|---|---|
| `WEBDAV_PORT` | `47634` | WebDAV listen port |
| `SAVES_DIR` | `/data/saves` | Root directory served (per-account subdirs) |
| `SPOOL_AUTH_URL` | `http://spool-lock:47633/internal/webdav-auth` | Lock server auth callback |
| `WEBDAV_AUTH_SECRET` | *(required)* | Shared secret; must match `spool-lock` |
