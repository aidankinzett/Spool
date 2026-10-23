import { useServerBase } from "../../hooks/use-server-base";
import { useSpoolPlaytime } from "../../hooks/use-spool-playtime";
import { useGameActivity } from "../../hooks/use-game-activity";
import { PostGameSummary } from "./post-game-summary";
import { ActivityTimeline } from "./activity-timeline";
import { AchievementsList } from "./achievements-list";
import { T } from "./detail-card";

// Spool's additions to the game page, both for games Spool manages only (they
// render nothing otherwise). They share one activity load (useGameActivity).

// Between the Play row and the tab bar: the post-game summary. Side margins
// match the Play row's padding.
export function SpoolSummary({ appid }: { appid: number }) {
  const { base } = useServerBase();
  const { game, folded } = useSpoolPlaytime(appid, base);
  const { achievements, sessions, deviceId } = useGameActivity(base, game, folded);
  if (!base || !game) return null;

  return (
    <PostGameSummary
      key={game.id}
      base={base}
      gameId={game.id}
      sessions={sessions}
      data={achievements}
      deviceId={deviceId}
      accent={game.accent_color || T.spool}
      style={{ margin: "0 2.8vw 16px" }}
    />
  );
}

// At the top of the Activity tab: the activity timeline, then the full
// achievement list.
export function SpoolActivity({ appid }: { appid: number }) {
  const { base } = useServerBase();
  const { game, folded } = useSpoolPlaytime(appid, base);
  const { achievements, sessions, deviceId } = useGameActivity(base, game, folded);
  if (!base || !game) return null;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14, margin: "16px 0" }}>
      <ActivityTimeline
        key={game.id}
        base={base}
        sessions={sessions}
        data={achievements}
        deviceId={deviceId}
        accent={game.accent_color || T.spool}
      />
      <AchievementsList base={base} data={achievements} accent={game.accent_color || T.spool} />
    </div>
  );
}
