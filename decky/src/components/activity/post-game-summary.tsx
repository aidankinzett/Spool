import { useMemo, useState, type CSSProperties } from "react";
import { DialogButton } from "@decky/ui";
import { FaLock, FaXmark } from "react-icons/fa6";
import { sessionUnlocks } from "../../../../tauri/src/lib/activity";
import type { GameAchievements, PlaySession } from "../../types";
import { absDateTime, formatPlaytime, formatRelativeTime } from "../../lib/format";
import { achievementIconUrl } from "../../lib/server";
import { AchievementIcon, DetailCard, MonoLabel, T } from "./detail-card";

// Post-game summary: this device's most recent session and the achievements it
// unlocked. Ported from the desktop `PostGameSummaryCard.svelte`.
//
//   ┌─[accent] POST-GAME SUMMARY ─────────────────── 2h ago  ×┐
//   │ SESSION        ENDED                 ACHIEVEMENTS       │
//   │ 1h 20m         3 Jun 2026, 21:14     +3                 │
//   │ ███████████▓▓▓░░░░░░░░░░░░░░░░░░░░░░   12 → 15 / 40     │
//   │ [icon] Winner                         Rare · 4.2%       │
//   │        Win one game.                                    │
//   └─────────────────────────────────────────────────────────┘
//
// The card stays up until dismissed, or until the session is a week old.
// Dismissal is stored per game as the dismissed `session_id`, so the next
// session shows the card again. The desktop app keeps its own dismissal.

// Unlock rows shown before "+N more".
const SHOWN = 4;
// A session older than this gets no summary.
const MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;

function storageKey(gameId: string) {
  return `spool:post-game-dismissed:${gameId}`;
}

function readDismissed(gameId: string): string | null {
  try {
    return localStorage.getItem(storageKey(gameId));
  } catch {
    return null;
  }
}

function pct(n: number, total: number): number {
  return total > 0 ? (n / total) * 100 : 0;
}

function fmtRarity(p: number): string {
  const v = p < 10 ? p.toFixed(1) : String(Math.round(p));
  return p < 10 ? `Rare · ${v}%` : `${v}% of players`;
}

export function PostGameSummary({
  base,
  gameId,
  sessions,
  data,
  deviceId,
  accent = T.spool,
  style,
}: {
  base: string;
  gameId: string;
  /** This game's play sessions, every device. */
  sessions: PlaySession[];
  data: GameAchievements | null;
  /** This device's id; the card only summarises sessions played here. */
  deviceId: string | null;
  accent?: string;
  /** Applied to the card, which isn't rendered at all while hidden. */
  style?: CSSProperties;
}) {
  const [dismissed, setDismissed] = useState(() => readDismissed(gameId));

  const latest = useMemo(() => {
    if (!deviceId) return null;
    let best: PlaySession | null = null;
    for (const s of sessions) {
      if (s.device_id !== deviceId) continue;
      if (!best || Date.parse(s.ended_at) > Date.parse(best.ended_at)) best = s;
    }
    return best;
  }, [sessions, deviceId]);

  const earned = useMemo(
    () => (latest && data ? sessionUnlocks(latest, data.items) : []),
    [latest, data],
  );

  if (
    !latest ||
    dismissed === latest.session_id ||
    Date.now() - Date.parse(latest.ended_at) >= MAX_AGE_MS
  ) {
    return null;
  }

  const hasTotal = data?.schema_source != null && data.total > 0;
  const before = data ? data.unlocked - earned.length : 0;
  const stats = [
    { label: "SESSION", value: formatPlaytime(Math.round(latest.duration_secs / 60)) },
    { label: "ENDED", value: absDateTime(latest.ended_at) },
    { label: "ACHIEVEMENTS", value: earned.length > 0 ? `+${earned.length}` : "—" },
  ];

  const dismiss = () => {
    setDismissed(latest.session_id);
    try {
      localStorage.setItem(storageKey(gameId), latest.session_id);
    } catch {
      // Storage unavailable: the card hides until the page re-renders.
    }
  };

  return (
    <DetailCard
      title="POST-GAME SUMMARY"
      accent={accent}
      flow
      style={style}
      action={
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <MonoLabel size={9} color={T.ink3}>
            {formatRelativeTime(latest.ended_at)}
          </MonoLabel>
          <DialogButton
            onClick={dismiss}
            aria-label="Dismiss post-game summary"
            style={{
              minWidth: 0,
              width: 28,
              height: 28,
              padding: 0,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <FaXmark size={12} />
          </DialogButton>
        </div>
      }
    >
      <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 18 }}>
        {stats.map((s) => (
          <div key={s.label} style={{ minWidth: 0 }}>
            <MonoLabel size={9}>{s.label}</MonoLabel>
            <div
              style={{
                marginTop: 2,
                fontSize: 18,
                fontWeight: 600,
                color: T.ink0,
                fontVariantNumeric: "tabular-nums",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {s.value}
            </div>
          </div>
        ))}
      </div>

      {hasTotal && data && (
        <div style={{ marginTop: 12, display: "flex", alignItems: "center", gap: 10 }}>
          <div
            style={{
              flex: 1,
              display: "flex",
              height: 6,
              overflow: "hidden",
              borderRadius: 999,
              background: T.bg3,
            }}
          >
            <div style={{ height: "100%", width: `${pct(before, data.total)}%`, background: `${accent}5c` }} />
            <div style={{ height: "100%", width: `${pct(earned.length, data.total)}%`, background: accent }} />
          </div>
          <span style={{ fontFamily: T.mono, fontSize: 11, color: T.ink2, fontVariantNumeric: "tabular-nums" }}>
            {earned.length > 0 ? `${before} → ` : ""}
            {data.unlocked} / {data.total}
          </span>
        </div>
      )}

      {earned.length > 0 ? (
        <>
          <div
            style={{
              marginTop: 14,
              paddingTop: 12,
              borderTop: `1px dashed ${T.line1}`,
              display: "flex",
              flexDirection: "column",
              gap: 10,
            }}
          >
            {earned.slice(0, SHOWN).map((a) => (
              <div key={a.api_name} style={{ display: "flex", alignItems: "center", gap: 12 }}>
                <AchievementIcon src={achievementIconUrl(base, a)} size={36} glow={accent} />
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div style={{ fontSize: 13, fontWeight: 500, color: T.ink0 }}>{a.display_name}</div>
                  {a.description && <div style={{ fontSize: 11.5, color: T.ink2 }}>{a.description}</div>}
                </div>
                {a.global_percent !== null && (
                  <div
                    style={{
                      flexShrink: 0,
                      textAlign: "right",
                      fontSize: 11,
                      fontVariantNumeric: "tabular-nums",
                      color: a.global_percent < 10 ? accent : T.ink3,
                    }}
                  >
                    {fmtRarity(a.global_percent)}
                  </div>
                )}
              </div>
            ))}
          </div>
          {earned.length > SHOWN && (
            <div style={{ marginTop: 10, fontSize: 12, color: T.ink3 }}>+{earned.length - SHOWN} more</div>
          )}
        </>
      ) : (
        data &&
        data.items.length > 0 && (
          <div
            style={{
              marginTop: 14,
              paddingTop: 12,
              borderTop: `1px dashed ${T.line1}`,
              display: "flex",
              alignItems: "center",
              gap: 8,
              fontSize: 12,
              color: T.ink3,
            }}
          >
            <FaLock size={11} />
            No achievements unlocked this session.
          </div>
        )
      )}
    </DetailCard>
  );
}
