import { Focusable } from "@decky/ui";
import type { LibraryGame } from "../types";
import { launchLibraryGame } from "../lib/launch";
import { formatPlaytime, formatRelativeTime } from "../lib/format";

interface Props {
  game: LibraryGame;
  coverUrl: string | null;
  base: string;
  onBack: () => void;
}

export function GameDetailPanel({ game, coverUrl, base, onBack }: Props) {
  const accent = game.accent_color ?? "#1a2330";

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        background: "#0e1823",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        zIndex: 10,
      }}
    >
      {/* Hero / cover strip */}
      <div
        style={{
          position: "relative",
          width: "100%",
          height: "220px",
          flexShrink: 0,
          background: accent,
          overflow: "hidden",
        }}
      >
        {coverUrl && (
          <img
            src={coverUrl}
            alt={game.game_name}
            style={{
              position: "absolute",
              inset: 0,
              width: "100%",
              height: "100%",
              objectFit: "cover",
              objectPosition: "center top",
            }}
          />
        )}
        {/* Gradient so text below is legible against any cover */}
        <div
          style={{
            position: "absolute",
            bottom: 0,
            left: 0,
            right: 0,
            height: "80px",
            background: "linear-gradient(transparent, #0e1823)",
          }}
        />
      </div>

      {/* Content */}
      <div style={{ padding: "1rem 1.5rem", flex: 1, overflowY: "auto" }}>
        {/* Title */}
        <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.3rem", fontWeight: 700, lineHeight: 1.2 }}>
          {game.game_name}
        </h2>

        {/* Meta row */}
        <div
          style={{
            display: "flex",
            gap: "1.25rem",
            opacity: 0.75,
            fontSize: "0.85rem",
            marginBottom: "1.25rem",
          }}
        >
          {game.playtime_minutes > 0 && (
            <span>⏱ {formatPlaytime(game.playtime_minutes)}</span>
          )}
          {game.last_played_at && (
            <span>🕹 {formatRelativeTime(game.last_played_at)}</span>
          )}
          {game.sync_badge && (
            <span style={{ color: "#f0a500" }}>☁ {game.sync_badge}</span>
          )}
        </div>

        {/* Actions */}
        <Focusable style={{ display: "flex", gap: "0.75rem" }}>
          <Focusable
            onActivate={() => {
              onBack();
              void launchLibraryGame(base, game.id, game.shortcut_app_id ?? null);
            }}
            style={{
              padding: "0.6rem 1.5rem",
              borderRadius: "6px",
              background: accent,
              fontWeight: 700,
              fontSize: "1rem",
              cursor: "pointer",
              border: "none",
              color: "#fff",
            }}
          >
            Play
          </Focusable>
          <Focusable
            onActivate={onBack}
            style={{
              padding: "0.6rem 1.25rem",
              borderRadius: "6px",
              background: "#2a3a52",
              fontWeight: 600,
              fontSize: "1rem",
              cursor: "pointer",
              border: "none",
              color: "#fff",
            }}
          >
            Back
          </Focusable>
        </Focusable>
      </div>
    </div>
  );
}
