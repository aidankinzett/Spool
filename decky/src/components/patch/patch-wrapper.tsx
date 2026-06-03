import { useServerBase } from "../../hooks/use-server-base";
import { useSpoolPlaytime } from "../../hooks/use-spool-playtime";
import { useParams } from "../../lib/steam";
import { SpoolPlaytimeBadge } from "./playtime-badge";
import { SpoolBackupBadge } from "./backup-badge";
import { SpoolMark } from "../spool-mark";

// Badge wrapper injected into the game detail page's InnerContainer via
// afterPatch. Uses useParams to read appid from Steam's internal router —
// window.location.pathname is always '/index.html' in Steam's CEF context.
//
// The library is fetched once here and the resolved entry is handed to each
// badge, so adding more badges doesn't multiply the HTTP traffic.
export function PatchWrapper() {
  const { base } = useServerBase();
  const { appid: appidStr } = useParams<{ appid: string }>();
  const appid = parseInt(appidStr ?? "0", 10);
  const { game, loading } = useSpoolPlaytime(appid, base);

  if (!appid || loading || !game) return null;

  return (
    <div style={{ display: "flex", flexDirection: "row", gap: "0.25rem", padding: "0.5rem 2.8vw" }}>
      <div>
        <SpoolMark size={16} />
      </div>
      <SpoolPlaytimeBadge game={game} />
      <SpoolBackupBadge game={game} />
    </div>
  );
}
