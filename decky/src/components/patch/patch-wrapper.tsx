import { useEffect, useRef } from "react";
import { useServerBase } from "../../hooks/use-server-base";
import { useSpoolPlaytime } from "../../hooks/use-spool-playtime";
import { useBackingUp } from "../../hooks/use-backing-up";
import { useParams } from "../../lib/steam";
import { SpoolBar } from "./spool-bar";

// Badge wrapper injected into the game detail page's InnerContainer via
// afterPatch. Uses useParams to read appid from Steam's internal router —
// window.location.pathname is always '/index.html' in Steam's CEF context.
//
// The library is fetched once here and the resolved entry is handed to the
// SpoolBar, which renders the whole compact row (identity · save state ·
// times · detail · actions menu) — see spool-bar.tsx.
export function PatchWrapper() {
  const { base } = useServerBase();
  const { appid: appidStr } = useParams<{ appid: string }>();
  const appid = parseInt(appidStr ?? "0", 10);
  const { game, loading, refresh } = useSpoolPlaytime(appid, base);
  const backingUp = useBackingUp(appid);

  // When a backup finishes (backingUp falls back to false), re-fetch so the
  // bar swaps the spinning reel for the fresh "Synced · Nm ago" line.
  const wasBackingUp = useRef(backingUp);
  useEffect(() => {
    if (wasBackingUp.current && !backingUp) void refresh();
    wasBackingUp.current = backingUp;
  }, [backingUp, refresh]);

  // Keep showing the bar while a backup runs even before the first fetch
  // resolves, so the spinner isn't gated behind `loading`/`game`.
  if (!appid || (!backingUp && (loading || !game))) return null;
  if (!game) return null;

  return (
    <div style={{ padding: "0.5rem 2.8vw" }}>
      <SpoolBar game={game} backingUp={backingUp} appid={appid} />
    </div>
  );
}
