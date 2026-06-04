import type { LibraryGame } from "../../types";
import { formatPlaytime, formatRelativeTime } from "../../lib/format";
import { SpoolMark } from "../spool-mark";
import { Reel, TapeMeter } from "./reel";
import { BadgeMenuButton } from "./badge-menu";

// Compact "hybrid" bar injected on the game detail page (replaces the old row
// of separate gray BadgeShell pills). One single-height bar: Spool identity ·
// save state (reel + word) · last-played / play-time · backup detail · ⋮ menu.
// Per-game accent comes from game.accent_color (the cover-art accent the
// desktop app extracts); falls back to Spool's tape oxide.

const C = {
  // The bar overlays the page's hero image rather than sitting on its own
  // panel, so text/glyphs carry a drop shadow for legibility over bright art.
  shadow: "0 1px 4px rgba(0,0,0,0.85), 0 0 1px rgba(0,0,0,0.6)",
  line: "rgba(255,255,255,0.22)", // divider — needs to read over the hero image
  ink0: "#f4f4f5",
  ink1: "rgba(244,244,245,0.78)",
  ink3: "rgba(244,244,245,0.40)",
  ok: "#7ee2a4",
  warn: "#f4b66c",
  info: "#7ec6ff",
  spool: "#d7c9a0",
  mono: `"JetBrains Mono", ui-monospace, monospace`,
  ui: `"Geist", system-ui, sans-serif`,
};

type SaveState = {
  word: string;
  tone: string; // color
  /** detail/revision line under the state, or null to hide */
  detail: string | null;
  backing: boolean;
};

// Map the live LibraryGame + backingUp flag to the bar's save state.
// Mirrors backup-badge.tsx's sync_badge handling, plus a fresh (no-backup) and
// a backing-up state.
function resolveState(game: LibraryGame, backingUp: boolean, accent: string): SaveState {
  if (backingUp) {
    return { word: "Backing up…", tone: accent, detail: null, backing: true };
  }
  if (!game.save_last_backed_up_at) {
    return { word: "No backup yet", tone: C.ink3, detail: null, backing: false };
  }
  const when = formatRelativeTime(game.save_last_backed_up_at);
  switch (game.sync_badge) {
    case "synced":
      return { word: "Synced", tone: C.ok, detail: when, backing: false };
    case "local-newer":
      return { word: "Not uploaded", tone: C.warn, detail: when, backing: false };
    case "cloud-newer": {
      // Prefer the backing device + its revision time ("Desktop-PC · 2h ago"),
      // folded from the rclone device blobs; fall back to this device's local
      // backup time when those fields aren't populated.
      const dev = game.save_last_backer_device;
      const at = game.save_cloud_revision_at;
      return {
        word: "Cloud newer",
        tone: C.info,
        detail: dev && at ? `${dev} · ${formatRelativeTime(at)}` : when,
        backing: false,
      };
    }
    default:
      // Cloud not configured / no sync info — fall back to the backup time.
      return { word: "Backed up", tone: C.ink1, detail: when, backing: false };
  }
}

function Divider() {
  return <span style={{ width: 1, height: 22, background: C.line, flexShrink: 0 }} />;
}

export function SpoolBar({
  game,
  backingUp,
  appid,
}: {
  game: LibraryGame;
  backingUp: boolean;
  appid: number;
}) {
  const accent = game.accent_color || C.spool;
  const st = resolveState(game, backingUp, accent);

  const lastPlayed = game.last_played_at ? formatRelativeTime(game.last_played_at) : null;
  const playtime = game.playtime_minutes > 0 ? formatPlaytime(game.playtime_minutes) : null;
  const times = [lastPlayed, playtime].filter(Boolean).join("  ·  ") || null;

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 14,
        height: 44,
        padding: "0 8px 0 4px",
        // Overlay directly on the hero image — no panel surface. While backing
        // up, a faint accent wash fades in from the left to flag activity.
        background: st.backing
          ? `linear-gradient(90deg, ${accent}26, transparent 45%)`
          : "transparent",
        borderRadius: 4,
        fontFamily: C.ui,
        textShadow: C.shadow,
      }}
    >
      {/* identity */}
      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 8,
          filter: "drop-shadow(0 1px 2px rgba(0,0,0,0.8))",
        }}
      >
        <SpoolMark size={17} color={C.ink0} tape={accent} />
        <span style={{ fontSize: 14.5, fontWeight: 600, color: C.ink0, letterSpacing: "-0.01em" }}>
          Spool
        </span>
      </span>

      <Divider />

      {/* save state — leads */}
      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 7,
          whiteSpace: "nowrap",
          filter: "drop-shadow(0 1px 2px rgba(0,0,0,0.8))",
        }}
      >
        <Reel size={14} color={st.tone} spinning={st.backing} />
        <span style={{ fontSize: 14, fontWeight: 600, color: st.tone }}>{st.word}</span>
        {st.backing && (
          <span style={{ marginLeft: 2 }}>
            <TapeMeter accent={accent} />
          </span>
        )}
      </span>

      {times && <Divider />}
      {times && (
        <span style={{ fontFamily: C.mono, fontSize: 11.5, color: C.ink1, whiteSpace: "nowrap" }}>
          {times}
        </span>
      )}

      <span style={{ flex: 1 }} />

      {st.detail && (
        <span style={{ fontFamily: C.mono, fontSize: 10.5, color: C.ink3, whiteSpace: "nowrap" }}>
          {st.detail}
        </span>
      )}

      <BadgeMenuButton game={game} appid={appid} />
    </div>
  );
}
