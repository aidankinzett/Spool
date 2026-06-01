import { Focusable } from "@decky/ui";
import type { LanPeer, PeerGame } from "../types";
import { fmtBytes } from "../lib/format";

export function PeerGameDetailPage({
  base,
  peer,
  game,
  onDownload,
  isDownloading,
}: {
  base: string;
  peer: LanPeer;
  game: PeerGame;
  onDownload: () => void;
  isDownloading: boolean;
}) {
  const peerBase = `${base}/lan/peers/${peer.addr}/${peer.file_server_port}`;
  const coverUrl = `${peerBase}/games/${game.id}/cover`;

  const sizeMb = game.install_size_mb;
  const sizeLabel =
    sizeMb > 0 ? fmtBytes(Math.round(sizeMb * 1024 * 1024)) : null;

  const bgSrc = coverUrl;

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
      <div
        style={{
          position: "absolute",
          inset: 0,
          backgroundImage: `url(${bgSrc})`,
          backgroundSize: "cover",
          backgroundPosition: "center",
          filter: "blur(3px) brightness(0.35)",
          transform: "scale(1.08)",
        }}
      />

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
          padding: "1rem 1.5rem calc(42px + 1.5rem)",
        }}
      >
        {/* Portrait cover thumbnail */}
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
          <span>{peer.device_name || peer.addr}</span>
          {sizeLabel && <span>{sizeLabel}</span>}
          {!game.shareable && (
            <span style={{ color: "#f0a500" }}>Not shared</span>
          )}
        </div>

        {/* Download / disabled button */}
        <Focusable
          onActivate={() => {
            if (game.shareable && !isDownloading) onDownload();
          }}
          style={{
            alignSelf: "flex-start",
            padding: "0.5rem 1.4rem",
            borderRadius: "5px",
            background: !game.shareable
              ? "#2a3a52"
              : isDownloading
                ? "#2a3a52"
                : "#3d7ab5",
            fontWeight: 700,
            fontSize: "0.95rem",
            cursor: game.shareable && !isDownloading ? "pointer" : "default",
            border: "none",
            color: "#fff",
            opacity: !game.shareable || isDownloading ? 0.6 : 1,
          }}
        >
          {isDownloading ? "Downloading…" : !game.shareable ? "Not available" : "⬇  Download"}
        </Focusable>
      </div>
    </Focusable>
  );
}
