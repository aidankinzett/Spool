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

// Mirror of the Rust `LanPeer` (lan/discovery.rs).
interface LanPeer {
  device_id: string;
  device_name: string;
  addr: string;
  game_count: number;
  file_server_port: number;
  last_seen_ago_secs: number;
}

// Mirror of the Rust `PeerGame` (lan/server.rs) — only the fields the grid uses.
interface PeerGame {
  id: string;
  game_name: string;
}

// Resolve the headless server base URL once. The whole full-screen UI talks
// to the server over loopback HTTP directly (not the Decky callable bridge):
// `http://127.0.0.1` is a secure origin, so `<img>` covers aren't blocked as
// mixed content from the https://steamloopback.host page.
function useServerBase(): { base: string | null; error: string | null } {
  const [base, setBase] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const { baseUrl } = await getServerBase();
      if (cancelled) return;
      if (baseUrl) setBase(baseUrl);
      else setError("Spool isn’t running. Launch Spool, then try again.");
    })();
    return () => {
      cancelled = true;
    };
  }, []);
  return { base, error };
}

// Shared cover-tile grid used by both the local library and a peer's games.
// Tiles are focusable for controller nav; `onActivate` is optional (inert in
// the LAN browse view for now — installing lands in a later phase).
interface Tile {
  key: string;
  name: string;
  coverUrl: string | null;
  accentColor?: string | null;
}
function CoverGrid({
  tiles,
  onActivate,
}: {
  tiles: Tile[];
  onActivate?: (key: string) => void;
}) {
  return (
    <Focusable
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))",
        gap: "1.25rem",
      }}
    >
      {tiles.map((t) => (
        <Focusable
          key={t.key}
          onActivate={() => onActivate?.(t.key)}
          style={{
            aspectRatio: "2 / 3",
            borderRadius: "8px",
            overflow: "hidden",
            position: "relative",
            display: "flex",
            alignItems: "flex-end",
            background: t.accentColor ?? "#1a2330",
          }}
        >
          {t.coverUrl ? (
            <img
              src={t.coverUrl}
              alt={t.name}
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
              {t.name}
            </span>
          )}
        </Focusable>
      ))}
    </Focusable>
  );
}

// ── Local library grid ─────────────────────────────────────────────────────
function LibraryGrid({ base }: { base: string }) {
  const [games, setGames] = useState<LibraryGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${base}/library`);
        const data = (await res.json()) as LibraryGame[];
        if (!cancelled) setGames(data);
      } catch {
        if (!cancelled) setError("Couldn’t load your library.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [base]);

  const coverUrl = (g: LibraryGame): string | null => {
    if (!g.cover_image_path) return null;
    const file = g.cover_image_path.split(/[/\\]/).pop();
    return file ? `${base}/covers/${encodeURIComponent(file)}` : null;
  };

  if (error) return <div style={{ opacity: 0.8 }}>{error}</div>;
  if (!games) return <div style={{ opacity: 0.7 }}>Loading…</div>;
  if (games.length === 0)
    return <div style={{ opacity: 0.7 }}>No games in your library yet.</div>;

  return (
    <CoverGrid
      tiles={games.map((g) => ({
        key: g.id,
        name: g.game_name,
        coverUrl: coverUrl(g),
        accentColor: g.accent_color,
      }))}
    />
  );
}

// ── LAN browse ─────────────────────────────────────────────────────────────
//
// Peers and their games are fetched server-side: the UI can't hit a peer's
// non-loopback http directly (mixed content), so the headless server discovers
// peers and proxies their /games + covers. We poll /lan/peers (there's no push
// bus over plain HTTP). Selecting a peer shows its shared games as a grid;
// tiles are inert until the install phase.
function LanView({ base }: { base: string }) {
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

// A selected peer's shared games, fetched through the server-side proxy.
function PeerGames({
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
  const peerBase = `${base}/lan/peers/${peer.addr}/${peer.file_server_port}`;

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${peerBase}/games`);
        if (!res.ok) throw new Error();
        const data = (await res.json()) as PeerGame[];
        if (!cancelled) setGames(data);
      } catch {
        if (!cancelled) setError("Couldn’t reach this device.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [peerBase]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
      <Focusable>
        <ButtonItem layout="below" onClick={onBack}>
          ← Back to devices
        </ButtonItem>
      </Focusable>
      <h2 style={{ margin: 0 }}>{peer.device_name || peer.addr}</h2>

      {error && <div style={{ opacity: 0.8 }}>{error}</div>}
      {!error && !games && <div style={{ opacity: 0.7 }}>Loading…</div>}
      {games && games.length === 0 && (
        <div style={{ opacity: 0.7 }}>This device isn’t sharing any games.</div>
      )}
      {games && games.length > 0 && (
        <CoverGrid
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

// ── Full-screen page: Library | LAN toggle ─────────────────────────────────
function SpoolPage() {
  const { base, error } = useServerBase();
  const [view, setView] = useState<"library" | "lan">("library");

  const TabButton = ({ id, label }: { id: "library" | "lan"; label: string }) => (
    <Focusable
      onActivate={() => setView(id)}
      style={{
        padding: "0.5rem 1.25rem",
        borderRadius: "6px",
        fontWeight: 600,
        background: view === id ? "#2a3a52" : "transparent",
        opacity: view === id ? 1 : 0.7,
      }}
    >
      {label}
    </Focusable>
  );

  return (
    <div
      style={{
        height: "100%",
        overflowY: "scroll",
        padding: "2rem",
        boxSizing: "border-box",
      }}
    >
      <Focusable
        style={{
          display: "flex",
          gap: "0.5rem",
          alignItems: "center",
          marginBottom: "1.5rem",
        }}
      >
        <h1 style={{ margin: "0 1rem 0 0" }}>Spool</h1>
        <TabButton id="library" label="Library" />
        <TabButton id="lan" label="LAN" />
      </Focusable>

      {error && <div style={{ opacity: 0.8 }}>{error}</div>}
      {base && view === "library" && <LibraryGrid base={base} />}
      {base && view === "lan" && <LanView base={base} />}
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
  // Register the full-screen route (Library | LAN). The QAM "Browse Library"
  // button navigates to it; we remove it on dismount to avoid duplicate
  // patches across hot-reloads.
  routerHook.addRoute(SPOOL_ROUTE, SpoolPage);

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
