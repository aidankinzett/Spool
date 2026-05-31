---
title: LAN Game Sharing
description: How Spool discovers other instances on the local network and streams installed games between them — UDP broadcast discovery, an in-process HTTP server, blake3 content-addressed manifests, and resumable parallel downloads.
sidebar:
  order: 6
---

Two machines on the same network running Spool can see each other's libraries and copy installed
games directly between them — no internet, no re-download from the store. One Steam Deck pulls
*ULTRAKILL* off another in a few minutes over Wi-Fi, and it lands in the receiver's library ready to
launch.

The subsystem lives in [`lan/`](https://github.com/aidankinzett/Spool/tree/master/tauri/src-tauri/src/lan),
split into three parts:

- **`discovery.rs`** — find peers via UDP broadcast.
- **`server.rs`** — an in-process HTTP server that exposes this instance's shared games.
- **`install.rs`** — the receiver: fetch a peer's manifest and stream the files down.

`mod.rs` holds the shared wire types (`PeerGame`, `PeerFile`, `PeerGameManifest`) and the host-side
uploads ledger.

## The shape of a transfer

```
Host A (sender)                          Host B (receiver)
───────────────                          ─────────────────
announce every 5s ─── UDP broadcast ───▶ peer registry (B sees A)
  :47631                                   │
                                           │ user clicks Install
GET /games/<id>/manifest  ◀──────────────┤ fetch manifest
  walk folder, blake3 every file          │
  ───────────────────────────────────────▶ PeerGameManifest (files + hashes + sizes)
                                           │
GET /games/<id>/files/<path>  ◀───────────┤ stream up to 4 files in parallel
  (HTTP Range for resume)                 │ → write to <name>.partial/
  ───────────────────────────────────────▶ verify blake3 per file
                                           │
                                           │ all files OK → rename .partial → final
                                           │ → add to library
```

## Discovery: UDP broadcast

Every instance runs an announce loop that broadcasts a small JSON packet every 5 seconds, and a
listen loop that collects everyone else's announces into a peer registry.

```rust
const BROADCAST_ADDR: Ipv4Addr = Ipv4Addr::BROADCAST; // 255.255.255.255
const DISCOVERY_PORT: u16 = 47631;
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(5);
const PEER_STALE_AFTER: Duration = Duration::from_secs(30);
```

The announce packet stays small enough to fit comfortably in one datagram, and carries everything a
browsing peer needs before it ever opens a TCP connection:

```rust
struct AnnouncePacket {
    magic: String,          // "spool" — fast reject for unrelated UDP on this port
    version: u32,
    device_id: String,
    device_name: String,
    game_count: u32,        // re-read fresh each tick, so peers see the count grow
    file_server_port: u16,  // 0 = "discovery only, not accepting transfers"
}
```

### Why broadcast, not multicast

Discovery uses limited broadcast (`255.255.255.255`) rather than a multicast group. Consumer mesh
routers — Google/Nest Wi-Fi especially — filter arbitrary admin-scoped multicast while still flooding
limited broadcasts normally, so multicast announces quietly vanish across the Wi-Fi↔Ethernet bridge
that a handheld and a desktop typically sit on opposite sides of. Broadcast is "ruder" (every host on
the link receives the packet), but it reliably traverses those bridges. The scope is the same either
way — routers don't forward `255.255.255.255` beyond the local segment — and the `magic` +
`device_id` checks make stray traffic cheap to discard.

### The socket

The socket is built through `socket2` rather than tokio directly, because two options must be set
before bind:

```rust
socket.set_reuse_address(true)?; // multiple Spool processes can listen (handy in dev)
socket.set_broadcast(true)?;     // required to send to 255.255.255.255
socket.set_nonblocking(true)?;
socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)).into())?;
```

`SO_REUSEADDR` means two instances on the same box can both bind `0.0.0.0:47631` and both still hear
each other — a locally-sent broadcast is delivered back to every socket bound to the port. There's no
loopback toggle to fiddle with (that was a multicast-only concern); instead the listen loop suppresses
*our own* announces by `device_id`:

```rust
if packet.device_id == our_device_id || packet.device_id.is_empty() {
    continue;
}
```

### Peer registry and staleness

Heard peers go into a `HashMap<device_id, PeerEntry>` with a `last_seen` timestamp. A reaper loop runs
every 5 s and drops anyone not heard from in 30 s (≈6 missed announces). The frontend is notified via a
`lan:peers-changed` event — but only when something *observable* changed (a new peer, a changed game
count, or the file server toggling between available and port-0), so a steady stream of identical
announces doesn't spam the UI.

The headless Decky plugin server reuses the same `listen_loop` and `reaper_loop` with a no-op change
callback (it polls `LanState::snapshot()` instead of pushing events), so peer discovery works without
the full Tauri app running.

## The HTTP server

When sharing is enabled, each instance also runs a small [axum](https://github.com/tokio-rs/axum) HTTP
server in-process. Its real port is what the announce packet advertises in `file_server_port`, so peers
always learn the live port from discovery rather than assuming a fixed one.

```rust
let router = Router::new()
    .route("/healthz", get(|| async { "ok" }))
    .route("/games", get(get_games_handler))
    .route("/games/:id/manifest", get(get_manifest_handler))
    .route("/games/:id/files/*path", get(get_file_handler))
    .route("/games/:id/cover", get(get_cover_handler))
    .route("/games/:id/hero", get(get_hero_handler))
    .route("/games/:id/cancel-check", get(get_cancel_check_handler));
```

Bind tries the user's preferred port (`lan_share_port`, default 47632) first and falls back to an
ephemeral port if it's taken — so a second instance on the same machine still comes up cleanly. The
returned port is stored in an `AtomicU16` that the announce loop reads each tick.

### Sharing is opt-in, twice

A game is only visible to peers when the user has flipped its **Sharing** toggle (`lan_shared`, default
`false`) *and* it has a real folder on disk to stream from:

```rust
let shareable = g.lan_shared && has_folder;
```

`/games` returns only `lan_shared` entries, in `PeerGame` form — a deliberately trimmed subset of
`GameEntry` that omits local filesystem paths (`exe_path`, `game_folder_path`, image paths) so browsing
a peer never leaks their disk layout. Every other endpoint re-checks `lan_shared` on each request and
returns **404** (not 403) for a non-shared id, so the *existence* of a private game never leaks across
the sharing boundary. If the user disables sharing mid-transfer, the next file request honours it.

### The manifest: walk + blake3

`/games/:id/manifest` walks the game's install folder and produces a flat list of every file with its
size, mtime, and a **blake3 hash**:

```rust
struct PeerFile {
    path: String,        // forward-slashed, relative to the install root
    size: u64,
    hash: String,        // blake3 hex; empty for zero-byte files
    mtime_unix_ms: u64,
}
```

Hashing reads every byte, so the whole walk runs on `spawn_blocking` to keep it off the async runtime.
The first manifest request for a big game is genuinely slow (~1 s/GB); after that an in-memory
`HashCache` keyed by absolute path → `(mtime, hash)` serves cached digests, re-hashing only files whose
mtime changed. The cache is an `Arc<RwLock<…>>` because reads dominate — concurrent manifest requests
probe it in parallel and only take the write lock on a genuine miss.

Paths in the manifest are relative and forward-slashed so the receiver can reconstruct local paths
across OSes, and symlinks are followed so junction/symlink-based installs ship their real bytes.

### File streaming with resume

`/games/:id/files/*path` streams one file. Two things make it robust:

- **Path-traversal safety.** The wildcard path is joined through `safe_join`, which rejects anything
  that isn't a plain `Component::Normal` segment — no `..`, no absolute paths, no Windows prefixes — so a
  malicious request can't escape the install root.
- **HTTP range resume.** The server honours `Range: bytes=N-`: the receiver sends how many bytes it
  already has and the server seeks past them and streams the rest. `Accept-Ranges: bytes` is set on every
  response. Only the `bytes=N-` form is supported; suffix (`bytes=-N`) and multi-range requests get 416.

### Graceful shutdown

The server is wired with axum's `with_graceful_shutdown`, coordinated by `LanServerShutdown` on managed
state. When the tray "Quit Spool" fires, it signals the notify, stops accepting new connections, lets
in-flight responses drain, and awaits the server task (bounded by a 2 s timeout) — so quitting the host
doesn't rip the connection out from under a peer mid-download.

## The receiver

`start_peer_install` kicks off a download. It returns immediately after queuing the work so the UI can
render an in-flight row right away; the heavy lifting runs in a spawned task.

### One install at a time

`LanDownloadState` is a single-slot guard — a second `start_peer_install` while one is running is
rejected with "Another LAN install is already in progress". The receiver mints a session UUID
(`install_token`) up front and immediately emits a "Fetching manifest…" placeholder, because the host's
first-request hashing means there's otherwise tens of seconds of dead air after the Install click.

### `.partial` staging

Files stream into a `<name>.partial` directory, never the final location. If a `<name>.partial` already
exists from an interrupted run and the final dir doesn't, the install **resumes into it**; otherwise a
fresh non-colliding dir is allocated. Only once *every* file has landed and verified does the receiver
flip it into place atomically:

```rust
tokio::fs::rename(&partial_dir, &final_dir).await?;
```

An interrupted transfer therefore never leaves a half-written game masquerading as installed — the
library entry is added only after the rename.

### Parallel, verified, resumable, throttled

Up to four files download concurrently via `buffer_unordered`:

```rust
const LAN_PARALLEL_FILES: usize = 4;
let mut stream = futures_util::stream::iter(file_futures).buffer_unordered(LAN_PARALLEL_FILES);
```

Each file fetch:

- **Resumes.** It probes the on-disk remnant: if it already matches the expected size *and* hash, the
  GET is skipped entirely; if it's a valid partial, a `Range` request appends the rest; otherwise it
  re-fetches from scratch.
- **Verifies.** A blake3 hasher runs alongside the disk writes (pre-seeded with the on-disk prefix when
  resuming). On a digest mismatch the file is moved aside as `<name>.bad` for inspection and the attempt
  fails; the retry re-fetches from scratch since the target path is now free. (A file whose manifest
  hash is empty — zero-byte files, older peers — skips verification rather than failing closed.)
- **Retries** transient network errors up to 5 times with exponential backoff, each retry resuming from
  the partial.
- **Restamps mtime** to match the source, so repeated installs across machines stay consistent and
  mtime-keyed tooling doesn't see spurious changes.
- **Throttles.** A shared byte counter keeps all four parallel tasks collectively under
  `lan_download_max_mbps` (0 = unlimited).

### Cancellation, both directions

Either side can stop a transfer:

- **Receiver cancels** → a cooperative flag checked between chunks; the `.partial` dir is wiped so a
  fresh attempt doesn't inherit half-written state.
- **Host cancels** (from their uploads panel) → the receiver's heartbeat polls `/cancel-check` and the
  per-file requests start returning **410 Gone**, which the receiver treats as a clean host-initiated
  abort.

### The host's view

The sender tracks each receiver as an `UploadSession` (registered the moment the manifest is fetched,
so the host sees the game name and total size before any file arrives). Parallel file fetches from one
receiver share a session id, progress is credited optimistically at request time, and `lan:uploads-changed`
emissions are throttled to ~5 Hz. A reaper drops sessions ~8 s after the last file request so finished
or cancelled transfers fall off the UI naturally.

### Landing in the library

After the rename, the receiver builds a `GameEntry` from the manifest metadata — name, developer,
genres, Steam/GOG ids, save paths — sets `game_folder_path` to the new install dir, derives `exe_path`
from the manifest's `exe_relative_path` (left empty if the source's exe lived outside its game folder,
for the user to wire up), tags `install_source: "lan"`, records the source device, and emits
`library:changed`. A cover image is prefetched from the peer during the transfer so the row has a
thumbnail immediately.

## Integrity end to end

blake3 is the throughline. The host hashes every file when building the manifest; the receiver verifies
every file as it lands, and can re-verify an on-disk remnant to decide whether to skip, resume, or
refetch. Combined with `.partial` staging and the atomic rename, the guarantee is: a game that appears
in the receiver's library is byte-for-byte identical to the source, or it never appears at all.

## Events

| Event | Emitted by | Meaning |
|-------|-----------|---------|
| `lan:peers-changed` | discovery listen/reaper | peer set changed (new/gone peer, count or port change) |
| `lan:download` | receiver | per-transfer progress (`starting` → `transferring` → `done`/`error`/`canceled`) |
| `lan:uploads-changed` | host server | the host's active-uploads ledger changed |
| `library:changed` | receiver, post-install | a LAN-installed game was added |

## Configuration

| Config field | Default | Role |
|--------------|---------|------|
| `lan_share_enabled` | `false` | master switch for serving + announcing the file server |
| `lan_share_port` | 47632 | preferred HTTP server port (ephemeral fallback if taken) |
| `lan_install_dir` | `…/Spool/lan-games/` | where received games are installed |
| `lan_download_max_mbps` | 0 | receiver-side bandwidth cap (0 = unlimited) |
| `device_name` | — | shown to peers in discovery and on installed entries |

Per-game, the **Sharing** toggle in the Edit dialog sets `lan_shared`.
