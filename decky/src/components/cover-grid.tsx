import { Focusable } from "@decky/ui";

// Shared cover-tile grid used by both the local library and a peer's games.
// Tiles are focusable for controller nav; `onActivate` is optional (inert in
// the LAN browse view for now — installing lands in a later phase).
export interface Tile {
  key: string;
  name: string;
  coverUrl: string | null;
  accentColor?: string | null;
}

export function CoverGrid({
  tiles,
  onActivate,
}: {
  tiles: Tile[];
  onActivate?: (key: string) => void;
}) {
  return (
    <Focusable
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))",
        gap: "1.25rem",
      }}
    >
      {tiles.map((t) => (
        <Focusable
          key={t.key}
          onActivate={() => onActivate?.(t.key)}
          style={{
            aspectRatio: "2 / 3",
            borderRadius: "8px",
            overflow: "hidden",
            position: "relative",
            display: "flex",
            alignItems: "flex-end",
            background: t.accentColor ?? "#1a2330",
          }}
        >
          {t.coverUrl ? (
            <img
              src={t.coverUrl}
              alt={t.name}
              style={{
                position: "absolute",
                inset: 0,
                width: "100%",
                height: "100%",
                objectFit: "cover",
              }}
            />
          ) : (
            <span
              style={{
                padding: "0.5rem",
                fontSize: "0.85rem",
                fontWeight: 600,
                textShadow: "0 1px 3px rgba(0,0,0,0.85)",
              }}
            >
              {t.name}
            </span>
          )}
        </Focusable>
      ))}
    </Focusable>
  );
}
