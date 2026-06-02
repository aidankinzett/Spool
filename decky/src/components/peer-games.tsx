import { toaster } from "@decky/api";
import { Navigation, ButtonItem, Focusable } from "@decky/ui";
import { useEffect, useRef, useState } from "react";
import type { DownloadProgress, LanPeer, PeerGame } from "../types";
import { fmtBytes } from "../lib/format";
import { CoverGrid } from "./cover-grid";
import { useServerBase } from "../hooks/use-server-base";
import { useParams } from "../lib/steam";

// A single peer's shared game grid. Registered as its own route so the B
// button naturally navigates back to the peers list (/spool/lan).
export function PeerGamesPage() {
  const { peerAddr, peerPort } =
    useParams<{ peerAddr: string; peerPort: string }>();
  const { base, error: baseError } = useServerBase();

  const [peer, setPeer] = useState<LanPeer | null>(null);
  const [games, setGames] = useState<PeerGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const peerBase = base ? `${base}/lan/peers/${peerAddr}/${peerPort}` : null;

  // Fetch peer info (device name) from the local peers list.
  useEffect(() => {
    if (!base) return;
    let cancelled = false;
    void fetch(`${base}/lan/peers`)
      .then((r) => r.json() as Promise<LanPeer[]>)
      .then((peers) => {
        const found =
          peers.find(
            (p) =>
              p.addr === peerAddr && String(p.file_server_port) === peerPort,
          ) ?? null;
        if (!cancelled && found) setPeer(found);
      })
      .catch(() => undefined);
    return () => { cancelled = true; };
  }, [base, peerAddr, peerPort]);

  // Fetch the game list.
  useEffect(() => {
    if (!peerBase) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${peerBase}/games`);
        if (!res.ok) throw new Error();
        const data = (await res.json()) as PeerGame[];
        if (!cancelled) setGames(data);
      } catch {
        if (!cancelled) setError("Couldn't reach this device.");
      }
    })();
    return () => { cancelled = true; };
  }, [peerBase]);

  // Pick up any in-flight download on mount (e.g. returned via B button).
  useEffect(() => {
    if (!base) return;
    void fetch(`${base}/lan/download`)
      .then((r) => r.json() as Promise<DownloadProgress | null>)
      .then((p) => {
        if (p && p.status !== "done" && p.status !== "error" && p.status !== "canceled") {
          setDownload(p);
          startPolling();
        }
      })
      .catch(() => undefined);
    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [base]);

  function startPolling() {
    if (!base || pollRef.current) return;
    pollRef.current = setInterval(() => {
      void fetch(`${base}/lan/download`)
        .then((r) => r.json() as Promise<DownloadProgress | null>)
        .then((p) => {
          setDownload(p);
          if (!p || p.status === "done" || p.status === "error" || p.status === "canceled") {
            if (pollRef.current) clearInterval(pollRef.current);
            pollRef.current = null;
            if (p?.status === "done") {
              toaster.toast({ title: "Install complete", body: p.game_name });
            } else if (p?.status === "error") {
              toaster.toast({ title: "Install failed", body: p.message ?? p.game_name });
            }
            setTimeout(() => setDownload(null), 3000);
          }
        })
        .catch(() => undefined);
    }, 500);
  }

  async function cancelDownload() {
    if (!base || !download) return;
    await fetch(`${base}/lan/download`, {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ install_token: download.install_token }),
    }).catch(() => undefined);
  }

  const isActive =
    download !== null &&
    (download.status === "starting" || download.status === "transferring");
  const pct =
    download && download.bytes_total > 0
      ? Math.round((download.bytes_done / download.bytes_total) * 100)
      : 0;
  const deviceName = peer?.device_name || peerAddr;

  const err = baseError ?? error;

  return (
    <div style={{ padding: "2rem", display: "flex", flexDirection: "column", gap: "1rem" }}>
      <h2 style={{ margin: 0 }}>{deviceName}</h2>

      {/* Download progress row */}
      {download !== null && (
        <div
          style={{
            background: "#1a2330",
            borderRadius: "8px",
            padding: "0.75rem 1rem",
            display: "flex",
            flexDirection: "column",
            gap: "0.5rem",
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
            }}
          >
            <span style={{ fontWeight: 600 }}>
              {download.game_name || "Fetching manifest…"}
            </span>
            {isActive && (
              <Focusable>
                <ButtonItem layout="below" onClick={() => void cancelDownload()}>
                  Cancel
                </ButtonItem>
              </Focusable>
            )}
            {download.status === "done" && (
              <span style={{ color: "#4caf50" }}>Installed</span>
            )}
            {download.status === "canceled" && (
              <span style={{ opacity: 0.7 }}>Cancelled</span>
            )}
            {download.status === "error" && (
              <span style={{ color: "#f44336" }}>Failed</span>
            )}
          </div>

          {download.status === "error" && download.message && (
            <div style={{ fontSize: "0.8rem", opacity: 0.8 }}>{download.message}</div>
          )}

          {(download.status === "starting" || download.status === "transferring") && (
            <>
              <div
                style={{
                  height: "4px",
                  borderRadius: "2px",
                  background: "#2a3a52",
                  overflow: "hidden",
                }}
              >
                <div
                  style={{
                    height: "100%",
                    width: `${pct}%`,
                    background: "#4a90d9",
                    transition: "width 0.3s",
                  }}
                />
              </div>
              <div style={{ fontSize: "0.8rem", opacity: 0.7 }}>
                {download.bytes_total > 0
                  ? `${fmtBytes(download.bytes_done)} / ${fmtBytes(download.bytes_total)}  (${fmtBytes(Math.round(download.bytes_per_second))}/s)`
                  : download.current_file || "Starting…"}
              </div>
            </>
          )}
        </div>
      )}

      {err && <div style={{ opacity: 0.8 }}>{err}</div>}
      {!err && !games && <div style={{ opacity: 0.7 }}>Loading…</div>}
      {games && games.length === 0 && (
        <div style={{ opacity: 0.7 }}>This device isn't sharing any games.</div>
      )}
      {games && games.length > 0 && (
        <CoverGrid
          onActivate={(id) =>
            Navigation.Navigate(
              `/spool/lan-game/${peerAddr}/${peerPort}/${id}`,
            )
          }
          tiles={games.map((g) => ({
            key: g.id,
            name: g.game_name,
            coverUrl: `${peerBase}/games/${g.id}/cover`,
          }))}
        />
      )}
    </div>
  );
}
