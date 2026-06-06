import { toaster } from "@decky/api";
import { DialogButton, Focusable } from "@decky/ui";
import { useEffect, useRef, useState } from "react";
import type { DownloadProgress, LanPeer, PeerGame } from "../types";
import { fmtBytes } from "../lib/format";
import { useServerBase } from "../hooks/use-server-base";
import { useParams } from "../lib/steam";

// A single peer's shared games, as a list. Each row is the game's cover, name
// and install size with a Download button; activating it installs the game in
// place (no separate detail page). The backend serves a single install at a
// time, so while one game downloads its row shows live progress + Cancel and
// every other Download button is greyed. Registered as its own route so the B
// button navigates back to the peers list (/spool/lan).
export function PeerGamesPage() {
  const { peerAddr, peerPort } =
    useParams<{ peerAddr: string; peerPort: string }>();
  const { base, error: baseError } = useServerBase();

  const [peer, setPeer] = useState<LanPeer | null>(null);
  const [games, setGames] = useState<PeerGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const peerBase = base ? `${base}/lan/peers/${peerAddr}/${peerPort}` : null;

  // Fetch peer info (device name) from the local peers list.
  useEffect(() => {
    if (!base) return;
    let cancelled = false;
    void fetch(`${base}/lan/peers`)
      .then((r) => r.json() as Promise<LanPeer[]>)
      .then((peers) => {
        const found =
          peers.find(
            (p) =>
              p.addr === peerAddr && String(p.file_server_port) === peerPort,
          ) ?? null;
        if (!cancelled && found) setPeer(found);
      })
      .catch(() => undefined);
    return () => { cancelled = true; };
  }, [base, peerAddr, peerPort]);

  // Fetch the game list.
  useEffect(() => {
    if (!peerBase) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${peerBase}/games`);
        if (!res.ok) throw new Error();
        const data = (await res.json()) as PeerGame[];
        if (!cancelled) setGames(data);
      } catch {
        if (!cancelled) setError("Couldn't reach this device.");
      }
    })();
    return () => { cancelled = true; };
  }, [peerBase]);

  // Pick up any in-flight download on mount (e.g. returned via B button).
  useEffect(() => {
    if (!base) return;
    void fetch(`${base}/lan/download`)
      .then((r) => r.json() as Promise<DownloadProgress | null>)
      .then((p) => {
        if (p && p.status !== "done" && p.status !== "error" && p.status !== "canceled") {
          setDownload(p);
          startPolling();
        }
      })
      .catch(() => undefined);
    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [base]);

  function startPolling() {
    if (!base || pollRef.current) return;
    pollRef.current = setInterval(() => {
      void fetch(`${base}/lan/download`)
        .then((r) => r.json() as Promise<DownloadProgress | null>)
        .then((p) => {
          setDownload(p);
          if (!p || p.status === "done" || p.status === "error" || p.status === "canceled") {
            if (pollRef.current) clearInterval(pollRef.current);
            pollRef.current = null;
            if (p?.status === "done") {
              toaster.toast({ title: "Install complete", body: p.game_name });
            } else if (p?.status === "error") {
              toaster.toast({ title: "Install failed", body: p.message ?? p.game_name });
            }
            setTimeout(() => setDownload(null), 3000);
          }
        })
        .catch(() => undefined);
    }, 500);
  }

  async function startDownload(game: PeerGame) {
    if (!base || busy) return;
    // Optimistically show the row as starting so the UI reacts before the first
    // poll lands; polling then takes over from the backend's real progress.
    setDownload({
      install_token: "",
      source_device_name: peer?.device_name ?? peerAddr,
      source_game_id: game.id,
      game_name: game.game_name,
      bytes_done: 0,
      bytes_total: 0,
      current_file: "",
      status: "starting",
      bytes_per_second: 0,
    });
    try {
      const res = await fetch(`${base}/lan/install`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          peer_addr: peerAddr,
          peer_port: Number(peerPort),
          game_id: game.id,
        }),
      });
      if (!res.ok) throw new Error();
    } catch {
      setDownload(null);
      return;
    }
    startPolling();
  }

  async function cancelDownload() {
    if (!base || !download) return;
    await fetch(`${base}/lan/download`, {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ install_token: download.install_token }),
    }).catch(() => undefined);
  }

  // A download occupies the single install slot while it's starting/transferring.
  const busy =
    download !== null &&
    (download.status === "starting" || download.status === "transferring");
  const deviceName = peer?.device_name || peerAddr;
  const err = baseError ?? error;

  return (
    <div style={{ padding: "2rem", display: "flex", flexDirection: "column", gap: "1rem" }}>
      <h2 style={{ margin: 0 }}>{deviceName}</h2>

      {err && <div style={{ opacity: 0.8 }}>{err}</div>}
      {!err && !games && <div style={{ opacity: 0.7 }}>Loading…</div>}
      {games && games.length === 0 && (
        <div style={{ opacity: 0.7 }}>This device isn't sharing any games.</div>
      )}

      {games && games.length > 0 && (
        <Focusable style={{ display: "flex", flexDirection: "column", gap: "0.6rem" }}>
          {games.map((g) => (
            <GameRow
              key={g.id}
              game={g}
              coverUrl={`${peerBase}/games/${g.id}/cover`}
              // The active download owns the slot; this row is it when the ids match.
              active={busy && download?.source_game_id === g.id ? download : null}
              // Any in-flight download greys every other row's Download button.
              disabled={busy}
              onDownload={() => void startDownload(g)}
              onCancel={() => void cancelDownload()}
            />
          ))}
        </Focusable>
      )}
    </div>
  );
}

// One game row: cover + name + size on the left, a Download button on the
// right — or, when this row is the active install, a live progress readout with
// a Cancel button and a progress bar across the bottom.
function GameRow({
  game,
  coverUrl,
  active,
  disabled,
  onDownload,
  onCancel,
}: {
  game: PeerGame;
  coverUrl: string;
  active: DownloadProgress | null;
  disabled: boolean;
  onDownload: () => void;
  onCancel: () => void;
}) {
  const sizeLabel =
    game.install_size_mb > 0
      ? fmtBytes(Math.round(game.install_size_mb * 1024 * 1024))
      : null;
  const pct =
    active && active.bytes_total > 0
      ? Math.round((active.bytes_done / active.bytes_total) * 100)
      : 0;

  return (
    <div
      style={{
        position: "relative",
        display: "flex",
        alignItems: "center",
        gap: "0.9rem",
        padding: "0.6rem 0.75rem",
        borderRadius: "8px",
        background: "#1a2330",
        overflow: "hidden",
      }}
    >
      {/* Portrait cover thumbnail */}
      <div
        style={{
          width: "44px",
          height: "59px",
          borderRadius: "4px",
          overflow: "hidden",
          flexShrink: 0,
          background: "#0e1620",
        }}
      >
        <img
          src={coverUrl}
          alt={game.game_name}
          style={{ width: "100%", height: "100%", objectFit: "cover" }}
        />
      </div>

      {/* Name + size (or live progress text while installing) */}
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "0.2rem" }}>
        <span
          style={{
            fontWeight: 600,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {game.game_name}
        </span>
        <span style={{ fontSize: "0.8rem", opacity: 0.7 }}>
          {active
            ? active.bytes_total > 0
              ? `${fmtBytes(active.bytes_done)} / ${fmtBytes(active.bytes_total)} · ${fmtBytes(Math.round(active.bytes_per_second))}/s`
              : active.current_file || "Starting…"
            : sizeLabel ?? ""}
        </span>
      </div>

      {/* Right-side action */}
      {active ? (
        <div style={{ display: "flex", alignItems: "center", gap: "0.75rem", flexShrink: 0 }}>
          <span style={{ fontVariantNumeric: "tabular-nums", fontWeight: 600, minWidth: "3ch", textAlign: "right" }}>
            {pct}%
          </span>
          <DialogButton style={{ minWidth: "110px" }} onClick={onCancel}>
            Cancel
          </DialogButton>
        </div>
      ) : (
        <DialogButton
          style={{ minWidth: "150px", flexShrink: 0 }}
          disabled={disabled || !game.shareable}
          onClick={onDownload}
        >
          {game.shareable ? "Download" : "Not available"}
        </DialogButton>
      )}

      {/* Progress bar across the bottom edge while installing */}
      {active && (
        <div
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            bottom: 0,
            height: "3px",
            background: "#2a3a52",
          }}
        >
          <div
            style={{
              height: "100%",
              width: `${pct}%`,
              background: "#4a90d9",
              transition: "width 0.3s",
            }}
          />
        </div>
      )}
    </div>
  );
}
