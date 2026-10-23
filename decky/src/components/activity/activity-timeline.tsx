import { useMemo, useState } from "react";
import { DialogButton, Focusable } from "@decky/ui";
import { FaDesktop, FaPlay, FaTrophy } from "react-icons/fa6";
import { buildTimeline } from "../../../../tauri/src/lib/activity";
import type { AchievementView, GameAchievements, PlaySession } from "../../types";
import { absDate, formatPlaytime } from "../../lib/format";
import { achievementIconUrl } from "../../lib/server";
import { AchievementIcon, DetailCard, MonoLabel, T } from "./detail-card";

// Play sessions and achievement unlocks grouped by day, newest first. Ported
// from the desktop `ActivityTimelineCard.svelte`; the grouping is the shared
// `buildTimeline`.
//
//   ┌─[accent] ACTIVITY · TIMELINE ──────────────────────────┐
//   │ TODAY                                                   │
//   │ ● Played 1h 20m                                         │
//   │ ● Unlocked 2 achievements                               │
//   │     [icon] Winner                                       │
//   │     [icon] Speedrunner                                  │
//   │ 3 JUN 2026                                              │
//   │ ● Played 45m                            [▣ Steam Deck]  │
//   │ Show more                                               │
//   └─────────────────────────────────────────────────────────┘

// Each day is its own focus stop: the tab scrolls to follow controller focus,
// so a card that was one stop would leave everything below its first screenful
// unreachable.

// Days shown at first, and added per "Show more".
const FIRST = 5;
const MORE = 10;
// Unlocks listed by name in one entry; the rest show as an icon strip.
const NAMED = 3;
const DAY_MS = 24 * 60 * 60 * 1000;

function dayLabel(day: number): string {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const diff = Math.round((today - day) / DAY_MS);
  if (diff === 0) return "TODAY";
  if (diff === 1) return "YESTERDAY";
  return absDate(new Date(day).toISOString()).toUpperCase();
}

function unlockLabel(items: AchievementView[]): string {
  return `Unlocked ${items.length} achievement${items.length === 1 ? "" : "s"}`;
}

function DeviceChip({ name, accent }: { name: string; accent: string }) {
  return (
    <span
      style={{
        flexShrink: 0,
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        whiteSpace: "nowrap",
        borderRadius: 2,
        border: `1px solid ${T.line2}`,
        background: T.bg2,
        padding: "2px 8px",
        fontSize: 10.5,
        color: T.ink1,
      }}
    >
      <FaDesktop size={10} color={accent} />
      {name}
    </span>
  );
}

const row = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 10,
} as const;

const label = {
  display: "flex",
  minWidth: 0,
  alignItems: "center",
  gap: 8,
  fontSize: 12.5,
} as const;

export function ActivityTimeline({
  base,
  sessions,
  data,
  deviceId,
  accent = T.spool,
}: {
  base: string;
  sessions: PlaySession[];
  data: GameAchievements | null;
  deviceId: string | null;
  accent?: string;
}) {
  const [limit, setLimit] = useState(FIRST);
  const days = useMemo(
    () => buildTimeline(sessions, data?.items ?? [], deviceId),
    [sessions, data, deviceId],
  );
  if (days.length === 0) return null;

  return (
    <DetailCard title="ACTIVITY · TIMELINE" accent={accent} flow>
      <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
        {days.slice(0, limit).map((d) => (
          <Focusable key={d.day} onActivate={() => {}}>
            <MonoLabel size={9} color={T.ink3}>
              {dayLabel(d.day)}
            </MonoLabel>
            <div
              style={{
                marginTop: 8,
                display: "flex",
                flexDirection: "column",
                gap: 12,
                borderLeft: `1px solid ${T.line2}`,
                paddingLeft: 16,
              }}
            >
              {d.entries.map((e) => (
                <div
                  key={e.kind === "session" ? e.session.session_id : `u:${e.deviceName ?? ""}`}
                  style={{ position: "relative" }}
                >
                  <span
                    style={{
                      position: "absolute",
                      left: -21,
                      top: 5,
                      width: 9,
                      height: 9,
                      boxSizing: "border-box",
                      borderRadius: 999,
                      border: `2px solid ${T.bg1}`,
                      background: e.kind === "unlocks" ? accent : T.ink3,
                    }}
                  />
                  {e.kind === "session" ? (
                    <div style={row}>
                      <div style={{ ...label, color: T.ink1 }}>
                        <FaPlay size={10} color={T.ink3} style={{ flexShrink: 0 }} />
                        <span>Played {formatPlaytime(e.minutes)}</span>
                      </div>
                      {!e.isThisDevice && e.session.device_name && (
                        <DeviceChip name={e.session.device_name} accent={accent} />
                      )}
                    </div>
                  ) : (
                    <>
                      <div style={row}>
                        <div style={{ ...label, color: T.ink0 }}>
                          <FaTrophy size={11} color={accent} style={{ flexShrink: 0 }} />
                          <span>{unlockLabel(e.items)}</span>
                        </div>
                        {e.deviceName && <DeviceChip name={e.deviceName} accent={accent} />}
                      </div>
                      {e.items.length <= NAMED ? (
                        <div style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 6 }}>
                          {e.items.map((a) => (
                            <div key={a.api_name} style={{ display: "flex", minWidth: 0, alignItems: "center", gap: 10 }}>
                              <AchievementIcon src={achievementIconUrl(base, a)} size={28} />
                              <div style={{ minWidth: 0 }}>
                                <div style={{ fontSize: 12, color: T.ink0 }}>{a.display_name}</div>
                                {a.description && <div style={{ fontSize: 11, color: T.ink3 }}>{a.description}</div>}
                              </div>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <div style={{ marginTop: 8, display: "flex", flexWrap: "wrap", gap: 6 }}>
                          {e.items.map((a) => (
                            <AchievementIcon
                              key={a.api_name}
                              src={achievementIconUrl(base, a)}
                              size={32}
                              title={a.display_name}
                            />
                          ))}
                        </div>
                      )}
                    </>
                  )}
                </div>
              ))}
            </div>
          </Focusable>
        ))}
      </div>

      {days.length > limit && (
        <DialogButton
          onClick={() => setLimit((l) => l + MORE)}
          style={{ marginTop: 14, width: "auto", minWidth: 0, padding: "6px 14px", fontSize: 12 }}
        >
          Show more
        </DialogButton>
      )}
    </DetailCard>
  );
}
