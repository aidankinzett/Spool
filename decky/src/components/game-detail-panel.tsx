import { Navigation, Focusable, showModal, ConfirmModal } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import type { LibraryGame } from "../types";
import { launchLibraryGame } from "../lib/launch";
import { formatPlaytime, formatRelativeTime } from "../lib/format";
import { useServerBase } from "../hooks/use-server-base";
import { useParams } from "../lib/steam";
import { deleteGame } from "../api/callables";

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
  const [heroDataUrl, setHeroDataUrl] = useState<string | null>(null);

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

  useEffect(() => {
    if (!base || !id) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${base}/games/${id}/steam-art/hero`);
        if (res.ok) {
          const { imageType, base64 } = (await res.json()) as {
            imageType: string;
            base64: string;
          };
          if (!cancelled) setHeroDataUrl(`data:image/${imageType};base64,${base64}`);
        }
      } catch {
        // hero image is optional
      }
    })();
    return () => { cancelled = true; };
  }, [base, id]);

  const err = baseError ?? error;
  if (err) return <div style={{ padding: "2rem", opacity: 0.8 }}>{err}</div>;
  if (!game) return <div style={{ padding: "2rem", opacity: 0.7 }}>Loading…</div>;

  const confirmDelete = () => {
    showModal(
      <ConfirmModal
        strTitle={`Delete ${game.game_name} from disk?`}
        strDescription={
          "This permanently removes the install folder" +
          (game.game_folder_path ? `\n${game.game_folder_path}` : "") +
          "\nand its library entry. This can't be undone."
        }
        strOKButtonText="Delete from disk"
        strCancelButtonText="Cancel"
        bDestructiveWarning
        onOK={() => {
          void (async () => {
            const res = await deleteGame(game.id);
            if (res.ok) {
              toaster.toast({
                title: "Spool",
                body: `Deleted ${game.game_name} from disk`,
              });
              Navigation.NavigateBack();
            } else {
              toaster.toast({
                title: "Spool",
                body: `Couldn't delete: ${res.reason ?? "unknown error"}`,
              });
            }
          })();
        }}
      />,
    );
  };

  const cover = base ? coverUrl(base, game) : null;
  const accent = game.accent_color ?? "#3d7ab5";
  const canDelete = !!game.game_folder_path;
  // Prefer hero (landscape) for background; fall back to portrait cover
  const bgSrc = heroDataUrl ?? cover;

  return (
    <Focusable
      style={{
        position: "relative",
        height: "100%",
        background: "#0e1823",
        overflow: "hidden",
      }}
    >
      {/* Full-bleed background image */}
      {bgSrc && (
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
      )}

      {/* Gradient — heavier at bottom so text and button are always legible */}
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
        {cover && (
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
              src={cover}
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

        {/* Action buttons — Play (accent) + Delete (destructive) */}
        <Focusable style={{ display: "flex", gap: "0.6rem", alignItems: "center" }}>
          <Focusable
            onActivate={() => {
              Navigation.NavigateBack();
              if (base) void launchLibraryGame(base, game.id, game.shortcut_app_id ?? null);
            }}
            style={{
              padding: "0.5rem 1.4rem",
              borderRadius: "5px",
              background: accent,
              fontWeight: 700,
              fontSize: "0.95rem",
              cursor: "pointer",
              border: "none",
              color: "#fff",
            }}
          >
            ▶  Play
          </Focusable>

          {canDelete && (
            <Focusable
              onActivate={confirmDelete}
              style={{
                padding: "0.5rem 1.2rem",
                borderRadius: "5px",
                background: "transparent",
                fontWeight: 600,
                fontSize: "0.9rem",
                cursor: "pointer",
                border: "1px solid rgba(255,107,107,0.6)",
                color: "#ff8a8a",
              }}
            >
              Delete from disk
            </Focusable>
          )}
        </Focusable>
      </div>
    </Focusable>
  );
}
