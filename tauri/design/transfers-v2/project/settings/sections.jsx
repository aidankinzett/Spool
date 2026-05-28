/* The 6 setting categories rendered as content pages.
   Each one consumes the shared settings state from the parent app. */

const SectionHeader = ({ icon, title, subtitle }) => (
  <div style={{ marginBottom: 18 }}>
    <div style={{
      display: "flex",
      alignItems: "center",
      gap: 12,
      marginBottom: subtitle ? 6 : 0,
    }}>
      <span style={{
        width: 36,
        height: 36,
        borderRadius: 8,
        background: "rgba(255,255,255,0.05)",
        border: "1px solid rgba(255,255,255,0.06)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        color: "rgba(255,255,255,0.9)",
      }}>{icon}</span>
      <div style={{
        fontFamily: "var(--font-display)",
        fontSize: 22,
        fontWeight: 600,
        letterSpacing: "-0.01em",
      }}>{title}</div>
    </div>
    {subtitle && (
      <div style={{
        fontSize: 13,
        color: "rgba(255,255,255,0.55)",
        marginLeft: 48,
        lineHeight: 1.5,
      }}>{subtitle}</div>
    )}
  </div>
);

const SectionStack = ({ children }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>{children}</div>
);

/* ============ GENERAL ============ */
const GeneralSection = ({ s, set, t }) => (
  <>
    <SectionHeader
      icon={<IconGeneral size={18} />}
      title="General"
      subtitle="Core paths and app behaviour."
    />
    <SectionStack>
      <SettingsCard
        icon={<IconFolder size={16} />}
        title="Ludusavi executable"
        description="Path to ludusavi.exe — used for save backup and restore."
        showIcons={t.showIcons}
        density={t.density}
        accent={t.accent}
        control={
          <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <TextField
              value={s.ludusaviPath}
              onChange={(v) => set({ ludusaviPath: v })}
              placeholder="C:\Path\to\ludusavi.exe"
              monospace
              accent={t.accent}
              style={{ width: 280 }}
            />
            <Button accent={t.accent}>Browse</Button>
          </div>
        }
        status={s.ludusaviPath ? { kind: "ok", text: "Detected" } : { kind: "warn", text: "Not found" }}
      />

      <SettingsCard
        icon={<IconAppearance size={16} />}
        title="Theme"
        description="Match Windows, or override with light or dark."
        showIcons={t.showIcons}
        density={t.density}
        accent={t.accent}
        control={
          <Select
            value={s.theme}
            onChange={(v) => set({ theme: v })}
            accent={t.accent}
            options={[
              { value: "system", label: "System default" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
            style={{ width: 180 }}
          />
        }
      />

      <SettingsCard
        icon={<IconInfo size={16} />}
        title="About"
        description="Spool v3.0.1 · Up to date"
        showIcons={t.showIcons}
        density={t.density}
        accent={t.accent}
        control={
          <Button variant="ghost" accent={t.accent}>Release notes</Button>
        }
      />
    </SectionStack>
  </>
);

/* ============ ARTWORK ============ */
const ArtworkSection = ({ s, set, t }) => (
  <>
    <SectionHeader
      icon={<IconArtwork size={18} />}
      title="Artwork"
      subtitle="Automatically fetch cover art for your library and launcher shortcuts."
    />
    <SectionStack>
      <SettingsCard
        icon={<IconArtwork size={16} />}
        title="SteamGridDB"
        description="Download cover images automatically when generating a wrapper."
        showIcons={t.showIcons}
        density={t.density}
        accent={t.accent}
        expandable
        expanded={s.sgdb.enabled}
        onToggleExpand={() => set({ sgdb: { ...s.sgdb, enabled: !s.sgdb.enabled } })}
        control={
          <ToggleSwitch
            checked={s.sgdb.enabled}
            onChange={(v) => set({ sgdb: { ...s.sgdb, enabled: v } })}
            accent={t.accent}
          />
        }
        status={s.sgdb.enabled && s.sgdb.apiKey ? { kind: "ok", text: "Authenticated" } : null}
      >
        <SubField label="API Key" helper="Required to download cover, hero, and logo artwork.">
          <div style={{ display: "flex", gap: 6 }}>
            <TextField
              value={s.sgdb.apiKey}
              onChange={(v) => set({ sgdb: { ...s.sgdb, apiKey: v } })}
              placeholder="Paste API key…"
              password
              monospace
              accent={t.accent}
            />
            <Button accent={t.accent} icon={<IconKey size={13} />}>Get key</Button>
          </div>
        </SubField>
      </SettingsCard>
    </SectionStack>
  </>
);

/* ============ SOURCES ============ */
const SourcesSection = ({ s, set, t }) => {
  const [newUrl, setNewUrl] = React.useState("");
  const [selected, setSelected] = React.useState(null);

  const add = () => {
    const v = newUrl.trim();
    if (!v) return;
    if (s.sources.includes(v)) return;
    set({ sources: [...s.sources, v] });
    setNewUrl("");
  };
  const remove = (url) => {
    set({ sources: s.sources.filter(u => u !== url) });
    if (selected === url) setSelected(null);
  };

  return (
    <>
      <SectionHeader
        icon={<IconSources size={18} />}
        title="Download sources"
        subtitle="Hydra-compatible JSON source URLs used by the Browse Games window."
      />
      <SectionStack>
        <div style={{
          background: "rgba(255,255,255,0.024)",
          border: "1px solid rgba(255,255,255,0.06)",
          borderRadius: 6,
          padding: 12,
        }}>
          <div style={{ display: "flex", gap: 6, marginBottom: 10 }}>
            <TextField
              value={newUrl}
              onChange={setNewUrl}
              placeholder="https://example.com/source.json"
              monospace
              accent={t.accent}
            />
            <Button variant="primary" accent={t.accent} icon={<IconPlus size={13} />} onClick={add}>
              Add
            </Button>
          </div>

          <div style={{
            border: "1px solid rgba(255,255,255,0.05)",
            borderRadius: 4,
            background: "rgba(0,0,0,0.2)",
            maxHeight: 220,
            overflow: "auto",
          }}>
            {s.sources.length === 0 && (
              <div style={{
                padding: "24px 16px",
                fontSize: 12,
                color: "rgba(255,255,255,0.45)",
                textAlign: "center",
              }}>
                No sources configured. Add a Hydra JSON URL to get started.
              </div>
            )}
            {s.sources.map((url, i) => (
              <div
                key={url}
                onClick={() => setSelected(url)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                  padding: "8px 12px",
                  cursor: "pointer",
                  background: selected === url
                    ? `${t.accent}1f`
                    : (i % 2 ? "rgba(255,255,255,0.014)" : "transparent"),
                  borderLeft: selected === url ? `2px solid ${t.accent}` : "2px solid transparent",
                  fontSize: 12,
                  fontFamily: `"JetBrains Mono","Cascadia Code",ui-monospace,monospace`,
                  color: "rgba(255,255,255,0.85)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                <span style={{
                  width: 6, height: 6, borderRadius: 3,
                  background: "#7ee2a4", flexShrink: 0,
                }}/>
                <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis" }}>{url}</span>
                <button
                  onClick={(e) => { e.stopPropagation(); remove(url); }}
                  style={{
                    background: "transparent",
                    border: "none",
                    color: "rgba(255,255,255,0.5)",
                    cursor: "pointer",
                    padding: 4,
                    display: "flex",
                    borderRadius: 3,
                  }}
                  onMouseEnter={(e) => e.currentTarget.style.color = "#ff8a8a"}
                  onMouseLeave={(e) => e.currentTarget.style.color = "rgba(255,255,255,0.5)"}
                  title="Remove source"
                >
                  <IconTrash size={13} />
                </button>
              </div>
            ))}
          </div>

          <div style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            marginTop: 10,
            fontSize: 11,
            color: "rgba(255,255,255,0.45)",
          }}>
            <span>{s.sources.length} source{s.sources.length === 1 ? "" : "s"} configured</span>
            <a href="#" style={{ color: t.accent, textDecoration: "none" }}>Browse community sources →</a>
          </div>
        </div>
      </SectionStack>
    </>
  );
};

/* ============ LAN ============ */
const LanSection = ({ s, set, t }) => (
  <>
    <SectionHeader
      icon={<IconLan size={18} />}
      title="LAN sharing"
      subtitle="Share installed games with other devices on your local network."
    />
    <SectionStack>
      <SettingsCard
        icon={<IconLan size={16} />}
        title="LAN game transfer"
        description="Other devices on the same network can browse and pull installs from this machine."
        showIcons={t.showIcons}
        density={t.density}
        accent={t.accent}
        expandable
        expanded={s.lan.enabled}
        onToggleExpand={() => set({ lan: { ...s.lan, enabled: !s.lan.enabled } })}
        control={
          <ToggleSwitch
            checked={s.lan.enabled}
            onChange={(v) => set({ lan: { ...s.lan, enabled: v } })}
            accent={t.accent}
          />
        }
        status={s.lan.enabled
          ? { kind: "ok", text: `Listening :${s.lan.port}` }
          : null}
      >
        <SubField label="Port" helper="TCP port that peers connect to. Default 47632.">
          <TextField
            value={s.lan.port}
            onChange={(v) => set({ lan: { ...s.lan, port: v } })}
            placeholder="47632"
            monospace
            accent={t.accent}
            style={{ width: 140 }}
          />
        </SubField>
        <SubField label="Default install dir" helper="Where downloads from peers land by default.">
          <div style={{ display: "flex", gap: 6 }}>
            <TextField
              value={s.lan.installDir}
              onChange={(v) => set({ lan: { ...s.lan, installDir: v } })}
              placeholder="Where downloads go by default…"
              readOnly
              monospace
              accent={t.accent}
            />
            <Button accent={t.accent}>Browse</Button>
          </div>
        </SubField>
      </SettingsCard>
    </SectionStack>
  </>
);

/* ============ SYNC SERVER ============ */
const SyncSection = ({ s, set, t }) => {
  const [showRegister, setShowRegister] = React.useState(false);
  return (
    <>
      <SectionHeader
        icon={<IconSync size={18} />}
        title="Cloud sync"
        subtitle="Prevent launching a game that's already running on another device."
      />
      <SectionStack>
        <SettingsCard
          icon={<IconSync size={16} />}
          title="Sync server"
          description="Coordinate game session locks across all your devices."
          showIcons={t.showIcons}
          density={t.density}
          accent={t.accent}
          expandable
          expanded={s.sync.enabled}
          onToggleExpand={() => set({ sync: { ...s.sync, enabled: !s.sync.enabled } })}
          control={
            <ToggleSwitch
              checked={s.sync.enabled}
              onChange={(v) => set({ sync: { ...s.sync, enabled: v } })}
              accent={t.accent}
            />
          }
          status={s.sync.enabled && s.sync.serverUrl
            ? { kind: "ok", text: `v3.8.2 · connected` }
            : null}
        >
          <SubField label="Server URL" helper="HTTP endpoint of your sync server (LAN or remote).">
            <div style={{ display: "flex", gap: 6 }}>
              <TextField
                value={s.sync.serverUrl}
                onChange={(v) => set({ sync: { ...s.sync, serverUrl: v } })}
                placeholder="http://raspberrypi.local:47633"
                monospace
                accent={t.accent}
                prefix={<IconWifi size={13} />}
              />
              <Button accent={t.accent}>Scan LAN</Button>
            </div>
          </SubField>
          <SubField label="API key" helper={
            s.sync.serverUrl ? "Server v3.8.2 — up to date." : "Register a device to obtain an API key."
          }>
            <div style={{ display: "flex", gap: 6 }}>
              <TextField
                value={s.sync.apiKey}
                onChange={(v) => set({ sync: { ...s.sync, apiKey: v } })}
                placeholder="•••••••••••••••••••••••"
                password
                monospace
                accent={t.accent}
              />
              <Button
                accent={t.accent}
                onClick={() => setShowRegister(!showRegister)}
              >
                Register…
              </Button>
            </div>
          </SubField>
          <SubField label="Device name" hint="Shown to other devices">
            <TextField
              value={s.sync.deviceName}
              onChange={(v) => set({ sync: { ...s.sync, deviceName: v } })}
              placeholder="PC name shown to other devices"
              accent={t.accent}
            />
          </SubField>

          {showRegister && (
            <div style={{
              marginTop: 6,
              padding: 12,
              background: "rgba(76,194,255,0.05)",
              border: `1px solid ${t.accent}33`,
              borderRadius: 4,
            }}>
              <div style={{ fontSize: 12, fontWeight: 500, marginBottom: 10, color: t.accent }}>
                Register a new device with this server
              </div>
              <SubField label="Admin secret">
                <TextField password monospace accent={t.accent} placeholder="From server config" />
              </SubField>
              <SubField label="Username">
                <TextField accent={t.accent} placeholder="alex" />
              </SubField>
              <div style={{ display: "flex", justifyContent: "flex-end", gap: 6, marginTop: 6 }}>
                <Button variant="ghost" accent={t.accent} onClick={() => setShowRegister(false)}>
                  Cancel
                </Button>
                <Button variant="primary" accent={t.accent}>Register</Button>
              </div>
            </div>
          )}
        </SettingsCard>
      </SectionStack>
    </>
  );
};

/* ============ DOWNLOADS / TORBOX ============ */
const DownloadsSection = ({ s, set, t }) => (
  <>
    <SectionHeader
      icon={<IconDownload size={18} />}
      title="Downloads"
      subtitle="External download providers for fetching game files."
    />
    <SectionStack>
      <SettingsCard
        icon={<IconDownload size={16} />}
        title="TorBox"
        description="Download game files via the TorBox debrid service."
        showIcons={t.showIcons}
        density={t.density}
        accent={t.accent}
        expandable
        expanded={s.torbox.enabled}
        onToggleExpand={() => set({ torbox: { ...s.torbox, enabled: !s.torbox.enabled } })}
        control={
          <ToggleSwitch
            checked={s.torbox.enabled}
            onChange={(v) => set({ torbox: { ...s.torbox, enabled: v } })}
            accent={t.accent}
          />
        }
        status={s.torbox.enabled && s.torbox.apiKey ? { kind: "ok", text: "Linked" } : null}
      >
        <SubField label="API key" helper="Required to authorise download requests.">
          <div style={{ display: "flex", gap: 6 }}>
            <TextField
              value={s.torbox.apiKey}
              onChange={(v) => set({ torbox: { ...s.torbox, apiKey: v } })}
              placeholder="Paste TorBox API key…"
              password
              monospace
              accent={t.accent}
            />
            <Button accent={t.accent} icon={<IconKey size={13} />}>Get key</Button>
          </div>
        </SubField>
        <SubField label="Download to" helper="Where TorBox-fetched files land.">
          <div style={{ display: "flex", gap: 6 }}>
            <TextField
              value={s.torbox.downloadDir}
              onChange={(v) => set({ torbox: { ...s.torbox, downloadDir: v } })}
              placeholder="Default: ~/Downloads"
              readOnly
              monospace
              accent={t.accent}
            />
            <Button accent={t.accent}>Browse</Button>
          </div>
        </SubField>
      </SettingsCard>
    </SectionStack>
  </>
);

Object.assign(window, {
  GeneralSection, ArtworkSection, SourcesSection, LanSection, SyncSection, DownloadsSection,
});
