import { Navigation, Focusable } from "@decky/ui";
import { useEffect, useState } from "react";
import type { LanPeer, PeerGame } from "../types";
import { fmtBytes } from "../lib/format";
import { useServerBase } from "../hooks/use-server-base";
import { useParams } from "../lib/steam";
import { STEAM_FOOTER_HEIGHT } from "../lib/steam-chrome";

export function PeerGameDetailPage() {
  const { peerAddr, peerPort, gameId } =
    useParams<{ peerAddr: string; peerPort: string; gameId: string }>();
  const { base, error: baseError } = useServerBase();

  const [game, setGame] = useState<PeerGame | null>(null);
  const [peer, setPeer] = useState<LanPeer | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isDownloading, setIsDownloading] = useState(false);

  const peerBase = base
    ? `${base}/lan/peers/${peerAddr}/${peerPort}`
    : null;

  // Fetch game details from the peer's game list.
  useEffect(() => {
    if (!peerBase) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${peerBase}/games`);
        if (!res.ok) throw new Error();
        const games = (await res.json()) as PeerGame[];
        const found = games.find((g) => g.id === gameId) ?? null;
        if (!cancelled) {
          if (found) setGame(found);
          else setError("Game not found on this device.");
        }
      } catch {
        if (!cancelled) setError("Couldn't reach this device.");
      }
    })();
    return () => { cancelled = true; };
  }, [peerBase, gameId]);

  // Fetch peer info (device name) from the local peer list.
  useEffect(() => {
    if (!base) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${base}/lan/peers`);
        const peers = (await res.json()) as LanPeer[];
        const found = peers.find(
          (p) => p.addr === peerAddr && String(p.file_server_port) === peerPort,
        ) ?? null;
        if (!cancelled && found) setPeer(found);
      } catch {
        // device name is optional — fall back to addr
      }
    })();
    return () => { cancelled = true; };
  }, [base, peerAddr, peerPort]);

  async function startDownload() {
    if (!base || !game || isDownloading) return;
    setIsDownloading(true);
    try {
      const res = await fetch(`${base}/lan/install`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          peer_addr: peerAddr,
          peer_port: Number(peerPort),
          game_id: gameId,
        }),
      });
      if (!res.ok) throw new Error();
    } catch {
      setIsDownloading(false);
      return;
    }
    Navigation.NavigateBack();
  }

  const err = baseError ?? error;
  if (err) return <div style={{ padding: "2rem", opacity: 0.8 }}>{err}</div>;
  if (!game) return <div style={{ padding: "2rem", opacity: 0.7 }}>Loading…</div>;

  const coverUrl = peerBase ? `${peerBase}/games/${game.id}/cover` : null;
  const deviceName = peer?.device_name || peerAddr;
  const sizeLabel =
    game.install_size_mb > 0
      ? fmtBytes(Math.round(game.install_size_mb * 1024 * 1024))
      : null;

  return (
    <Focusable
      style={{
        position: "relative",
        height: "100%",
        background: "#0e1823",
        overflow: "hidden",
      }}
    >
      {/* Full-bleed background */}
      {coverUrl && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            backgroundImage: `url(${coverUrl})`,
            backgroundSize: "cover",
            backgroundPosition: "center",
            filter: "blur(3px) brightness(0.35)",
            transform: "scale(1.08)",
          }}
        />
      )}

      {/* Gradient */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          background:
            "linear-gradient(to bottom, rgba(14,24,35,0.25) 0%, rgba(14,24,35,0.55) 50%, rgba(14,24,35,0.92) 100%)",
        }}
      />

      {/* Content anchored to bottom */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          flexDirection: "column",
          justifyContent: "flex-end",
          // Clear the Game-Mode footer bar that overlays the bottom of the page.
          padding: `1rem 1.5rem calc(${STEAM_FOOTER_HEIGHT}px + 1.5rem)`,
        }}
      >
        {/* Portrait cover thumbnail */}
        {coverUrl && (
          <div
            style={{
              width: "72px",
              height: "96px",
              borderRadius: "5px",
              overflow: "hidden",
              boxShadow: "0 4px 18px rgba(0,0,0,0.65)",
              marginBottom: "0.65rem",
              flexShrink: 0,
            }}
          >
            <img
              src={coverUrl}
              alt={game.game_name}
              style={{ width: "100%", height: "100%", objectFit: "cover" }}
            />
          </div>
        )}

        <h2
          style={{
            margin: "0 0 0.35rem",
            fontSize: "1.35rem",
            fontWeight: 700,
            lineHeight: 1.2,
            textShadow: "0 1px 6px rgba(0,0,0,0.7)",
          }}
        >
          {game.game_name}
        </h2>

        <div
          style={{
            display: "flex",
            gap: "1rem",
            opacity: 0.75,
            fontSize: "0.82rem",
            marginBottom: "1rem",
          }}
        >
          <span>{deviceName}</span>
          {sizeLabel && <span>{sizeLabel}</span>}
          {!game.shareable && (
            <span style={{ color: "#f0a500" }}>Not shared</span>
          )}
        </div>

        <Focusable
          onActivate={() => void startDownload()}
          style={{
            alignSelf: "flex-start",
            padding: "0.5rem 1.4rem",
            borderRadius: "5px",
            background:
              !game.shareable || isDownloading ? "#2a3a52" : "#3d7ab5",
            fontWeight: 700,
            fontSize: "0.95rem",
            cursor:
              game.shareable && !isDownloading ? "pointer" : "default",
            border: "none",
            color: "#fff",
            opacity: !game.shareable || isDownloading ? 0.6 : 1,
          }}
        >
          {isDownloading
            ? "Starting…"
            : !game.shareable
              ? "Not available"
              : "⬇  Download"}
        </Focusable>
      </div>
    </Focusable>
  );
}
