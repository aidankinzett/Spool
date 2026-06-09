import { Navigation, Focusable } from "@decky/ui";
import { useEffect, useState } from "react";
import type { LanPeer } from "../types";
import { useServerBase } from "../hooks/use-server-base";
import { getLanPeers } from "../lib/server";

// Full-screen LAN peers list. Registered as its own route so the B button
// naturally navigates back to the library (/spool).
export function LanPage() {
  const { base, error: baseError } = useServerBase();
  const [peers, setPeers] = useState<LanPeer[] | null>(null);
  // Tracks whether the *latest* /lan/peers fetch failed, kept separate from the
  // peer list so a connection drop shows a connection error rather than the
  // identical "No Spool devices found" an empty network shows.
  const [fetchError, setFetchError] = useState(false);

  useEffect(() => {
    if (!base) return;
    let cancelled = false;
    const poll = async () => {
      try {
        const data = await getLanPeers(base);
        if (!cancelled) {
          setPeers(data);
          setFetchError(false);
        }
      } catch {
        if (!cancelled) setFetchError(true);
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 3000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [base]);

  const unreachable = (
    <div style={{ padding: "2rem", opacity: 0.8 }}>
      Couldn't reach Spool on this device.
    </div>
  );

  if (baseError) return <div style={{ padding: "2rem", opacity: 0.8 }}>{baseError}</div>;
  // No successful response yet: distinguish "still scanning" from "can't reach".
  if (!peers) return fetchError ? unreachable : <div style={{ padding: "2rem", opacity: 0.7 }}>Scanning…</div>;
  // An empty list only means "no devices" when the latest fetch actually
  // succeeded; if it's failing now, the empty list is stale.
  if (peers.length === 0)
    return fetchError ? unreachable : (
      <div style={{ padding: "2rem", opacity: 0.7 }}>
        No Spool devices found on your network.
      </div>
    );

  return (
    <div style={{ padding: "2rem" }}>
      <Focusable style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
        {peers.map((p) => {
          const browsable = p.file_server_port !== 0;
          return (
            <Focusable
              key={p.device_id}
              onActivate={() =>
                browsable &&
                Navigation.Navigate(`/spool/lan/${p.addr}/${p.file_server_port}`)
              }
              style={{
                padding: "1rem 1.25rem",
                borderRadius: "8px",
                background: "#1a2330",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                opacity: browsable ? 1 : 0.5,
              }}
            >
              <span style={{ fontWeight: 600 }}>{p.device_name || p.addr}</span>
              <span style={{ fontSize: "0.85rem", opacity: 0.8 }}>
                {browsable
                  ? `${p.game_count} game${p.game_count === 1 ? "" : "s"}`
                  : "sharing off"}
              </span>
            </Focusable>
          );
        })}
      </Focusable>
    </div>
  );
}
