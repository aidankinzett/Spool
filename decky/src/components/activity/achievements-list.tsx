import { useMemo } from "react";
import { Focusable } from "@decky/ui";
import { FaLock } from "react-icons/fa6";
import type { GameAchievements } from "../../types";
import { achievementIconUrl } from "../../lib/server";
import { AchievementIcon, DetailCard, MonoLabel, T } from "./detail-card";

// The game's full achievement list. Ported from the desktop
// `AchievementsCard.svelte`, without its collapse: every achievement is listed.
//
//   ┌─[accent] ACHIEVEMENTS ─────────────────────── 12 / 40┐
//   │ ████████░░░░░░░░░░░░  30%                             │
//   │ [icon] Winner                         3 Jun 2026      │
//   │        Win one game.                                  │
//   │ [icon] Hidden achievement             4.2% of players │
//   └───────────────────────────────────────────────────────┘
//
// Unlocked entries come first (newest first), then locked ones (most common
// first). Hidden locked achievements show no name or description. Each row is
// a focus stop so the controller can scroll through the list. Rendered only
// when the game has a schema or any unlocks.

function fmtDate(secs: number): string {
  return new Date(secs * 1000).toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

function fmtPercent(p: number): string {
  return `${p < 10 ? p.toFixed(1) : Math.round(p)}% of players`;
}

export function AchievementsList({
  base,
  data,
  accent = T.spool,
}: {
  base: string;
  data: GameAchievements | null;
  accent?: string;
}) {
  const sorted = useMemo(() => {
    if (!data) return [];
    const unlocked = data.items
      .filter((a) => a.unlocked_at !== null)
      .sort((a, b) => (b.unlocked_at ?? 0) - (a.unlocked_at ?? 0));
    const locked = data.items
      .filter((a) => a.unlocked_at === null)
      .sort((a, b) => (b.global_percent ?? -1) - (a.global_percent ?? -1));
    return [...unlocked, ...locked];
  }, [data]);

  if (!data || data.items.length === 0) return null;

  // Without a schema only unlocked entries are known, so there's no total.
  const hasTotal = data.schema_source != null;
  const pct = data.total > 0 ? Math.round((data.unlocked / data.total) * 100) : 0;

  return (
    <DetailCard
      title="ACHIEVEMENTS"
      accent={accent}
      flow
      action={
        <MonoLabel size={9} color={T.ink3}>
          {hasTotal ? `${data.unlocked} / ${data.total}` : `${data.unlocked} UNLOCKED`}
        </MonoLabel>
      }
    >
      {hasTotal && (
        <div style={{ marginBottom: 12, display: "flex", alignItems: "center", gap: 10 }}>
          <div style={{ flex: 1, height: 6, overflow: "hidden", borderRadius: 999, background: T.bg3 }}>
            <div style={{ height: "100%", width: `${pct}%`, borderRadius: 999, background: accent }} />
          </div>
          <span style={{ fontFamily: T.mono, fontSize: 11, color: T.ink2, fontVariantNumeric: "tabular-nums" }}>
            {pct}%
          </span>
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {sorted.map((a) => {
          const locked = a.unlocked_at === null;
          const secret = locked && a.hidden;
          return (
            <Focusable
              key={a.api_name}
              onActivate={() => {}}
              style={{ display: "flex", alignItems: "center", gap: 12, opacity: locked ? 0.6 : 1 }}
            >
              {secret ? (
                <div
                  style={{
                    width: 36,
                    height: 36,
                    flexShrink: 0,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    borderRadius: 2,
                    border: `1px solid ${T.line2}`,
                    background: T.bg2,
                    color: T.ink3,
                  }}
                >
                  <FaLock size={13} />
                </div>
              ) : (
                <div style={{ filter: locked ? "grayscale(1)" : undefined, flexShrink: 0 }}>
                  <AchievementIcon src={achievementIconUrl(base, a)} size={36} />
                </div>
              )}
              <div style={{ minWidth: 0, flex: 1 }}>
                <div style={{ fontSize: 13, fontWeight: 500, color: T.ink0 }}>
                  {secret ? "Hidden achievement" : a.display_name}
                </div>
                {!secret && a.description && (
                  <div style={{ fontSize: 11.5, color: T.ink2 }}>{a.description}</div>
                )}
              </div>
              <div
                style={{
                  flexShrink: 0,
                  textAlign: "right",
                  fontSize: 11,
                  color: T.ink3,
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {a.unlocked_at !== null ? (
                  <>
                    {fmtDate(a.unlocked_at)}
                    {a.unlocked_on && <div>on {a.unlocked_on}</div>}
                  </>
                ) : (
                  a.global_percent !== null && fmtPercent(a.global_percent)
                )}
              </div>
            </Focusable>
          );
        })}
      </div>

      {!hasTotal && (
        <p style={{ margin: "12px 0 0", fontSize: 11.5, color: T.ink3 }}>
          {data.epic_app != null && data.appid == null
            ? "Spool couldn't get this Epic game's achievement list from egdata.app, so only unlocked achievements are shown, by ID."
            : "Add a Steam Web API key in Spool's Settings → Achievements to see names, icons, and the full list."}
        </p>
      )}
    </DetailCard>
  );
}
