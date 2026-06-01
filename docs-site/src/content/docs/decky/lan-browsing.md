---
title: LAN browsing
description: Browsing LAN peers and downloading their shared games from Game Mode.
sidebar:
  order: 5
---

The plugin exposes Spool's [LAN game-sharing](../architecture/lan-sharing) from Game Mode: discover peers, browse their shared games, and download one into your library. The QAM library page has a **LAN** button that navigates to `/spool/lan`.

The headless server does the network work; the React UI only renders it. The Deck running the plugin is a **pure consumer** — the server's discovery listener watches for peer announces but the plugin doesn't announce itself or run a file server.

## Why the server proxies peer requests

The UI runs under `https://steamloopback.host`. A peer's file server is plain `http://<peer-ip>:<port>`, so the browser would block those fetches as mixed content. To dodge that, the UI never talks to a peer directly — it asks the **local** headless server (`http://127.0.0.1`, a secure origin) to proxy the request, and the server fetches from the peer server-side.

## Pages and polling

**`LanPage`** (`/spool/lan`) polls `GET ${base}/lan/peers` every 3 s and lists discovered peers — device name (or IP) and game count. A peer with no file server (`file_server_port === 0`, discovery-only) is dimmed and not selectable. Activating a peer navigates to `/spool/lan/:peerAddr/:peerPort`.

**`PeerGamesPage`** (`/spool/lan/:addr/:port`) shows that peer's shared games:

- `GET ${base}/lan/peers` — to resolve the peer's display name.
- `GET ${base}/lan/peers/:addr/:port/games` — the peer's game list (`PeerGame[]`), proxied server-side.
- `GET ${base}/lan/peers/:addr/:port/games/:id/cover` — each cover, proxied so the grid can `<img>`-load it by URL.

It also tracks an in-flight download: on mount and then every 500 ms (while a download is `starting` / `transferring`) it polls `GET ${base}/lan/download` and renders a progress box — game name, status, a progress bar, bytes done / total, and transfer speed — with a **Cancel** button that issues `DELETE ${base}/lan/download { install_token }`. It toasts on completion. Activating a game navigates to its detail page.

**`PeerGameDetailPage`** (`/spool/lan-game/:addr/:port/:gameId`) shows the game's size and whether it's shareable, with a **Download** button. The button posts:

```
POST ${base}/lan/install { peer_addr, peer_port, game_id }
```

which returns an `install_token`, then navigates back so the peer page's poller picks up the progress.

## Server side

`post_lan_install` (`plugin_server.rs`) resolves the install root (`lan.install_dir` from config, defaulting to `~/.local/share/Spool/lan-games`) and the bandwidth cap (`lan.download_max_mbps`), loads the library fresh, and kicks off `lan::install::begin_install` — the same content-addressed, blake3-verified, resumable transfer the desktop app uses ([LAN sharing](../architecture/lan-sharing)). There's a single in-flight install slot. Because the headless server has no Tauri event bus, progress isn't pushed as events — the install's progress and `library:changed` callbacks are no-ops, and the UI **polls** `GET /lan/download` instead.
