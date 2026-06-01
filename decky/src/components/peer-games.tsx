import { toaster } from "@decky/api";
import { Navigation, ButtonItem, Focusable } from "@decky/ui";
import { useEffect, useRef, useState } from "react";
import type { DownloadProgress, LanPeer, PeerGame } from "../types";
import { fmtBytes } from "../lib/format";
import { CoverGrid } from "./cover-grid";

// A selected peer's shared games, fetched through the server-side proxy.
// Activating a tile navigates to the LAN game detail page (router-based so
// the hardware B button navigates back here).
export function PeerGames({
  base,
  peer,
  onBack,
}: {
  base: string;
  peer: LanPeer;
  onBack: () => void;
}) {
  const [games, setGames] = useState<PeerGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const peerBase = `${base}/lan/peers/${peer.addr}/${peer.file_server_port}`;

  // Fetch the game list once on mount.
  useEffect(() => {
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
    return () => {
      cancelled = true;
    };
  }, [peerBase]);

  // On mount, pick up any in-flight download (e.g. navigated away and back).
  useEffect(() => {
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
  }, []);

  function startPolling() {
    if (pollRef.current) return;
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
    if (!download) return;
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

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
      <Focusable>
        <ButtonItem layout="below" onClick={onBack}>
          ← Back to devices
        </ButtonItem>
      </Focusable>
      <h2 style={{ margin: 0 }}>{peer.device_name || peer.addr}</h2>

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

      {error && <div style={{ opacity: 0.8 }}>{error}</div>}
      {!error && !games && <div style={{ opacity: 0.7 }}>Loading…</div>}
      {games && games.length === 0 && (
        <div style={{ opacity: 0.7 }}>This device isn't sharing any games.</div>
      )}
      {games && games.length > 0 && (
        <CoverGrid
          onActivate={(id) =>
            Navigation.Navigate(
              `/spool/lan-game/${peer.addr}/${peer.file_server_port}/${id}`,
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
