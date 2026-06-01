import { Navigation, Focusable } from "@decky/ui";
import { useEffect, useState } from "react";
import type { LibraryGame } from "../types";
import { launchLibraryGame } from "../lib/launch";
import { formatPlaytime, formatRelativeTime } from "../lib/format";
import { useServerBase } from "../hooks/use-server-base";
import { useParams } from "../lib/steam";

function coverUrl(base: string, g: LibraryGame): string | null {
  if (!g.cover_image_path) return null;
  const file = g.cover_image_path.split(/[/\\]/).pop();
  return file ? `${base}/covers/${encodeURIComponent(file)}` : null;
}

export function GameDetailPage() {
  const { id } = useParams<{ id: string }>();
  const { base, error: baseError } = useServerBase();
  const [game, setGame] = useState<LibraryGame | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!base || !id) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${base}/library`);
        const data = (await res.json()) as LibraryGame[];
        const found = data.find((g) => g.id === id) ?? null;
        if (!cancelled) {
          if (found) setGame(found);
          else setError("Game not found.");
        }
      } catch {
        if (!cancelled) setError("Couldn't load game details.");
      }
    })();
    return () => { cancelled = true; };
  }, [base, id]);

  const err = baseError ?? error;
  if (err) {
    return (
      <div style={{ padding: "2rem", opacity: 0.8 }}>{err}</div>
    );
  }
  if (!game) {
    return <div style={{ padding: "2rem", opacity: 0.7 }}>Loading…</div>;
  }

  const cover = base ? coverUrl(base, game) : null;
  const accent = game.accent_color ?? "#1a2330";

  return (
    <Focusable
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        background: "#0e1823",
        overflow: "hidden",
      }}
    >
      {/* Cover — fills the top portion of the screen */}
      <div
        style={{
          position: "relative",
          flex: "0 0 55%",
          background: accent,
          overflow: "hidden",
        }}
      >
        {cover && (
          <img
            src={cover}
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
        {/* Gradient into the details section */}
        <div
          style={{
            position: "absolute",
            bottom: 0,
            left: 0,
            right: 0,
            height: "120px",
            background: "linear-gradient(transparent, #0e1823)",
          }}
        />
      </div>

      {/* Details */}
      <div
        style={{
          flex: 1,
          padding: "0.75rem 1.75rem 1.5rem",
          display: "flex",
          flexDirection: "column",
          gap: "0.75rem",
          overflowY: "auto",
        }}
      >
        <h2 style={{ margin: 0, fontSize: "1.5rem", fontWeight: 700, lineHeight: 1.2 }}>
          {game.game_name}
        </h2>

        <div style={{ display: "flex", gap: "1.25rem", opacity: 0.7, fontSize: "0.9rem" }}>
          {game.playtime_minutes > 0 && (
            <span>{formatPlaytime(game.playtime_minutes)} played</span>
          )}
          {game.last_played_at && (
            <span>Last played {formatRelativeTime(game.last_played_at)}</span>
          )}
          {game.sync_badge && (
            <span style={{ color: "#f0a500" }}>{game.sync_badge}</span>
          )}
        </div>

        {/* Buttons */}
        <Focusable style={{ display: "flex", gap: "0.75rem", marginTop: "auto" }}>
          <Focusable
            onActivate={() => {
              Navigation.NavigateBack();
              if (base) void launchLibraryGame(base, game.id, game.shortcut_app_id ?? null);
            }}
            style={{
              flex: 1,
              padding: "0.7rem 0",
              borderRadius: "6px",
              background: accent,
              fontWeight: 700,
              fontSize: "1rem",
              textAlign: "center",
              cursor: "pointer",
              border: "none",
              color: "#fff",
            }}
          >
            Play
          </Focusable>
          <Focusable
            onActivate={() => Navigation.NavigateBack()}
            style={{
              padding: "0.7rem 1.5rem",
              borderRadius: "6px",
              background: "#2a3a52",
              fontWeight: 600,
              fontSize: "1rem",
              textAlign: "center",
              cursor: "pointer",
              border: "none",
              color: "#fff",
            }}
          >
            Back
          </Focusable>
        </Focusable>
      </div>
    </Focusable>
  );
}
