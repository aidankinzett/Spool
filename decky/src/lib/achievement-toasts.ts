// In-game achievement toasts for Spool-launched games.
//
// The `spool --run` workflow records each achievement it sees unlock (read
// from the game's Steam-emulator stats file) in the library DB. Desktop
// notifications don't show over gamescope, so while a Spool shortcut is
// running this polls the headless server's GET /achievements/recent and shows
// each new unlock as a Steam toast.
//
// Polling runs only while a non-Steam shortcut is running (Spool's shortcut
// appids have the high bit set), and for LINGER_MS after the last one stops,
// since the workflow does a final read of the stats file after the game exits.
import { toaster } from "@decky/api";
import { createElement } from "react";
import { getServerBase } from "../api/callables";

const POLL_MS = 3000;
const LINGER_MS = 20_000;

type Unlock = {
  game_name: string;
  api_name: string;
  display_name: string;
  icon: string | null;
  detected_at: number;
};

const running = new Set<number>();
let timer: ReturnType<typeof setInterval> | null = null;
let stopAt = 0;
let base: string | null = null;
// Only unlocks recorded after this (unix ms) are shown.
let since = 0;
let inFlight = false;

function isShortcut(appid: number): boolean {
  return (appid & 0x80000000) !== 0;
}

function show(u: Unlock) {
  toaster.toast({
    title: "Achievement unlocked",
    body: u.display_name,
    subtext: u.game_name,
    logo: u.icon
      ? createElement("img", {
          src: u.icon,
          style: { width: "100%", height: "100%", objectFit: "cover" },
        })
      : undefined,
  });
}

async function poll() {
  if (running.size === 0 && Date.now() > stopAt) {
    stop();
    return;
  }
  if (inFlight) return;
  inFlight = true;
  try {
    if (!base) base = (await getServerBase()).baseUrl;
    if (!base) return;
    const res = await fetch(`${base}/achievements/recent?since=${since}`);
    if (!res.ok) return;
    const unlocks = (await res.json()) as Unlock[];
    for (const u of unlocks) {
      since = Math.max(since, u.detected_at);
      show(u);
    }
  } catch {
    // The server may have restarted on a new port; resolve it again next tick.
    base = null;
  } finally {
    inFlight = false;
  }
}

function stop() {
  if (timer) clearInterval(timer);
  timer = null;
}

// Feed from SteamClient.GameSessions.RegisterForAppLifetimeNotifications.
export function onAppLifetime(appid: number, isRunning: boolean): void {
  if (!isShortcut(appid)) return;
  if (isRunning) {
    running.add(appid);
    if (!timer) {
      since = Date.now();
      timer = setInterval(() => void poll(), POLL_MS);
    }
  } else {
    running.delete(appid);
    stopAt = Date.now() + LINGER_MS;
  }
}

export function stopAchievementToasts(): void {
  running.clear();
  stop();
}
