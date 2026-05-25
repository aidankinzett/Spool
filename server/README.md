# ludusavi-wrap Lock Server

Lightweight sync server that prevents two devices from playing the same game simultaneously. Self-host it on a Raspberry Pi, home server, or any Linux box.

## Quick start

**Prerequisites:** Docker and Docker Compose (or Podman Compose).

```bash
# 1. Download the compose file
curl -O https://raw.githubusercontent.com/akinz/ludusavi-wrap/master/server/docker-compose.yml

# 2. Set a strong admin secret (used once to register accounts)
#    Edit docker-compose.yml and replace "changeme" with a real secret.

# 3. Start the server
docker compose up -d

# 4. Verify it's running
curl http://localhost:3000/health
# → {"ok":true}
```

The database is persisted in `./data/ludusavi.db` on your host machine.

## Register an account

Open ludusavi-wrap on any of your PCs, go to **Settings → Sync Server**, enable the toggle, enter the server URL (e.g. `http://raspberrypi.local:3000`), and click **Register...**. Enter your admin secret and a username, then click **Register** — the API key is filled in automatically.

Alternatively, via curl:

```bash
curl -X POST http://your-server:3000/auth/register \
  -H "X-Admin-Secret: your-admin-secret" \
  -H "Content-Type: application/json" \
  -d '{"username": "mypc"}'
# → {"api_key":"xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"}
```

Copy the `api_key` into the API Key field in Settings.

## Connect from ludusavi-wrap

1. Open **Settings → Sync Server**
2. Toggle it on
3. Click **Scan LAN** — if the server is on your local network, the URL is filled automatically
4. Or enter the URL manually: `http://raspberrypi.local:3000` (or the Pi's IP address)
5. Paste your API key and save

## Updating

```bash
docker compose pull
docker compose up -d
```

## Networking notes

- **LAN access:** The server listens on port 3000. On the host machine, ensure port 3000 is not blocked by the firewall.
- **mDNS:** The container hostname is `ludusavi-lock`. On Raspberry Pi OS (which runs Avahi), the host's `.local` name (`raspberrypi.local` or similar) is what clients will use to reach it — mDNS does not automatically advertise the container's hostname.
- **WAN access:** For access outside your LAN, put the server behind a reverse proxy (e.g. Caddy or nginx) with HTTPS. Do not expose port 3000 directly to the internet without TLS.
- **Backup the database:** The entire state lives in `./data/ludusavi.db`. Copy this file periodically to back it up.

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `ADMIN_SECRET` | *(required)* | Secret used to create new user accounts |
| `PORT` | `3000` | HTTP listen port |
| `DATABASE_PATH` | `/data/ludusavi.db` | Path to the SQLite database file |
