/* Spool · Additional Settings sections + per-route artboard wrapper.
   These are designed to drop into the existing settings IA — the route
   wrapper renders just one section in the settings chrome so you can
   build pages one at a time. */

/* ─────────────────────────── ROUTE WRAPPER ─────────────────────────── */
/* Reuses the SettingsWindow chrome but shows a single focused section. */

const ALL_ROUTES = [
  { group: "Library",            items: [
    { id: "ludusavi",  title: "Ludusavi",  sub: "Save backup engine" },
    { id: "backups",   title: "Backups",   sub: "Triggers · retention" },
    { id: "appearance",title: "Appearance",sub: "Density · accent" },
    { id: "artwork",   title: "Cover artwork", sub: "SteamGridDB" },
  ]},
  { group: "Sharing & Sync",     items: [
    { id: "lan",     title: "LAN sharing", sub: "Transfers between devices" },
    { id: "sync",    title: "Sync server", sub: "Session locks · devices" },
    { id: "device",  title: "This device", sub: "Shown to peers" },
  ]},
  { group: "Sources & Downloads",items: [
    { id: "hydra",   title: "Source feeds",sub: "Hydra JSON URLs" },
    { id: "torbox",  title: "TorBox",       sub: "Debrid provider" },
  ]},
  { group: "Notifications",      items: [
    { id: "notifs",  title: "Notifications",sub: "Toasts · sounds" },
  ]},
  { group: "Maintenance",        items: [
    { id: "updates",   title: "Updates",   sub: "Spool · games" },
    { id: "shortcuts", title: "Shortcuts", sub: "Keyboard reference" },
    { id: "about",     title: "About",     sub: "Version · diagnostics" },
  ]},
];

function SettingsRoute({ width = 1180, height = 760, active }) {
  return (
    <div style={{
      width, height,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
      display: "flex", flexDirection: "column",
    }}>
      <SettingsChrome />
      <div style={{ flex: 1, display: "grid", gridTemplateColumns: "260px 1fr", minHeight: 0 }}>
        <RouteNav active={active} />
        <RouteBody active={active} />
      </div>
    </div>
  );
}

function RouteNav({ active }) {
  return (
    <nav style={{
      borderRight: `1px solid ${TOK.c.line}`,
      background: TOK.c.bg1,
      padding: "20px 14px",
      overflowY: "auto",
    }}>
      {ALL_ROUTES.map(g => (
        <div key={g.group} style={{ marginBottom: 22 }}>
          <div style={{ padding: "0 8px 6px" }}>
            <MonoLabel size={10}>{g.group}</MonoLabel>
          </div>
          {g.items.map(it => {
            const isActive = active === it.id;
            return (
              <div
                key={it.id}
                style={{
                  display: "flex", flexDirection: "column", alignItems: "flex-start",
                  padding: "6px 8px",
                  background: isActive ? TOK.c.bg3 : "transparent",
                  borderLeft: `2px solid ${isActive ? TOK.c.spool : "transparent"}`,
                  gap: 1,
                  color: isActive ? TOK.c.ink0 : TOK.c.ink1,
                  cursor: "pointer",
                  fontFamily: TOK.font.ui,
                }}
              >
                <span style={{ fontSize: 12.5, fontWeight: isActive ? 500 : 400 }}>{it.title}</span>
                <span style={{ fontSize: 10.5, color: TOK.c.ink3 }}>{it.sub}</span>
              </div>
            );
          })}
        </div>
      ))}
    </nav>
  );
}

function RouteBody({ active }) {
  const route = ALL_ROUTES.flatMap(g => g.items).find(r => r.id === active);
  return (
    <div style={{ overflowY: "auto", padding: "32px 40px 80px" }}>
      <div style={{ maxWidth: 720, margin: "0 auto" }}>
        <div style={{ marginBottom: 26 }}>
          <MonoLabel size={10}>Spool · cabinet · /{active}</MonoLabel>
          <h1 style={{
            fontFamily: TOK.font.display, fontSize: 28, fontWeight: 700,
            letterSpacing: "-0.022em", margin: "6px 0 4px",
          }}>{route?.title || "Section"}</h1>
          <p style={{ margin: 0, fontSize: 13, color: TOK.c.ink2, maxWidth: 540, lineHeight: 1.55 }}>
            {ROUTE_BLURBS[active] || ""}
          </p>
        </div>
        {renderRoute(active)}
      </div>
    </div>
  );
}

const ROUTE_BLURBS = {
  ludusavi:   "Spool delegates save backup and restore to ludusavi. We won't track any game without it.",
  backups:    "When ludusavi runs, how many revisions to keep, where they live.",
  appearance: "Spool is dark-only — but you choose density, accent, and the rhythm of motion.",
  artwork:    "Cover, hero, and logo art is fetched from SteamGridDB when you add a game.",
  lan:        "Discovers other Spool devices on your local network and shares game installs over HTTP.",
  sync:       "A small HTTP service that holds the lock so two devices don't fight over saves.",
  device:     "The label other Spool devices see in their peer list, plus your unique device ID.",
  hydra:      "Hydra-compatible JSON feeds. The Browse Games window aggregates everything listed here.",
  torbox:     "Cloud debrid service. Spool fetches files via your TorBox account when you click Download.",
  notifs:     "Pick which events surface as toasts, and where they appear on screen.",
  updates:    "Spool itself, and the games in your library.",
  shortcuts:  "Every keyboard shortcut Spool currently listens to.",
  about:      "Version, license, and copy-paste diagnostics for filing issues.",
};

function renderRoute(id) {
  switch (id) {
    case "backups":   return <BackupsRoute />;
    case "sync":      return <SyncRouteRich />;
    case "notifs":    return <NotificationsRoute />;
    case "updates":   return <UpdatesRoute />;
    case "shortcuts": return <ShortcutsRoute />;
    case "about":     return <AboutRoute />;
    case "hydra":     return <FeedsRouteRich />;
    case "appearance":return <AppearanceRoute />;
    case "ludusavi":  return <LudusaviRouteRich />;
    default:          return <PlaceholderRoute />;
  }
}

function PlaceholderRoute() {
  return (
    <div style={{
      padding: 24, background: TOK.c.bg1, border: `1px dashed ${TOK.c.line}`,
      borderRadius: TOK.r.sm, color: TOK.c.ink3, fontSize: 12, textAlign: "center",
    }}>
      Section content lives in the main scroll view.
    </div>
  );
}

/* ─────────────────────────── LUDUSAVI (rich) ─────────────────────────── */
function LudusaviRouteRich() {
  return (
    <Section title="Engine" helper={null}>
      <Field label="Executable" helper="Detected v0.27.0 · /usr/bin/ludusavi" status="ok">
        <Row>
          <Input value="/usr/bin/ludusavi" mono />
          <Btn icon={ICN.folder}>Browse</Btn>
        </Row>
      </Field>
      <Field label="Manifest" helper="The list of known games and where they store saves. ludusavi updates this on its own; force a refresh below if a game you just added isn't recognized.">
        <Row>
          <span style={{
            display: "inline-flex", alignItems: "center", gap: 6,
            fontFamily: TOK.font.mono, fontSize: 11.5, color: TOK.c.ink1, letterSpacing: "0.04em",
          }}>
            <span style={{ width: 6, height: 6, borderRadius: 3, background: TOK.c.ok }} />
            18 042 entries · synced 6h ago
          </span>
          <Btn icon={ICN.cloud}>Refresh manifest</Btn>
        </Row>
      </Field>
      <Field label="Conflict policy" helper="What happens when local and cloud saves disagree.">
        <Seg value="prompt" options={[
          { v: "prompt", l: "Ask each time" },
          { v: "local",  l: "Prefer local" },
          { v: "cloud",  l: "Prefer cloud" },
        ]}/>
      </Field>
      <Field label="Ignored games" helper="Spool won't run ludusavi for these. Use the Edit dialog on a game to add it.">
        <div style={{
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
        }}>
          {[
            { n: "Helldivers 2",      r: "Always online" },
            { n: "Genshin Impact",    r: "Anti-cheat conflict" },
          ].map((g, i) => (
            <div key={g.n} style={{
              display: "flex", alignItems: "center", gap: 8,
              padding: "8px 12px",
              borderBottom: i < 1 ? `1px dashed ${TOK.c.line}` : "none",
              fontSize: 12,
            }}>
              <span style={{ flex: 1, color: TOK.c.ink1 }}>{g.n}</span>
              <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em" }}>{g.r}</span>
              <button style={{ background: "transparent", border: "none", color: TOK.c.ink3, cursor: "pointer", display: "flex", padding: 2 }}>{ICN.close}</button>
            </div>
          ))}
        </div>
      </Field>
      <Field label="Diagnostics">
        <Row>
          <Btn icon={ICN.external}>Open in terminal</Btn>
          <Btn icon={ICN.copy}>Copy --version output</Btn>
        </Row>
      </Field>
    </Section>
  );
}

/* ─────────────────────────── BACKUPS (new) ─────────────────────────── */
function BackupsRoute() {
  return (
    <>
      <Section title="Defaults" helper="Override per-game from the Edit dialog → Saves.">
        <Field label="Backup trigger" helper="When Spool tells ludusavi to capture a revision.">
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {[
              { id: "exit",  l: "When the game closes", sub: "Recommended — catches every session's progress.", on: true },
              { id: "launch",l: "When the game launches", sub: "Snapshot before you start, in case the game corrupts a save mid-run.", on: true },
              { id: "interval", l: "Every 30 minutes during play", sub: "Hot games that crash often. Slight overhead.", on: false },
              { id: "manual",l: "Manual only", sub: "Disable automatic backups entirely.", on: false },
            ].map(o => (
              <label key={o.id} style={{
                display: "flex", gap: 10, padding: "10px 12px",
                background: o.on ? `${TOK.c.spool}10` : TOK.c.bg2,
                border: `1px solid ${o.on ? TOK.c.spool + "55" : TOK.c.line2}`,
                borderRadius: TOK.r.sm, cursor: "pointer",
              }}>
                <span style={{
                  width: 14, height: 14, borderRadius: 3, marginTop: 2,
                  background: o.on ? TOK.c.spool : "transparent",
                  border: `1.4px solid ${o.on ? TOK.c.spool : TOK.c.line3}`,
                  display: "inline-flex", alignItems: "center", justifyContent: "center", flexShrink: 0,
                }}>
                  {o.on && <svg width="9" height="9" viewBox="0 0 9 9"><path d="M1.5 4.5 3.5 6.5 7.5 2.5" fill="none" stroke={TOK.c.bg0} strokeWidth="1.4" strokeLinecap="round" /></svg>}
                </span>
                <div>
                  <div style={{ fontSize: 12.5, fontWeight: 500 }}>{o.l}</div>
                  <div style={{ fontSize: 11, color: TOK.c.ink3, marginTop: 2 }}>{o.sub}</div>
                </div>
              </label>
            ))}
          </div>
        </Field>

        <Field label="Retention" helper="How many revisions to keep per game.">
          <Seg value="all" options={[
            { v: "10",  l: "Last 10" },
            { v: "50",  l: "Last 50" },
            { v: "size",l: "Up to 500 MB" },
            { v: "all", l: "Keep everything" },
          ]}/>
        </Field>

        <Field label="Compression" helper="Trade CPU for disk. Most users want Balanced.">
          <Seg value="balanced" options={[
            { v: "none",     l: "None" },
            { v: "balanced", l: "Balanced" },
            { v: "max",      l: "Maximum" },
          ]}/>
        </Field>

        <Field label="Archive location" helper="Where ludusavi writes the compressed revisions.">
          <Row>
            <Input value="/home/anna/.local/share/spool/saves" mono prefix={ICN.folder} />
            <Btn icon={ICN.folder}>Browse</Btn>
          </Row>
        </Field>
      </Section>

      <Section title="Usage" helper="Across all games on this device.">
        <div style={{
          display: "grid", gridTemplateColumns: "repeat(3, 1fr)",
          gap: 18, padding: "14px 18px",
        }}>
          <Stat label="REVISIONS" value="386" sub="across 18 games" />
          <Stat label="ARCHIVE SIZE" value="2.4 GB" sub="compressed" />
          <Stat label="OLDEST" value="14 mo ago" sub="Stardew Valley" />
        </div>
        <div style={{ padding: "0 18px 16px" }}>
          <Row>
            <Btn icon={ICN.upload}>Back up everything now</Btn>
            <Btn icon={ICN.trash} danger>Prune old revisions…</Btn>
          </Row>
        </div>
      </Section>
    </>
  );
}

/* ─────────────────────────── SYNC (rich) ─────────────────────────── */
function SyncRouteRich() {
  return (
    <>
      <Section title="Server" helper="A small HTTP service. Self-host (recommended) or skip and stay local-only.">
        <Field label="Status" status="ok" helper="Last successful sync 2 minutes ago.">
          <Pill kind="ok">Online · v3.8.2</Pill>
        </Field>
        <Field label="Server URL">
          <Row>
            <Input value="http://nas.local:47633" mono prefix={ICN.cloud} />
            <Btn icon={ICN.wifi}>Test</Btn>
          </Row>
        </Field>
        <Field label="API key">
          <Row>
            <Input value="spl_••••••••••••••••" mono password />
            <Btn>Rotate…</Btn>
          </Row>
        </Field>
        <Field label="Sync schedule" helper="Outside of game launch/exit, how often to push pending revisions.">
          <Seg value="5m" options={[
            { v: "live", l: "Live (push instantly)" },
            { v: "5m",   l: "Every 5 min" },
            { v: "manual", l: "Manual only" },
          ]}/>
        </Field>
      </Section>

      <Section title="Registered devices" helper="Devices that share saves through this server. Locks rotate between them automatically.">
        <div style={{
          margin: "0 18px 14px",
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`,
          borderRadius: TOK.r.sm,
        }}>
          {[
            { n: "Workshop · Desktop", os: "Linux",        you: true,  last: "2m ago",     state: "ok",  uuid: "spl-1f4c-…811" },
            { n: "Living room · Deck", os: "Linux · Deck", last: "14m ago",                state: "ok",  uuid: "spl-9af0-…2c33" },
            { n: "Office · ThinkPad",  os: "Linux",        last: "4h ago",                 state: "ok",  uuid: "spl-3b21-…0042" },
            { n: "Media · Mini PC",    os: "Linux",        last: "3 days ago",             state: "stale", uuid: "spl-cc4d-…aa10" },
          ].map((d, i, arr) => (
            <div key={d.uuid} style={{
              display: "grid", gridTemplateColumns: "20px 1fr 100px 90px 80px",
              gap: 10, alignItems: "center",
              padding: "9px 12px",
              borderBottom: i < arr.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
            }}>
              <span style={{
                width: 8, height: 8, borderRadius: 4,
                background: d.state === "ok" ? TOK.c.ok : TOK.c.warn,
                boxShadow: d.state === "ok" ? `0 0 6px ${TOK.c.ok}66` : "none",
              }} />
              <div style={{ minWidth: 0 }}>
                <div style={{
                  display: "flex", alignItems: "center", gap: 6,
                  fontSize: 12.5, fontWeight: 500,
                }}>
                  {d.n}
                  {d.you && <MonoLabel size={9} color={TOK.c.ink3}>YOU</MonoLabel>}
                </div>
                <div style={{ fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em", marginTop: 2 }}>
                  {d.uuid}
                </div>
              </div>
              <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink2, letterSpacing: "0.04em" }}>{d.os}</span>
              <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em" }}>{d.last}</span>
              {d.you
                ? <Pill kind="off" soft>this</Pill>
                : <Btn danger style={{ height: 22, fontSize: 10.5 }}>Forget</Btn>}
            </div>
          ))}
        </div>
      </Section>

      <Section title="Actions" helper="">
        <div style={{ padding: "12px 18px" }}>
          <Row>
            <Btn icon={ICN.cloud}>Force resync now</Btn>
            <Btn icon={ICN.external}>Open server admin</Btn>
            <Btn danger icon={ICN.close}>Unlink from server</Btn>
          </Row>
        </div>
      </Section>
    </>
  );
}

/* ─────────────────────────── APPEARANCE (rich) ─────────────────────────── */
function AppearanceRoute() {
  return (
    <>
      <Section title="Density" helper="Touch is auto-applied on Steam Deck. Override to force it on desktop.">
        <Field label="Mode">
          <Seg value="comfortable" options={[
            { v: "compact",     l: "Compact" },
            { v: "comfortable", l: "Comfortable" },
            { v: "touch",       l: "Touch" },
          ]}/>
        </Field>
        <Field label="Preview" helper="Live preview of typography at the current density.">
          <div style={{
            padding: "14px 16px",
            background: TOK.c.bg0,
            border: `1px solid ${TOK.c.line}`,
            borderRadius: TOK.r.sm,
          }}>
            <MonoLabel size={9.5}>SPL-0044 · SIDE A · EMBER</MonoLabel>
            <div style={{
              marginTop: 6, fontFamily: TOK.font.display, fontSize: 22, fontWeight: 700,
              letterSpacing: "-0.02em",
            }}>Elden Ring: Nightreign</div>
            <div style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, marginTop: 4, letterSpacing: "0.04em" }}>
              73h 41m · 64 sessions · last 2h ago
            </div>
          </div>
        </Field>
      </Section>

      <Section title="Accent" helper="Chrome accent. Game detail pages always use their cover-art accent regardless.">
        <Field label="Color">
          <div style={{ display: "flex", flexWrap: "wrap", gap: 10 }}>
            {[
              { c: TOK.c.spool, n: "Oxide" },
              { c: "#a5d5ff",   n: "Tape" },
              { c: "#a5edc1",   n: "Reel" },
              { c: "#f6cf94",   n: "Sleeve" },
              { c: "#bf8cf5",   n: "Magenta" },
              { c: "#ff8a8a",   n: "Rouge" },
            ].map((s, i) => (
              <div key={s.c} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6 }}>
                <button style={{
                  width: 36, height: 36, borderRadius: TOK.r.sm,
                  background: s.c,
                  border: `2px solid ${i === 0 ? TOK.c.ink0 : "transparent"}`,
                  boxShadow: `0 0 0 1px ${TOK.c.line2}`,
                  cursor: "pointer",
                }} />
                <span style={{ fontFamily: TOK.font.mono, fontSize: 9, color: TOK.c.ink3, letterSpacing: "0.06em" }}>
                  {s.n.toUpperCase()}
                </span>
              </div>
            ))}
          </div>
        </Field>
      </Section>

      <Section title="Motion" helper="">
        <Field label="Reduce motion" helper="Disables rotating reels and other ambient animations. Keeps essential transitions.">
          <Toggle value={false} />
        </Field>
        <Field label="Background rendering" helper="Render the cassette tape strip on each game detail page.">
          <Toggle value={true} />
        </Field>
      </Section>
    </>
  );
}

/* ─────────────────────────── FEEDS (rich) ─────────────────────────── */
function FeedsRouteRich() {
  const feeds = [
    { id: "elamigos", url: "https://hydralinks.cloud/sources/elamigos.json",  count: 1842, lastFetch: "2h ago", enabled: true,  expanded: true },
    { id: "repacks",  url: "https://hydralinks.cloud/sources/repacks.json",   count:  978, lastFetch: "2h ago", enabled: true },
    { id: "lan",      url: "https://gitea.workshop.local/spool/lan-sources.json", count: 124, lastFetch: "30m ago", enabled: true,  selfhosted: true },
  ];
  return (
    <>
      <Section title="Active feeds" helper="">
        <div style={{ padding: "12px 18px 6px" }}>
          <Row>
            <Input value="" placeholder="https://example.com/source.json" mono />
            <Btn variant="primary" accent={TOK.c.spool} icon={ICN.plus}>Add feed</Btn>
          </Row>
        </div>
        <div style={{ margin: "8px 18px 14px", display: "flex", flexDirection: "column", gap: 8 }}>
          {feeds.map(f => <FeedDetailCard key={f.id} f={f} />)}
        </div>
        <div style={{ padding: "0 18px 12px", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: 11.5, color: TOK.c.ink3 }}>
            All feeds aggregate into the Browse Games window.
          </span>
          <a style={{ color: TOK.c.spool, textDecoration: "none", fontSize: 11.5 }}>Community feed list →</a>
        </div>
      </Section>

      <Section title="Fetch behavior" helper="">
        <Field label="Refresh every" helper="Spool will pull each feed in the background.">
          <Seg value="6h" options={[
            { v: "1h",     l: "1 hour" },
            { v: "6h",     l: "6 hours" },
            { v: "24h",    l: "Daily" },
            { v: "manual", l: "Manual" },
          ]}/>
        </Field>
        <Field label="Stale results" helper="If a feed is unreachable, keep its cached entries available.">
          <Toggle value={true} />
        </Field>
      </Section>
    </>
  );
}

function FeedDetailCard({ f }) {
  const color = f.id === "elamigos" ? "#ff9a4f" : f.id === "repacks" ? "#bf6cf5" : "#7ee2a4";
  return (
    <div style={{
      background: TOK.c.bg2,
      border: `1px solid ${TOK.c.line}`,
      borderRadius: TOK.r.sm,
      overflow: "hidden",
    }}>
      <div style={{
        display: "flex", alignItems: "center", gap: 12,
        padding: "10px 12px",
        borderBottom: f.expanded ? `1px dashed ${TOK.c.line}` : "none",
      }}>
        <span style={{ width: 7, height: 7, borderRadius: 4, background: color, flexShrink: 0 }} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            fontFamily: TOK.font.mono, fontSize: 11.5, color: TOK.c.ink0,
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
          }}>{f.url}</div>
          <div style={{ display: "flex", gap: 10, marginTop: 3 }}>
            <MonoLabel size={9}>{f.count.toLocaleString()} games</MonoLabel>
            <MonoLabel size={9}>{f.lastFetch} · {f.enabled ? "ok" : "off"}</MonoLabel>
            {f.selfhosted && <MonoLabel size={9} color={TOK.c.ok}>SELF-HOSTED</MonoLabel>}
          </div>
        </div>
        <Toggle value={f.enabled} />
        <button style={{ background: "transparent", border: "none", color: TOK.c.ink3, cursor: "pointer", padding: 4, display: "flex" }}>
          {ICN.trash}
        </button>
      </div>
      {f.expanded && (
        <div style={{
          padding: "10px 12px",
          background: TOK.c.bg0,
          display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 14,
        }}>
          <Stat label="LAST FETCH" value={f.lastFetch} sub="200 OK · 142 KB" />
          <Stat label="NEW THIS WEEK" value="142" sub="games added" />
          <Stat label="UPDATE FAILURES" value="0" sub="last 30 days" />
        </div>
      )}
    </div>
  );
}

/* ─────────────────────────── NOTIFICATIONS (new) ─────────────────────────── */
function NotificationsRoute() {
  const events = [
    { id: "backup",   l: "Save backed up",     sub: "When ludusavi captures a new revision",       on: false, kind: "ok",   important: false },
    { id: "restore",  l: "Save restored",      sub: "Confirm restores worked",                     on: true,  kind: "ok" },
    { id: "conflict", l: "Save conflict",      sub: "Two devices wrote saves since last sync",     on: true,  kind: "warn", critical: true },
    { id: "peer",     l: "Peer joined LAN",    sub: "Another Spool device appears on the network", on: true,  kind: "info" },
    { id: "download", l: "Download complete",  sub: "From source feed, TorBox, or a LAN peer",     on: true,  kind: "ok" },
    { id: "update",   l: "Update available",   sub: "Spool itself, or a game in your library",     on: true,  kind: "info" },
    { id: "syncErr",  l: "Sync error",         sub: "Server unreachable or auth failed",           on: true,  kind: "bad",  critical: true },
    { id: "diskErr",  l: "Disk almost full",   sub: "Under 5 GB on the install drive",             on: true,  kind: "bad",  critical: true },
  ];
  return (
    <>
      <Section title="Events" helper="Pick which actions show a toast. Critical errors always show, regardless of these toggles.">
        <div style={{
          margin: "0 18px 14px",
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
        }}>
          {events.map((e, i) => (
            <div key={e.id} style={{
              display: "grid", gridTemplateColumns: "10px 1fr auto",
              gap: 12, alignItems: "center",
              padding: "11px 14px",
              borderBottom: i < events.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
              opacity: e.critical && !e.on ? 0.7 : 1,
            }}>
              <span style={{
                width: 6, height: 6, borderRadius: 3,
                background: e.kind === "ok" ? TOK.c.ok
                          : e.kind === "warn" ? TOK.c.warn
                          : e.kind === "bad" ? TOK.c.bad
                          : TOK.c.info,
              }} />
              <div>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span style={{ fontSize: 12.5, fontWeight: 500 }}>{e.l}</span>
                  {e.critical && <MonoLabel size={9} color={TOK.c.bad}>CRITICAL · ALWAYS ON</MonoLabel>}
                </div>
                <div style={{ fontSize: 11, color: TOK.c.ink3, marginTop: 2 }}>{e.sub}</div>
              </div>
              <Toggle value={e.on || e.critical} />
            </div>
          ))}
        </div>
      </Section>

      <Section title="Behavior" helper="">
        <Field label="Position">
          <div style={{ display: "flex", gap: 6 }}>
            {[
              { v: "tr", l: "Top right",    active: true },
              { v: "br", l: "Bottom right" },
              { v: "tl", l: "Top left" },
              { v: "bl", l: "Bottom left" },
            ].map(p => (
              <button key={p.v} style={{
                width: 64, height: 44,
                background: p.active ? `${TOK.c.spool}14` : TOK.c.bg2,
                border: `1px solid ${p.active ? TOK.c.spool + "66" : TOK.c.line2}`,
                borderRadius: TOK.r.sm, cursor: "pointer",
                position: "relative",
              }}>
                <span style={{
                  position: "absolute",
                  top: p.v.startsWith("t") ? 6 : "auto",
                  bottom: p.v.startsWith("b") ? 6 : "auto",
                  left: p.v.endsWith("l") ? 6 : "auto",
                  right: p.v.endsWith("r") ? 6 : "auto",
                  width: 22, height: 10, borderRadius: 2,
                  background: p.active ? TOK.c.spool : TOK.c.ink3,
                }} />
              </button>
            ))}
          </div>
        </Field>
        <Field label="Stay on screen for" helper="Critical errors stay until dismissed.">
          <Seg value="6s" options={[
            { v: "3s",     l: "3 s" },
            { v: "6s",     l: "6 s" },
            { v: "12s",    l: "12 s" },
            { v: "sticky", l: "Stay until dismissed" },
          ]}/>
        </Field>
        <Field label="Sound" helper="A single subtle tone, separate per kind.">
          <Toggle value={false} />
        </Field>
        <Field label="OS-level notifications" helper="Forward critical events to your desktop's notification system so you see them when Spool isn't focused.">
          <Toggle value={true} />
        </Field>
      </Section>
    </>
  );
}

/* ─────────────────────────── UPDATES (new) ─────────────────────────── */
function UpdatesRoute() {
  return (
    <>
      <Section title="Spool" helper="">
        <div style={{
          margin: "0 18px 12px",
          padding: "14px 16px",
          background: `linear-gradient(180deg, ${TOK.c.spool}10 0%, ${TOK.c.bg1} 100%)`,
          border: `1px solid ${TOK.c.spool}44`,
          borderRadius: TOK.r.md,
          display: "flex", alignItems: "center", gap: 14,
        }}>
          <SpinningReels />
          <div style={{ flex: 1 }}>
            <MonoLabel size={10} color={TOK.c.spool}>UPDATE AVAILABLE · 3.1.0</MonoLabel>
            <div style={{
              fontFamily: TOK.font.display, fontSize: 18, fontWeight: 600, letterSpacing: "-0.012em",
              marginTop: 4,
            }}>You're on v3.0.1 · v3.1.0 is ready.</div>
            <div style={{ fontSize: 11.5, color: TOK.c.ink2, marginTop: 4, lineHeight: 1.5 }}>
              LAN transfer resumes after disconnects · per-game Wine prefix support · settings search.
              <a style={{ color: TOK.c.spool, marginLeft: 4 }}>Full release notes →</a>
            </div>
          </div>
          <Btn variant="primary" accent={TOK.c.spool} icon={ICN.download}>Install & restart</Btn>
        </div>

        <Field label="Auto-update" helper="Download in the background and apply on the next launch.">
          <Toggle value={true} />
        </Field>
        <Field label="Channel" helper="Beta gets new features 1–2 weeks earlier.">
          <Seg value="stable" options={[
            { v: "stable", l: "Stable" },
            { v: "beta",   l: "Beta" },
            { v: "nightly",l: "Nightly" },
          ]}/>
        </Field>
        <Field label="Last checked">
          <Row>
            <span style={{ fontFamily: TOK.font.mono, fontSize: 11.5, color: TOK.c.ink1, letterSpacing: "0.04em" }}>
              4 minutes ago · github.com/spool/releases
            </span>
            <Btn>Check now</Btn>
          </Row>
        </Field>
      </Section>

      <Section title="Games" helper="Spool can watch your source feeds and ping you when a newer release of a game in your library appears.">
        <Field label="Notify on game updates">
          <Toggle value={true} />
        </Field>
        <Field label="Pending" helper="2 of 8 games have newer releases available.">
          <div style={{
            background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
          }}>
            {[
              { n: "Hades II",        cur: "v0.94", nxt: "v1.00", from: "FitGirl · elamigos.json" },
              { n: "Hollow Knight: Silksong", cur: "v1.0.2", nxt: "v1.0.4", from: "DODI · repacks.json" },
            ].map((g, i, arr) => (
              <div key={g.n} style={{
                display: "grid", gridTemplateColumns: "1fr auto auto",
                gap: 12, alignItems: "center",
                padding: "10px 12px",
                borderBottom: i < arr.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
              }}>
                <div>
                  <div style={{ fontSize: 12.5, fontWeight: 500 }}>{g.n}</div>
                  <div style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em", marginTop: 2 }}>
                    {g.from}
                  </div>
                </div>
                <span style={{ fontFamily: TOK.font.mono, fontSize: 11, color: TOK.c.ink2, letterSpacing: "0.04em" }}>
                  {g.cur} → <span style={{ color: TOK.c.spool }}>{g.nxt}</span>
                </span>
                <Btn icon={ICN.download} style={{ height: 24, fontSize: 11.5 }}>Update</Btn>
              </div>
            ))}
          </div>
        </Field>
      </Section>
    </>
  );
}

/* ─────────────────────────── SHORTCUTS (new) ─────────────────────────── */
function ShortcutsRoute() {
  const cmdOrCtrl = "⌘"; // shown as cross-platform glyph
  const groups = [
    { title: "Library", rows: [
      [["Search"], [cmdOrCtrl + "K"]],
      [["Add a game"], [cmdOrCtrl + "N"]],
      [["Toggle sidebar"], [cmdOrCtrl + "B"]],
      [["Next game"], ["↓"]],
      [["Previous game"], ["↑"]],
      [["Edit selected"], ["F2"]],
      [["Remove selected"], ["Del"]],
    ]},
    { title: "Detail", rows: [
      [["Play"], ["Enter"]],
      [["Open install folder"], [cmdOrCtrl + "O"]],
      [["Back up saves now"], [cmdOrCtrl + "B"]],
      [["Restore saves…"], [cmdOrCtrl + "R"]],
      [["Save history"], [cmdOrCtrl + "H"]],
      [["Copy install path"], [cmdOrCtrl + "Shift", "C"]],
    ]},
    { title: "Global", rows: [
      [["Settings"], [cmdOrCtrl + ","]],
      [["Browse sources"], [cmdOrCtrl + "P"]],
      [["LAN peers"], [cmdOrCtrl + "L"]],
      [["Downloads"], [cmdOrCtrl + "J"]],
      [["Open this reference"], ["?"]],
      [["Close window"], [cmdOrCtrl + "W"]],
    ]},
  ];
  return (
    <Section title="Keyboard reference" helper={null}>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18, padding: "12px 18px" }}>
        {groups.map(g => (
          <div key={g.title} style={{
            background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`,
            borderRadius: TOK.r.sm, overflow: "hidden",
          }}>
            <div style={{
              padding: "9px 12px",
              borderBottom: `1px dashed ${TOK.c.line}`,
              background: TOK.c.bg1,
            }}>
              <MonoLabel size={10}>{g.title.toUpperCase()}</MonoLabel>
            </div>
            <div>
              {g.rows.map(([label, keys], i) => (
                <div key={i} style={{
                  display: "flex", alignItems: "center", justifyContent: "space-between",
                  padding: "8px 12px",
                  borderBottom: i < g.rows.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
                  fontSize: 12,
                }}>
                  <span style={{ color: TOK.c.ink1 }}>{label}</span>
                  <span style={{ display: "inline-flex", gap: 4 }}>
                    {keys.map((k, j) => (
                      <kbd key={j} style={{
                        display: "inline-flex", alignItems: "center", justifyContent: "center",
                        minWidth: 20, height: 20, padding: "0 5px",
                        background: TOK.c.bg0,
                        border: `1px solid ${TOK.c.line2}`,
                        borderRadius: 3,
                        fontFamily: TOK.font.mono, fontSize: 9.5,
                        color: TOK.c.ink1, letterSpacing: "0.04em",
                      }}>{k}</kbd>
                    ))}
                  </span>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
      <div style={{
        padding: "10px 18px",
        fontSize: 11, color: TOK.c.ink3,
      }}>
        Shortcuts use ⌘ on macOS and Ctrl on Windows / Linux. They're not user-configurable in this version.
      </div>
    </Section>
  );
}

/* ─────────────────────────── ABOUT (new) ─────────────────────────── */
function AboutRoute() {
  return (
    <>
      <Section title="Version" helper={null}>
        <div style={{
          padding: "16px 18px",
          display: "grid", gridTemplateColumns: "auto 1fr", gap: 18, alignItems: "center",
        }}>
          <div style={{
            width: 96, height: 96, borderRadius: 18,
            background: `linear-gradient(155deg, ${TOK.c.spoolDeep} 0%, #322820 100%)`,
            display: "flex", alignItems: "center", justifyContent: "center",
            boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
          }}>
            <SpoolMark size={56} color={TOK.c.ink0} tape={TOK.c.spool} />
          </div>
          <div>
            <div style={{
              fontFamily: TOK.font.display, fontSize: 32, fontWeight: 700,
              letterSpacing: "-0.025em",
            }}>Spool 3.0.1</div>
            <div style={{ fontFamily: TOK.font.mono, fontSize: 11, color: TOK.c.ink2, letterSpacing: "0.06em", marginTop: 4 }}>
              build · 2026.05.27 · 1f4cd811
            </div>
            <div style={{ marginTop: 10, display: "flex", gap: 6 }}>
              <Pill kind="ok">Stable</Pill>
              <Pill kind="info" soft>Linux · x86_64</Pill>
              <Pill kind="off" soft>Tauri 2.4 · SvelteKit 2.20</Pill>
            </div>
          </div>
        </div>
      </Section>

      <Section title="Dependencies" helper="Spool stands on the shoulders of giants.">
        <div style={{
          margin: "0 18px 14px",
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
        }}>
          {[
            { n: "ludusavi",    v: "v0.27.0", role: "Save backup & restore", license: "MIT" },
            { n: "Tauri",       v: "v2.4.0",  role: "Native app shell",       license: "Apache 2.0 / MIT" },
            { n: "SvelteKit",   v: "v2.20.0", role: "UI framework",           license: "MIT" },
            { n: "rust-libp2p", v: "v0.53",   role: "LAN discovery",          license: "MIT" },
            { n: "rusqlite",    v: "v0.31",   role: "Local catalog storage",  license: "MIT" },
          ].map((d, i, arr) => (
            <div key={d.n} style={{
              display: "grid", gridTemplateColumns: "1fr 70px 1fr 80px auto",
              gap: 12, alignItems: "center",
              padding: "8px 12px",
              borderBottom: i < arr.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
              fontSize: 12,
            }}>
              <span style={{ color: TOK.c.ink0, fontWeight: 500 }}>{d.n}</span>
              <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.04em" }}>{d.v}</span>
              <span style={{ color: TOK.c.ink2, fontSize: 11.5 }}>{d.role}</span>
              <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em" }}>{d.license}</span>
              <button style={{
                background: "transparent", border: "none", color: TOK.c.ink3, cursor: "pointer",
                display: "flex", padding: 4,
              }}>{ICN.external}</button>
            </div>
          ))}
        </div>
      </Section>

      <Section title="Diagnostics" helper="Use when filing a bug report.">
        <div style={{
          margin: "0 18px 12px",
          padding: 12,
          background: TOK.c.bg0, border: `1px dashed ${TOK.c.line}`, borderRadius: TOK.r.sm,
          fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink1, letterSpacing: "0.02em",
          lineHeight: 1.7,
        }}>
          <div><span style={{ color: TOK.c.ink3 }}>spool      </span>3.0.1 · linux-x86_64</div>
          <div><span style={{ color: TOK.c.ink3 }}>ludusavi   </span>0.27.0 · /usr/bin/ludusavi</div>
          <div><span style={{ color: TOK.c.ink3 }}>sync       </span>http://nas.local:47633 · v3.8.2 · online</div>
          <div><span style={{ color: TOK.c.ink3 }}>device     </span>Workshop · Desktop · spl-1f4c-…811</div>
          <div><span style={{ color: TOK.c.ink3 }}>library    </span>18 games · 386 revisions · 2.4 GB</div>
          <div><span style={{ color: TOK.c.ink3 }}>peers      </span>3 online · 1 offline</div>
        </div>
        <div style={{ padding: "0 18px 12px" }}>
          <Row>
            <Btn icon={ICN.copy}>Copy diagnostics</Btn>
            <Btn icon={ICN.folder}>Open logs folder</Btn>
            <Btn icon={ICN.external}>Report an issue</Btn>
          </Row>
        </div>
      </Section>

      <Section title="Credits" helper={null}>
        <div style={{ padding: "12px 18px", fontSize: 12, color: TOK.c.ink1, lineHeight: 1.7 }}>
          Built by <a style={{ color: TOK.c.spool }}>@anna</a>. The cassette mark and Spool wordmark are <em>not</em> released under the MIT license that covers the code — use them only for the official builds.
        </div>
      </Section>
    </>
  );
}

Object.assign(window, {
  SettingsRoute, ALL_ROUTES,
  BackupsRoute, NotificationsRoute, UpdatesRoute, ShortcutsRoute, AboutRoute,
  SyncRouteRich, FeedsRouteRich, AppearanceRoute, LudusaviRouteRich,
});
