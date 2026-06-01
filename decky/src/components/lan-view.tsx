import { Focusable } from "@decky/ui";
import { useEffect, useState } from "react";
import type { LanPeer } from "../types";
import { PeerGames } from "./peer-games";

// ── LAN browse ─────────────────────────────────────────────────────────────
//
// Peers and their games are fetched server-side: the UI can't hit a peer's
// non-loopback http directly (mixed content), so the headless server discovers
// peers and proxies their /games + covers. We poll /lan/peers (there's no push
// bus over plain HTTP). Selecting a peer shows its shared games as a grid;
// tiles are inert until the install phase.
export function LanView({ base }: { base: string }) {
  const [peers, setPeers] = useState<LanPeer[] | null>(null);
  const [selected, setSelected] = useState<LanPeer | null>(null);

  useEffect(() => {
    let cancelled = false;
    const poll = async () => {
      try {
        const res = await fetch(`${base}/lan/peers`);
        const data = (await res.json()) as LanPeer[];
        if (!cancelled) setPeers(data);
      } catch {
        if (!cancelled) setPeers([]);
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 3000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [base]);

  if (selected)
    return <PeerGames base={base} peer={selected} onBack={() => setSelected(null)} />;

  if (!peers) return <div style={{ opacity: 0.7 }}>Scanning…</div>;
  if (peers.length === 0)
    return (
      <div style={{ opacity: 0.7 }}>No Spool devices found on your network.</div>
    );

  return (
    <Focusable style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
      {peers.map((p) => {
        const browsable = p.file_server_port !== 0;
        return (
          <Focusable
            key={p.device_id}
            onActivate={() => browsable && setSelected(p)}
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
  );
}
