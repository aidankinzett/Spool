import { useServerBase } from "../hooks/use-server-base";
import { useParams } from "../lib/steam";
import { SpoolPlaytimeBadge } from "./playtime-badge";

// Badge wrapper injected into the game detail page's InnerContainer via
// afterPatch. Uses useParams to read appid from Steam's internal router —
// window.location.pathname is always '/index.html' in Steam's CEF context.
export function PlaytimePatchWrapper() {
  const { base } = useServerBase();
  const { appid: appidStr } = useParams<{ appid: string }>();
  const appid = parseInt(appidStr ?? "0", 10);

  if (!appid) return null;

  return (
    <div style={{ padding: "0.5rem 0" }}>
      <SpoolPlaytimeBadge appid={appid} base={base} />
    </div>
  );
}
