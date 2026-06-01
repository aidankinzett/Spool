// Persist game_id -> Steam appid so a game added to Steam once isn't re-added
// (which would duplicate the shortcut) on later launches. Lives in the CEF
// web-context localStorage (steamloopback.host origin).
const APPID_MAP_KEY = "spool:steamAppids";

export function loadAppidMap(): Record<string, number> {
  try {
    return JSON.parse(localStorage.getItem(APPID_MAP_KEY) || "{}");
  } catch {
    return {};
  }
}

export function rememberAppid(gameId: string, appid: number) {
  const map = loadAppidMap();
  map[gameId] = appid;
  localStorage.setItem(APPID_MAP_KEY, JSON.stringify(map));
}

// Reverse of loadAppidMap: maps steam_appid (non-Steam shortcut CRC id) -> spool game_id.
export function buildInverseAppidMap(): Record<number, string> {
  const map = loadAppidMap();
  return Object.fromEntries(
    Object.entries(map).map(([gameId, appid]) => [appid, gameId])
  );
}
