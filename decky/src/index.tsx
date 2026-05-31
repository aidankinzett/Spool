import {
  callable,
  definePlugin,
  toaster,
  routerHook,
  addEventListener,
  removeEventListener,
} from "@decky/api";
import {
  ButtonItem,
  Focusable,
  Navigation,
  PanelSection,
  PanelSectionRow,
  TextField,
  ToggleField,
  staticClasses,
} from "@decky/ui";
import { useEffect, useState } from "react";
import { FaFloppyDisk } from "react-icons/fa6";

// Full-screen library route registered via routerHook. The QAM "Browse
// Library" button navigates here.
const SPOOL_ROUTE = "/spool";

// `SteamClient` (incl. GameSessions.RegisterForAppLifetimeNotifications and the
// LifetimeNotification payload) is provided as an ambient global by @decky/ui.
// The stop callback's `n` has `unAppID` (CRC app id — for Spool's non-Steam
// shortcuts it equals the `steam_appid` in active-session.json) and `bRunning`
// (false on a stop event).

const onAppStop = callable<[appid: number], { acted: boolean; game?: string }>(
  "on_app_stop",
);
const backupNow = callable<
  [],
  { acted: boolean; ok: boolean; game?: string; reason?: string }
>("backup_now");
const getStatus = callable<
  [],
  { hasSession: boolean; game?: string; backedUp?: boolean; startedAt?: string }
>("get_status");

interface Settings {
  spool_command: string;
  notify: boolean;
}
const getSettings = callable<[], Settings>("get_settings");
const setSettings = callable<
  [spool_command: string, notify: boolean],
  Settings
>("set_settings");

// Hands the UI the headless server's loopback base URL (e.g.
// "http://127.0.0.1:47650") so it can fetch /library and <img>-load /covers/*
// directly. `baseUrl` is null when the server isn't running.
const getServerBase = callable<[], { baseUrl: string | null }>(
  "get_server_base",
);

// Mirror of the fields the grid needs from the Rust `GameEntry`.
interface LibraryGame {
  id: string;
  game_name: string;
  cover_image_path: string | null;
  accent_color: string | null;
}

// ── Full-screen library grid ──────────────────────────────────────────────
//
// Talks to the headless server over loopback HTTP directly (not through the
// Decky callable bridge): one callable to learn the base URL, then a plain
// `fetch` for the library and `<img>` tags for covers. `http://127.0.0.1` is
// a secure origin, so the covers aren't blocked as mixed content from the
// https://steamloopback.host page.
function LibraryGrid() {
  const [base, setBase] = useState<string | null>(null);
  const [games, setGames] = useState<LibraryGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const { baseUrl } = await getServerBase();
      if (cancelled) return;
      if (!baseUrl) {
        setError("Spool isn’t running. Launch Spool, then try again.");
        return;
      }
      setBase(baseUrl);
      try {
        const res = await fetch(`${baseUrl}/library`);
        const data = (await res.json()) as LibraryGame[];
        if (!cancelled) setGames(data);
      } catch {
        if (!cancelled) setError("Couldn’t load your library.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const coverUrl = (g: LibraryGame): string | null => {
    if (!base || !g.cover_image_path) return null;
    const file = g.cover_image_path.split(/[/\\]/).pop();
    return file ? `${base}/covers/${encodeURIComponent(file)}` : null;
  };

  return (
    <div
      style={{
        height: "100%",
        overflowY: "scroll",
        padding: "2rem",
        boxSizing: "border-box",
      }}
    >
      <h1 style={{ margin: "0 0 1.25rem" }}>Spool Library</h1>

      {error && <div style={{ opacity: 0.8 }}>{error}</div>}
      {!error && !games && <div style={{ opacity: 0.7 }}>Loading…</div>}
      {games && games.length === 0 && (
        <div style={{ opacity: 0.7 }}>No games in your library yet.</div>
      )}

      {games && games.length > 0 && (
        <Focusable
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))",
            gap: "1.25rem",
          }}
        >
          {games.map((g) => {
            const url = coverUrl(g);
            return (
              <Focusable
                key={g.id}
                onActivate={() => {}}
                style={{
                  aspectRatio: "2 / 3",
                  borderRadius: "8px",
                  overflow: "hidden",
                  position: "relative",
                  display: "flex",
                  alignItems: "flex-end",
                  background: g.accent_color ?? "#1a2330",
                }}
              >
                {url ? (
                  <img
                    src={url}
                    alt={g.game_name}
                    style={{
                      position: "absolute",
                      inset: 0,
                      width: "100%",
                      height: "100%",
                      objectFit: "cover",
                    }}
                  />
                ) : (
                  <span
                    style={{
                      padding: "0.5rem",
                      fontSize: "0.85rem",
                      fontWeight: 600,
                      textShadow: "0 1px 3px rgba(0,0,0,0.85)",
                    }}
                  >
                    {g.game_name}
                  </span>
                )}
              </Focusable>
            );
          })}
        </Focusable>
      )}
    </div>
  );
}

function Content() {
  const [status, setStatus] = useState<Awaited<ReturnType<typeof getStatus>> | null>(
    null,
  );
  const [settings, setLocalSettings] = useState<Settings | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = async () => setStatus(await getStatus());
  useEffect(() => {
    void refresh();
    void getSettings().then(setLocalSettings);
  }, []);

  const save = async (patch: Partial<Settings>) => {
    const next = { ...(settings ?? { spool_command: "", notify: true }), ...patch };
    setLocalSettings(next);
    setLocalSettings(await setSettings(next.spool_command, next.notify));
  };

  return (
    <>
      <PanelSection title="Library">
        <PanelSectionRow>
          <ButtonItem
            layout="below"
            onClick={() => {
              Navigation.Navigate(SPOOL_ROUTE);
              Navigation.CloseSideMenus();
            }}
          >
            Browse Library
          </ButtonItem>
        </PanelSectionRow>
      </PanelSection>

      <PanelSection title="Spool Backup">
        <PanelSectionRow>
          {status?.hasSession ? (
            <div style={{ fontSize: "0.8rem", opacity: 0.85 }}>
              Last session: <strong>{status.game}</strong>
              <br />
              {status.backedUp ? "Backed up ✓" : "Not yet backed up"}
            </div>
          ) : (
            <div style={{ fontSize: "0.8rem", opacity: 0.7 }}>
              No active Spool session recorded.
            </div>
          )}
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem
            layout="below"
            disabled={busy || !status?.hasSession}
            onClick={async () => {
              setBusy(true);
              if (status?.game) {
                toaster.toast({
                  title: "Spool Backup",
                  body: `Backing up ${status.game}…`,
                });
              }
              const r = await backupNow();
              toaster.toast({
                title: "Spool Backup",
                body: !r.acted
                  ? "Nothing to back up"
                  : r.ok
                    ? `Backed up ${r.game} ✓`
                    : `Backup failed: ${r.reason ?? "unknown error"}`,
              });
              setBusy(false);
              void refresh();
            }}
          >
            Back up now
          </ButtonItem>
        </PanelSectionRow>
      </PanelSection>

      <PanelSection title="Settings">
        <PanelSectionRow>
          <ToggleField
            label="Notify on backup"
            description="Show a toast when a backup finishes."
            checked={settings?.notify ?? true}
            onChange={(value) => void save({ notify: value })}
          />
        </PanelSectionRow>
        <PanelSectionRow>
          <TextField
            label="Spool command"
            description="Override the auto-detected spool / spool-launcher.sh path."
            value={settings?.spool_command ?? ""}
            onChange={(e) => void save({ spool_command: e.target.value })}
          />
        </PanelSectionRow>
      </PanelSection>
    </>
  );
}

export default definePlugin(() => {
  // Register the full-screen library route. The QAM "Browse Library" button
  // navigates to it; we remove it on dismount to avoid duplicate patches
  // across hot-reloads.
  routerHook.addRoute(SPOOL_ROUTE, LibraryGrid);

  // Register the game-stop listener ONCE at plugin load (not inside the panel,
  // which unmounts when the QAM closes). On a stop, let the backend decide
  // whether a forced-close fallback backup is needed.
  const sub = SteamClient.GameSessions.RegisterForAppLifetimeNotifications(
    (n) => {
      if (!n.bRunning) {
        // Spool's non-Steam shortcut appids are `crc32(...) | 0x80000000`, so
        // the high bit is set. Steam surfaces those through `unAppID` as a
        // *signed* int32 (e.g. -105595925 instead of 4189371371), which would
        // never match the unsigned `steam_appid` in active-session.json. `>>> 0`
        // coerces it back to the unsigned 32-bit value the backend compares.
        void onAppStop(n.unAppID >>> 0);
      }
    },
  );

  const onBackupFinished = (game: string, ok: boolean, reason: string) => {
    toaster.toast({
      title: "Spool Backup",
      body: ok ? `Backed up ${game} ✓` : `Backup failed: ${reason || "unknown error"}`,
    });
  };
  addEventListener("spool_backup_finished", onBackupFinished);

  return {
    name: "Spool",
    titleView: <div className={staticClasses.Title}>Spool</div>,
    content: <Content />,
    icon: <FaFloppyDisk />,
    onDismount() {
      sub.unregister();
      removeEventListener("spool_backup_finished", onBackupFinished);
      routerHook.removeRoute(SPOOL_ROUTE);
    },
  };
});
