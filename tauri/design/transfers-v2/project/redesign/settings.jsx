/* Spool · Settings (restructured)
   Single-scroll page with anchored side nav.
   Three groups:
     · Library    — ludusavi path, theme, artwork
     · Sharing    — LAN share, sync server, device name
     · Sources    — Hydra source URLs, TorBox debrid */

const SETTINGS_GROUPS = [
  {
    id: "library",
    title: "Library",
    icon: <I d="M3 3h10v10H3z M3 6.5h10 M5 4.5v9 M3 9.5h10" />,
    items: [
      { id: "ludusavi", title: "Ludusavi", sub: "Save backup engine" },
      { id: "theme",    title: "Appearance", sub: "Density, accent" },
      { id: "artwork",  title: "Cover artwork", sub: "SteamGridDB" },
    ],
  },
  {
    id: "sharing",
    title: "Sharing & Sync",
    icon: <I d="M3 8a5 5 0 0 1 10 0M5 8a3 3 0 0 1 6 0M7 8a1 1 0 0 1 2 0M3 13h10" />,
    items: [
      { id: "lan",     title: "LAN sharing",  sub: "Transfers between devices" },
      { id: "sync",    title: "Sync server",  sub: "Session locks across devices" },
      { id: "device",  title: "This device",  sub: "Shown to peers" },
    ],
  },
  {
    id: "sources",
    title: "Sources & Downloads",
    icon: <I d="M3 5l5-2.5L13 5L8 7.5L3 5zM3 8l5 2.5L13 8M3 11l5 2.5L13 11" />,
    items: [
      { id: "hydra",  title: "Source feeds", sub: "Hydra JSON URLs" },
      { id: "torbox", title: "TorBox",       sub: "Debrid download provider" },
    ],
  },
];

function SettingsWindow({ width = 1180, height = 760 }) {
  const [active, setActive] = React.useState("ludusavi");
  const [state, setState] = React.useState({
    ludusaviPath: "C:\\Tools\\ludusavi\\ludusavi.exe",
    density: "comfortable",
    accent: "auto",
    sgdb: { enabled: true, apiKey: "sgdb_••••••••••••••••a4f2" },
    lan: { enabled: true, port: "47632", installDir: "D:\\Games\\Spool" },
    sync: { enabled: true, serverUrl: "http://nas.local:47633", apiKey: "spl_••••••••••••••••" },
    device: { name: "Workshop · Desktop" },
    sources: [
      "https://hydralinks.cloud/sources/elamigos.json",
      "https://hydralinks.cloud/sources/repacks.json",
      "https://my.gitea.host/spool/lan-sources.json",
    ],
    torbox: { enabled: false, apiKey: "", dir: "" },
  });
  const set = (patch) => setState(s => ({ ...s, ...patch }));

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: TOK.r.lg,
      overflow: "hidden",
      display: "flex", flexDirection: "column",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
    }}>
      <SettingsChrome />
      <div style={{ flex: 1, display: "grid", gridTemplateColumns: "260px 1fr", minHeight: 0 }}>
        <SettingsNav active={active} onJump={setActive} />
        <SettingsScroll s={state} set={set} active={active} onActive={setActive} />
      </div>
    </div>
  );
}

function SettingsChrome() {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 14,
      height: TOK.d.desktop.titleBar,
      padding: "0 8px 0 14px",
      background: "rgba(0,0,0,0.32)",
      borderBottom: `1px solid ${TOK.c.line}`,
      userSelect: "none",
    }}>
      <SpoolMark size={18} color={TOK.c.ink1} tape={TOK.c.spool} />
      <MonoLabel size={10.5}>SPOOL</MonoLabel>
      <span style={{ color: TOK.c.ink3, fontSize: 10 }}>/</span>
      <MonoLabel size={10.5} color={TOK.c.ink1}>SETTINGS</MonoLabel>
      <div style={{ flex: 1 }} />
      <div style={{
        display: "flex", alignItems: "center", gap: 7,
        padding: "0 8px",
        height: 22, background: TOK.c.bg2,
        border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
        fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink2, letterSpacing: "0.08em",
      }}>
        <span style={{ color: TOK.c.ok, width: 5, height: 5, borderRadius: 3, background: TOK.c.ok }} />
        ALL CHANGES SAVED
      </div>
      <div style={{ width: 6 }} />
      <ChromeBtn glyph="min" />
      <ChromeBtn glyph="max" />
      <ChromeBtn glyph="close" />
    </div>
  );
}

/* ─────────────────────────── Side nav ─────────────────────────── */
function SettingsNav({ active, onJump }) {
  return (
    <nav style={{
      borderRight: `1px solid ${TOK.c.line}`,
      background: TOK.c.bg1,
      padding: "20px 14px",
      overflowY: "auto",
    }}>
      {SETTINGS_GROUPS.map((g, gi) => (
        <div key={g.id} style={{ marginBottom: 22 }}>
          <div style={{
            display: "flex", alignItems: "center", gap: 8,
            padding: "0 8px 6px",
          }}>
            <span style={{ color: TOK.c.ink2, display: "flex" }}>{g.icon}</span>
            <MonoLabel size={10}>{g.title}</MonoLabel>
          </div>
          {g.items.map(it => {
            const isActive = active === it.id;
            return (
              <button
                key={it.id}
                onClick={() => onJump(it.id)}
                style={{
                  display: "flex", flexDirection: "column", alignItems: "flex-start",
                  width: "100%", textAlign: "left",
                  padding: "6px 8px",
                  background: isActive ? TOK.c.bg3 : "transparent",
                  borderLeft: `2px solid ${isActive ? TOK.c.spool : "transparent"}`,
                  border: "none",
                  borderRadius: 0,
                  cursor: "pointer",
                  fontFamily: TOK.font.ui,
                  gap: 1,
                  color: isActive ? TOK.c.ink0 : TOK.c.ink1,
                }}
                onMouseEnter={(e) => { if (!isActive) e.currentTarget.style.background = TOK.c.bg2; }}
                onMouseLeave={(e) => { if (!isActive) e.currentTarget.style.background = "transparent"; }}
              >
                <span style={{ fontSize: 12.5, fontWeight: 500 }}>{it.title}</span>
                <span style={{ fontSize: 10.5, color: TOK.c.ink3 }}>{it.sub}</span>
              </button>
            );
          })}
        </div>
      ))}

      <div style={{
        marginTop: 24, padding: "10px 8px",
        borderTop: `1px dashed ${TOK.c.line}`,
        display: "flex", flexDirection: "column", gap: 4,
      }}>
        <MonoLabel size={9}>v3.0.1 · 2026.05</MonoLabel>
        <a style={{ fontSize: 11, color: TOK.c.ink2 }}>Release notes</a>
        <a style={{ fontSize: 11, color: TOK.c.ink2 }}>Logs · Diagnostics</a>
      </div>
    </nav>
  );
}

/* ─────────────────────────── Scroll body ─────────────────────────── */
function SettingsScroll({ s, set, active, onActive }) {
  return (
    <div style={{
      overflowY: "auto",
      padding: "32px 40px 80px",
      scrollBehavior: "smooth",
    }}>
      <div style={{ maxWidth: 720, margin: "0 auto" }}>
        <div style={{ marginBottom: 30 }}>
          <MonoLabel size={10}>Spool · cabinet</MonoLabel>
          <h1 style={{
            fontFamily: TOK.font.display, fontSize: 32, fontWeight: 700,
            letterSpacing: "-0.022em", margin: "6px 0 4px",
          }}>Settings</h1>
          <p style={{ margin: 0, fontSize: 13, color: TOK.c.ink2, maxWidth: 540, lineHeight: 1.55 }}>
            Set up Ludusavi, share games on your LAN, and connect external sources. Changes save as you go.
          </p>
        </div>

        {/* Library group */}
        <Group title="Library" sub="Where saves live and how the shelf looks.">
          <Section id="ludusavi" title="Ludusavi"
                   helper="Spool delegates save backup/restore to ludusavi. We won't touch a game without it.">
            <Field label="Executable" helper={s.ludusaviPath ? "Detected v0.27.0" : "Browse to ludusavi.exe"} status={s.ludusaviPath ? "ok" : "warn"}>
              <Row>
                <Input value={s.ludusaviPath} mono onChange={(v) => set({ ludusaviPath: v })} />
                <Btn icon={ICN.folder}>Browse</Btn>
              </Row>
            </Field>
            <Field label="Conflict policy" helper="What happens when local and cloud saves disagree.">
              <Seg value="prompt" options={[
                { v: "prompt", l: "Ask each time" },
                { v: "local",  l: "Prefer local" },
                { v: "cloud",  l: "Prefer cloud" },
              ]}/>
            </Field>
          </Section>

          <Section id="theme" title="Appearance"
                   helper="Spool is dark-only — but you choose the temperature of the room.">
            <Field label="Density" helper="Touch is auto-applied on Steam Deck.">
              <Seg value="comfortable" options={[
                { v: "compact", l: "Compact" },
                { v: "comfortable", l: "Comfortable" },
                { v: "touch", l: "Touch" },
              ]}/>
            </Field>
            <Field label="Accent" helper="Chrome accent. Game detail pages use their cover art accent regardless.">
              <Row>
                {[TOK.c.spool, "#a5d5ff", "#a5edc1", "#f6cf94", "#bf8cf5", "#ff8a8a"].map(c => (
                  <Swatch key={c} c={c} active={c === TOK.c.spool} />
                ))}
                <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, marginLeft: 6 }}>
                  Current · SPOOL OXIDE
                </span>
              </Row>
            </Field>
            <Field label="Reduce motion" helper="Disables rotating reels and other ambient animations.">
              <Toggle value={false} />
            </Field>
          </Section>

          <Section id="artwork" title="Cover artwork"
                   helper="Cover, hero, and logo art is fetched from SteamGridDB when you add a game.">
            <ToggleField
              label="Use SteamGridDB"
              helper={s.sgdb.enabled ? "Authenticated · 2 312 covers in cache" : "Disabled — covers use generated placeholders."}
              status={s.sgdb.enabled ? "ok" : "off"}
              value={s.sgdb.enabled}
              onChange={(v) => set({ sgdb: { ...s.sgdb, enabled: v } })}
            >
              <Field label="API key">
                <Row>
                  <Input value={s.sgdb.apiKey} mono password />
                  <Btn icon={ICN.key}>Get a key</Btn>
                </Row>
              </Field>
              <Field label="Art style" helper="Preference order when multiple grids are available.">
                <Seg value="alt" options={[
                  { v: "alt",   l: "Alternate" },
                  { v: "white", l: "White-logo" },
                  { v: "none",  l: "No logo" },
                ]}/>
              </Field>
            </ToggleField>
          </Section>
        </Group>

        {/* Sharing group */}
        <Group title="Sharing & Sync" sub="Between your devices, and across your home network.">
          <Section id="lan" title="LAN sharing"
                   helper="Discovers other Spool instances on your local network and shares game installs over HTTP.">
            <ToggleField
              label="Share installs over LAN"
              helper={s.lan.enabled ? `Listening on :${s.lan.port} · 2 peers visible` : "Off — your installs stay private."}
              status={s.lan.enabled ? "ok" : "off"}
              value={s.lan.enabled}
              onChange={(v) => set({ lan: { ...s.lan, enabled: v } })}
            >
              <Field label="Port" helper="TCP port peers connect to.">
                <Input value={s.lan.port} mono style={{ width: 120 }} />
              </Field>
              <Field label="Default install dir" helper="Where downloads from peers land.">
                <Row>
                  <Input value={s.lan.installDir} mono />
                  <Btn icon={ICN.folder}>Browse</Btn>
                </Row>
              </Field>
              <PeerListPreview />
            </ToggleField>
          </Section>

          <Section id="sync" title="Sync server"
                   helper="A small HTTP service that holds a per-game lock so two devices don't fight over saves.">
            <ToggleField
              label="Use a sync server"
              helper={s.sync.enabled ? `${s.sync.serverUrl} · v3.8.2 · 4 devices registered` : "Off — you'll only get local backups."}
              status={s.sync.enabled ? "ok" : "off"}
              value={s.sync.enabled}
              onChange={(v) => set({ sync: { ...s.sync, enabled: v } })}
            >
              <Field label="Server URL">
                <Row>
                  <Input value={s.sync.serverUrl} mono prefix={ICN.cloud} />
                  <Btn icon={ICN.wifi}>Scan LAN</Btn>
                </Row>
              </Field>
              <Field label="API key">
                <Row>
                  <Input value={s.sync.apiKey} mono password />
                  <Btn>Register…</Btn>
                </Row>
              </Field>
            </ToggleField>
          </Section>

          <Section id="device" title="This device"
                   helper="The label other Spool devices see in their peer list.">
            <Field label="Device name">
              <Input value={s.device.name} />
            </Field>
            <Field label="OS / Role" helper="Auto-detected — you can override.">
              <Row>
                <Pill kind="info" soft>Linux · Desktop</Pill>
                <Pill kind="off" soft>Workshop</Pill>
                <Btn icon={ICN.pencil} style={{ height: 24, fontSize: 11.5 }}>Edit tags</Btn>
              </Row>
            </Field>
          </Section>
        </Group>

        {/* Sources */}
        <Group title="Sources & Downloads" sub="Where to find new games, and how to fetch them.">
          <Section id="hydra" title="Source feeds"
                   helper="Hydra-compatible JSON feeds. The Browse Games window aggregates everything listed here.">
            <SourceList items={s.sources} onRemove={(u) => set({ sources: s.sources.filter(x => x !== u) })} />
          </Section>

          <Section id="torbox" title="TorBox"
                   helper="Cloud debrid service. Spool fetches files via your TorBox account when you click 'Download'.">
            <ToggleField
              label="Enable TorBox"
              helper={s.torbox.enabled ? "Linked" : "Off — local & LAN sources only."}
              status={s.torbox.enabled ? "ok" : "off"}
              value={s.torbox.enabled}
              onChange={(v) => set({ torbox: { ...s.torbox, enabled: v } })}
            >
              <Field label="API key">
                <Row>
                  <Input value={s.torbox.apiKey} placeholder="Paste TorBox key…" password mono />
                  <Btn icon={ICN.key}>Get a key</Btn>
                </Row>
              </Field>
              <Field label="Download to">
                <Row>
                  <Input value={s.torbox.dir} placeholder="Default: ~/Downloads" mono readOnly />
                  <Btn icon={ICN.folder}>Browse</Btn>
                </Row>
              </Field>
            </ToggleField>
          </Section>
        </Group>

        {/* Danger zone */}
        <Group title="Advanced" sub="Maintenance, escape hatches.">
          <Section title="Reset" helper="Lose all settings and start over. The library and your saves stay.">
            <Row>
              <Btn danger icon={ICN.trash}>Reset settings</Btn>
              <Btn icon={ICN.external}>Open config folder</Btn>
              <Btn icon={ICN.copy}>Copy diagnostics</Btn>
            </Row>
          </Section>
        </Group>
      </div>
    </div>
  );
}

/* ─────────────────────────── Settings primitives ─────────────────────────── */
function Group({ title, sub, children }) {
  return (
    <section style={{ marginBottom: 36 }}>
      <div style={{ marginBottom: 14, paddingBottom: 10, borderBottom: `1px solid ${TOK.c.line}` }}>
        <h2 style={{ fontFamily: TOK.font.display, fontSize: 20, fontWeight: 600, letterSpacing: "-0.01em", margin: 0 }}>{title}</h2>
        {sub && <div style={{ fontSize: 12, color: TOK.c.ink2, marginTop: 3 }}>{sub}</div>}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>{children}</div>
    </section>
  );
}

function Section({ id, title, helper, children }) {
  return (
    <div id={id} style={{
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line}`,
      borderRadius: TOK.r.md,
      overflow: "hidden",
    }}>
      <div style={{
        padding: "14px 18px 12px",
        borderBottom: `1px dashed ${TOK.c.line}`,
        background: TOK.c.bg2,
      }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: TOK.c.ink0 }}>{title}</div>
        {helper && <div style={{ fontSize: 11.5, color: TOK.c.ink2, marginTop: 3, lineHeight: 1.5, maxWidth: 540 }}>{helper}</div>}
      </div>
      <div style={{ padding: "8px 0" }}>
        {children}
      </div>
    </div>
  );
}

function Field({ label, helper, status, children }) {
  return (
    <div style={{
      display: "grid", gridTemplateColumns: "180px 1fr", gap: 18, alignItems: "start",
      padding: "12px 18px",
    }}>
      <div style={{ paddingTop: 6 }}>
        <div style={{
          fontSize: 12.5, color: TOK.c.ink0, fontWeight: 500,
          display: "flex", alignItems: "center", gap: 6,
        }}>
          {label}
          {status && <span style={{
            width: 5, height: 5, borderRadius: 3,
            background: status === "ok" ? TOK.c.ok : status === "warn" ? TOK.c.warn : TOK.c.ink3,
          }}/>}
        </div>
        {helper && <div style={{ fontSize: 11, color: TOK.c.ink2, marginTop: 3, lineHeight: 1.5 }}>{helper}</div>}
      </div>
      <div>{children}</div>
    </div>
  );
}

function ToggleField({ label, helper, status, value, onChange, children }) {
  return (
    <div style={{ borderBottom: `1px dashed ${TOK.c.line}` }}>
      <div style={{
        display: "flex", alignItems: "center", gap: 14,
        padding: "14px 18px",
      }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 13, fontWeight: 500, color: TOK.c.ink0, display: "flex", alignItems: "center", gap: 8 }}>
            {label}
            {status === "ok" && <Pill kind="ok">{value ? "ON" : "OFF"}</Pill>}
            {status === "off" && <Pill kind="off">OFF</Pill>}
          </div>
          {helper && <div style={{ fontSize: 11.5, color: TOK.c.ink2, marginTop: 3 }}>{helper}</div>}
        </div>
        <Toggle value={value} onChange={onChange} />
      </div>
      {value && (
        <div style={{ background: TOK.c.bg0, paddingBottom: 4 }}>
          {children}
        </div>
      )}
    </div>
  );
}

function Row({ children }) {
  return <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>{children}</div>;
}

function Input({ value, onChange, mono, password, placeholder, prefix, readOnly, style }) {
  const [focus, setFocus] = React.useState(false);
  const [reveal, setReveal] = React.useState(false);
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 8,
      flex: 1, minWidth: 0,
      height: 30, padding: "0 10px",
      background: TOK.c.bg2,
      border: `1px solid ${focus ? TOK.c.line3 : TOK.c.line}`,
      borderRadius: TOK.r.sm,
      ...style,
    }}>
      {prefix && <span style={{ color: TOK.c.ink2, display: "flex" }}>{prefix}</span>}
      <input
        value={value || ""}
        onChange={(e) => onChange && onChange(e.target.value)}
        type={password && !reveal ? "password" : "text"}
        placeholder={placeholder}
        readOnly={readOnly}
        onFocus={() => setFocus(true)} onBlur={() => setFocus(false)}
        style={{
          flex: 1, minWidth: 0, background: "transparent", border: "none", outline: "none",
          color: TOK.c.ink0, fontFamily: mono ? TOK.font.mono : TOK.font.ui, fontSize: 12.5,
        }}
      />
      {password && (
        <button onClick={() => setReveal(r => !r)} style={{
          background: "transparent", border: "none", color: TOK.c.ink2, cursor: "pointer", display: "flex",
        }} tabIndex={-1}>
          {ICN.eye}
        </button>
      )}
    </div>
  );
}

function Toggle({ value, onChange }) {
  return (
    <button
      onClick={() => onChange && onChange(!value)}
      style={{
        position: "relative",
        width: 36, height: 20, borderRadius: 11,
        background: value ? TOK.c.spool : TOK.c.bg3,
        border: `1px solid ${value ? TOK.c.spool : TOK.c.line2}`,
        cursor: "pointer", padding: 0, flexShrink: 0,
        transition: "background 120ms ease",
      }}
    >
      <span style={{
        position: "absolute", top: 2, left: value ? 18 : 2,
        width: 14, height: 14, borderRadius: 7,
        background: value ? TOK.c.bg0 : TOK.c.ink1,
        transition: "left 140ms cubic-bezier(.2,.9,.3,1.2)",
      }} />
    </button>
  );
}

function Seg({ value, options }) {
  const [v, setV] = React.useState(value);
  return (
    <div style={{
      display: "inline-flex",
      background: TOK.c.bg2,
      border: `1px solid ${TOK.c.line}`,
      borderRadius: TOK.r.sm,
      padding: 2,
    }}>
      {options.map(o => (
        <button
          key={o.v} onClick={() => setV(o.v)}
          style={{
            padding: "4px 12px", height: 24,
            background: o.v === v ? TOK.c.bg3 : "transparent",
            color: o.v === v ? TOK.c.ink0 : TOK.c.ink2,
            border: "none", borderRadius: 2,
            fontFamily: TOK.font.ui, fontSize: 11.5, fontWeight: 500, cursor: "pointer",
          }}
        >{o.l}</button>
      ))}
    </div>
  );
}

function Swatch({ c, active }) {
  return (
    <button style={{
      width: 22, height: 22, borderRadius: 4,
      background: c,
      border: `1.5px solid ${active ? TOK.c.ink0 : "transparent"}`,
      boxShadow: `0 0 0 1px ${TOK.c.line2}`,
      cursor: "pointer",
    }}/>
  );
}

/* ─────────────────────────── Peer list (under LAN toggle) ─────────────────────────── */
function PeerListPreview() {
  const peers = [
    { name: "Workshop · Desktop", os: "this device", role: "host", games: 12 },
    { name: "Living room · Deck", os: "Linux · Deck",   role: "peer", games: 8, latency: 4 },
    { name: "Office · ThinkPad",  os: "Linux",          role: "peer", games: 22, latency: 11 },
  ];
  return (
    <div style={{
      margin: "0 18px 14px", border: `1px dashed ${TOK.c.line2}`, borderRadius: TOK.r.sm,
      background: TOK.c.bg0,
    }}>
      <div style={{
        padding: "8px 12px",
        borderBottom: `1px dashed ${TOK.c.line}`,
        display: "flex", justifyContent: "space-between", alignItems: "center",
      }}>
        <MonoLabel size={9.5}>Discovered peers</MonoLabel>
        <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.08em" }}>
          UDP · BROADCAST · 47632
        </span>
      </div>
      {peers.map((p, i) => (
        <div key={p.name} style={{
          display: "grid",
          gridTemplateColumns: "16px 1fr auto auto auto",
          gap: 12, alignItems: "center",
          padding: "8px 12px",
          borderBottom: i < peers.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
        }}>
          <span style={{
            width: 7, height: 7, borderRadius: 4,
            background: p.role === "host" ? TOK.c.spool : TOK.c.ok,
            boxShadow: `0 0 8px ${p.role === "host" ? TOK.c.spool : TOK.c.ok}66`,
          }} />
          <div style={{ display: "flex", flexDirection: "column", gap: 1, minWidth: 0 }}>
            <span style={{ fontSize: 12, color: TOK.c.ink0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p.name}</span>
            <span style={{ fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em" }}>
              {p.os}
            </span>
          </div>
          <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink2, letterSpacing: "0.06em" }}>
            {p.games} games
          </span>
          <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.06em" }}>
            {p.latency ? p.latency + " ms" : "—"}
          </span>
          {p.role === "host" ? (
            <Pill kind="off" soft>this device</Pill>
          ) : (
            <Btn style={{ height: 22, fontSize: 11 }}>Browse</Btn>
          )}
        </div>
      ))}
    </div>
  );
}

/* ─────────────────────────── Source list ─────────────────────────── */
function SourceList({ items, onRemove }) {
  const [newUrl, setNewUrl] = React.useState("");
  return (
    <div style={{ padding: "12px 18px 16px" }}>
      <Row>
        <Input value={newUrl} onChange={setNewUrl} placeholder="https://example.com/source.json" mono />
        <Btn variant="primary" accent={TOK.c.spool} icon={ICN.plus}>Add</Btn>
      </Row>
      <div style={{
        marginTop: 10,
        border: `1px solid ${TOK.c.line}`,
        borderRadius: TOK.r.sm,
        background: TOK.c.bg0,
      }}>
        {items.map((url, i) => (
          <div key={url} style={{
            display: "flex", alignItems: "center", gap: 10,
            padding: "8px 12px",
            borderBottom: i < items.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
          }}>
            <span style={{ width: 6, height: 6, borderRadius: 3, background: TOK.c.ok }} />
            <span style={{
              flex: 1, fontFamily: TOK.font.mono, fontSize: 11, color: TOK.c.ink1,
              whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
            }}>{url}</span>
            <span style={{ fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em" }}>
              {200 + i * 47} games · last sync 2h
            </span>
            <button
              onClick={() => onRemove(url)}
              title="Remove"
              style={{ background: "transparent", border: "none", color: TOK.c.ink3, cursor: "pointer", padding: 4, display: "flex" }}
            >
              {ICN.trash}
            </button>
          </div>
        ))}
      </div>
      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "center",
        marginTop: 10, fontSize: 11, color: TOK.c.ink3,
      }}>
        <span>{items.length} feeds · refreshed every 6h</span>
        <a style={{ color: TOK.c.spool, textDecoration: "none", fontSize: 11.5 }}>
          Browse community sources →
        </a>
      </div>
    </div>
  );
}

Object.assign(window, { SettingsWindow, SETTINGS_GROUPS });
