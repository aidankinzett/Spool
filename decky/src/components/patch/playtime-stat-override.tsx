import { type ReactNode } from "react";
import { useServerBase } from "../../hooks/use-server-base";
import { useSpoolPlaytime } from "../../hooks/use-spool-playtime";
import { useParams } from "../../lib/steam";
import { formatPlaytime } from "../../lib/format";

// Rendered in place of Steam's value inside the gamepad-UI "Playtime" GameStat.
// Shows Spool's cross-device total once the headless server responds; until
// then — or when there's no Spool match for this appid — it renders Steam's
// original value so the stat never goes blank.
export function PlaytimeStatValue({ fallback }: { fallback: ReactNode }) {
  const { base } = useServerBase();
  const { appid: appidStr } = useParams<{ appid: string }>();
  const appid = parseInt(appidStr ?? "0", 10);
  const { game, loading } = useSpoolPlaytime(appid, base);

  if (loading || !game || game.playtime_minutes <= 0) return <>{fallback}</>;
  return <>{formatPlaytime(game.playtime_minutes)}</>;
}
