import type { LibraryGame, LanPeer, PeerGame, DownloadProgress, LaunchInfo } from "../types";

// Base request helpers
async function serverGet<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`GET request failed: status ${res.status}`);
  }
  return res.json() as Promise<T>;
}

async function serverSend<T>(url: string, method: "POST" | "DELETE", body?: unknown): Promise<T> {
  const options: RequestInit = {
    method,
  };
  if (body !== undefined) {
    options.headers = { "Content-Type": "application/json" };
    options.body = JSON.stringify(body);
  }
  const res = await fetch(url, options);
  if (!res.ok) {
    throw new Error(`${method} request failed: status ${res.status}`);
  }
  return res.json() as Promise<T>;
}

// Named routes

// GET /library
export async function getLibrary(base: string): Promise<LibraryGame[]> {
  const data = await serverGet<unknown>(`${base}/library`);
  if (!Array.isArray(data)) {
    throw new Error("Invalid response format: expected an array");
  }
  return data as LibraryGame[];
}

// POST /fold
export async function triggerFold(base: string): Promise<void> {
  const res = await fetch(`${base}/fold`, { method: "POST" });
  if (!res.ok) {
    throw new Error(`POST /fold failed: status ${res.status}`);
  }
}

// GET /games/{id}/steam-launch-info
export async function getSteamLaunchInfo(base: string, gameId: string): Promise<LaunchInfo> {
  return serverGet<LaunchInfo>(`${base}/games/${gameId}/steam-launch-info`);
}

// GET /games/{id}/steam-art/{kind}
export async function getSteamArt(
  base: string,
  gameId: string,
  kind: string,
): Promise<{ imageType: string; base64: string } | null> {
  // Keep steam-art's expected-404 continue by returning null on non-ok status
  try {
    const res = await fetch(`${base}/games/${gameId}/steam-art/${kind}`);
    if (!res.ok) return null;
    return res.json() as Promise<{ imageType: string; base64: string }>;
  } catch {
    return null;
  }
}

// GET /lan/peers
export async function getLanPeers(base: string): Promise<LanPeer[]> {
  return serverGet<LanPeer[]>(`${base}/lan/peers`);
}

// GET /lan/peers/{addr}/{port}/games
export async function getPeerGames(base: string, peerAddr: string, peerPort: string | number): Promise<PeerGame[]> {
  return serverGet<PeerGame[]>(`${base}/lan/peers/${peerAddr}/${peerPort}/games`);
}

// GET /lan/download
export async function getLanDownload(base: string): Promise<DownloadProgress | null> {
  // Keep /lan/download's nullable body explicit by using serverGet but verifying/allowing null
  const res = await fetch(`${base}/lan/download`);
  if (!res.ok) {
    throw new Error(`GET /lan/download failed: status ${res.status}`);
  }
  return res.json() as Promise<DownloadProgress | null>;
}

// POST /lan/install
export async function startLanInstall(
  base: string,
  peerAddr: string,
  peerPort: number,
  gameId: string,
): Promise<{ install_token: string }> {
  return serverSend<{ install_token: string }>(`${base}/lan/install`, "POST", {
    peer_addr: peerAddr,
    peer_port: peerPort,
    game_id: gameId,
  });
}

// DELETE /lan/download
export async function cancelLanDownload(base: string, installToken: string): Promise<{ cancelled: boolean }> {
  return serverSend<{ cancelled: boolean }>(`${base}/lan/download`, "DELETE", {
    install_token: installToken,
  });
}
