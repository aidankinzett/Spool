import { useServerBase } from "../../hooks/use-server-base";
import { useSpoolPlaytime } from "../../hooks/use-spool-playtime";
import { formatPlaytime } from "../../lib/format";
import { SpoolMark } from "../spool-mark";

// Spool's section at the top of the game page's Activity tab. Renders nothing
// for games Spool doesn't manage.
//
// Placeholder while the post-game summary and timeline cards are ported.
export function SpoolActivity({ appid }: { appid: number }) {
  const { base } = useServerBase();
  const { game } = useSpoolPlaytime(appid, base);
  if (!game) return null;

  const accent = game.accent_color || "#d7c9a0";
  return (
    <div
      style={{
        margin: "16px 2.8vw",
        padding: "14px 16px",
        display: "flex",
        alignItems: "center",
        gap: 12,
        background: "rgba(21,24,29,0.92)",
        borderLeft: `2.5px solid ${accent}`,
        boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)",
        borderRadius: 4,
        color: "#f4f4f5",
      }}
    >
      <SpoolMark size={17} color="#f4f4f5" tape={accent} />
      <div style={{ fontSize: 14, fontWeight: 600 }}>Spool activity</div>
      <div style={{ fontSize: 13, color: "rgba(244,244,245,0.78)" }}>
        {formatPlaytime(game.playtime_minutes)} played
      </div>
    </div>
  );
}
