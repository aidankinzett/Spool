/* Main settings window — frame + sidebar nav + content. */

const { useState: useStateApp, useMemo } = React;

const CATEGORIES = [
  { id: "general",    label: "General",       icon: IconGeneral,    Component: GeneralSection },
  { id: "artwork",    label: "Artwork",       icon: IconArtwork,    Component: ArtworkSection },
  { id: "sources",    label: "Sources",       icon: IconSources,    Component: SourcesSection },
  { id: "lan",        label: "LAN sharing",   icon: IconLan,        Component: LanSection },
  { id: "sync",       label: "Cloud sync",    icon: IconSync,       Component: SyncSection },
  { id: "downloads",  label: "Downloads",     icon: IconDownload,   Component: DownloadsSection },
];

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "layout": "sidebar",
  "accent": "#4cc2ff",
  "density": "comfortable",
  "showIcons": true,
  "backdrop": "mica"
}/*EDITMODE-END*/;

function App() {
  const [tweaks, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const [active, setActive] = useStateApp("sync");
  const [query, setQuery] = useStateApp("");
  const [dirty, setDirty] = useStateApp(true); // mock: settings have unsaved changes

  // Mock initial state matches the screenshot
  const [s, setS] = useStateApp({
    ludusaviPath: "C:\\Users\\akinz\\Tools\\ludusavi.exe",
    theme: "dark",
    sgdb: { enabled: true, apiKey: "e0bc73ed008d77a31bd106b3444f" },
    lan: { enabled: true, port: "47632", installDir: "" },
    sync: {
      enabled: true,
      serverUrl: "http://192.168.86.34:47633",
      apiKey: "supersecretkey1234567890abcdef",
      deviceName: "DESKTOP-OAA3RS6",
    },
    torbox: {
      enabled: true,
      apiKey: "tb_a1b2c3d4e5f6g7h8i9j0",
      downloadDir: "",
    },
    sources: [
      "https://davidkazumisource.com/fontekazumi.json",
      "https://wkeynhk.online/steamgg",
    ],
  });
  const set = (patch) => { setS({ ...s, ...patch }); setDirty(true); };

  const ActiveComponent = useMemo(() => {
    return CATEGORIES.find(c => c.id === active)?.Component || GeneralSection;
  }, [active]);

  const filteredCats = useMemo(() => {
    if (!query.trim()) return CATEGORIES;
    const q = query.toLowerCase();
    return CATEGORIES.filter(c => c.label.toLowerCase().includes(q));
  }, [query]);

  return (
    <>
      <WindowFrame backdrop={tweaks.backdrop} accent={tweaks.accent}>
        {/* Title bar */}
        <TitleBar accent={tweaks.accent} />

        {tweaks.layout === "sidebar" && (
          <SidebarLayout
            cats={filteredCats}
            active={active}
            onSelect={setActive}
            query={query}
            onQuery={setQuery}
            tweaks={tweaks}
          >
            <ActiveComponent s={s} set={set} t={tweaks} />
          </SidebarLayout>
        )}
        {tweaks.layout === "tabs" && (
          <TabsLayout
            cats={CATEGORIES}
            active={active}
            onSelect={setActive}
            tweaks={tweaks}
          >
            <ActiveComponent s={s} set={set} t={tweaks} />
          </TabsLayout>
        )}
        {tweaks.layout === "single" && (
          <SinglePageLayout s={s} set={set} t={tweaks} />
        )}

        {/* Action bar */}
        <ActionBar dirty={dirty} accent={tweaks.accent} onSave={() => setDirty(false)} />
      </WindowFrame>

      {/* Tweaks panel */}
      <TweaksPanel title="Tweaks">
        <TweakSection label="Layout">
          <TweakRadio
            label="Style"
            value={tweaks.layout}
            options={[
              { value: "sidebar", label: "Sidebar" },
              { value: "tabs", label: "Tabs" },
              { value: "single", label: "One page" },
            ]}
            onChange={(v) => setTweak("layout", v)}
          />
          <TweakRadio
            label="Density"
            value={tweaks.density}
            options={[
              { value: "comfortable", label: "Comfortable" },
              { value: "compact", label: "Compact" },
            ]}
            onChange={(v) => setTweak("density", v)}
          />
          <TweakToggle
            label="Show icons"
            value={tweaks.showIcons}
            onChange={(v) => setTweak("showIcons", v)}
          />
        </TweakSection>
        <TweakSection label="Appearance">
          <TweakColor
            label="Accent"
            value={tweaks.accent}
            options={["#4cc2ff", "#7c5cff", "#21d07a", "#ff8a3d", "#ff5d8f"]}
            onChange={(v) => setTweak("accent", v)}
          />
          <TweakRadio
            label="Backdrop"
            value={tweaks.backdrop}
            options={[
              { value: "mica", label: "Mica" },
              { value: "acrylic", label: "Acrylic" },
              { value: "solid", label: "Solid" },
            ]}
            onChange={(v) => setTweak("backdrop", v)}
          />
        </TweakSection>
      </TweaksPanel>
    </>
  );
}

/* ============================== Window Frame ============================== */
function WindowFrame({ children, backdrop, accent }) {
  const bg = {
    mica: "linear-gradient(180deg, rgba(32,32,32,0.86) 0%, rgba(28,28,28,0.92) 100%)",
    acrylic: "rgba(40,40,40,0.7)",
    solid: "#202020",
  }[backdrop];

  return (
    <div
      style={{
        position: "relative",
        width: 1040,
        height: 720,
        background: bg,
        backdropFilter: backdrop === "acrylic" ? "blur(40px) saturate(140%)" : "blur(20px)",
        borderRadius: 8,
        border: "1px solid rgba(255,255,255,0.06)",
        boxShadow: "0 20px 60px rgba(0,0,0,0.55), 0 4px 16px rgba(0,0,0,0.4)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      {/* Subtle top-left accent glow (Mica-ish) */}
      {backdrop !== "solid" && (
        <div style={{
          position: "absolute",
          top: -200,
          left: -200,
          width: 600,
          height: 600,
          background: `radial-gradient(circle, ${accent}11, transparent 60%)`,
          pointerEvents: "none",
        }}/>
      )}
      {children}
    </div>
  );
}

/* ============================== Title bar ============================== */
function TitleBar({ accent }) {
  return (
    <div style={{
      height: 36,
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "0 0 0 16px",
      flexShrink: 0,
      position: "relative",
      zIndex: 2,
      WebkitUserSelect: "none",
      borderBottom: "1px solid rgba(255,255,255,0.04)",
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <SpoolMark size={16} fg="rgba(255,255,255,0.92)" />
        <div style={{ fontSize: 12, color: "rgba(255,255,255,0.85)", display: "flex", alignItems: "baseline", gap: 6 }}>
          <SpoolWordmark size={13} fg="rgba(255,255,255,0.92)" />
          <span style={{ color: "rgba(255,255,255,0.45)" }}>— Settings</span>
        </div>
      </div>
      <div style={{ display: "flex", alignItems: "center" }}>
        <TitleBarButton><IconMinimize size={12} /></TitleBarButton>
        <TitleBarButton><IconMaximize size={12} /></TitleBarButton>
        <TitleBarButton danger><IconClose size={12} /></TitleBarButton>
      </div>
    </div>
  );
}

function TitleBarButton({ children, danger }) {
  const [hover, setHover] = useStateApp(false);
  return (
    <button
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        width: 46,
        height: 36,
        background: hover ? (danger ? "#c42b1c" : "rgba(255,255,255,0.06)") : "transparent",
        color: hover && danger ? "#fff" : "rgba(255,255,255,0.78)",
        border: "none",
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        transition: "background 100ms ease",
      }}
    >
      {children}
    </button>
  );
}

/* ============================== Sidebar Layout ============================== */
function SidebarLayout({ cats, active, onSelect, query, onQuery, tweaks, children }) {
  return (
    <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
      <aside style={{
        width: 240,
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        padding: "16px 12px",
        gap: 4,
        borderRight: "1px solid rgba(255,255,255,0.04)",
      }}>
        <div style={{ padding: "0 4px 12px" }}>
          <TextField
            value={query}
            onChange={onQuery}
            placeholder="Find a setting"
            accent={tweaks.accent}
            prefix={<IconSearch size={13} />}
          />
        </div>
        {cats.map((c) => {
          const isActive = c.id === active;
          const IconC = c.icon;
          return (
            <NavItem
              key={c.id}
              active={isActive}
              accent={tweaks.accent}
              onClick={() => onSelect(c.id)}
              icon={<IconC size={16} />}
              label={c.label}
            />
          );
        })}
        {cats.length === 0 && (
          <div style={{ padding: 12, fontSize: 12, color: "rgba(255,255,255,0.4)" }}>
            No matches.
          </div>
        )}
      </aside>
      <main style={{
        flex: 1,
        minWidth: 0,
        overflow: "auto",
        padding: "26px 32px 32px",
      }}>
        <div style={{ maxWidth: 720 }}>{children}</div>
      </main>
    </div>
  );
}

function NavItem({ icon, label, active, onClick, accent }) {
  const [hover, setHover] = useStateApp(false);
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        position: "relative",
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "9px 12px",
        background: active
          ? "rgba(255,255,255,0.06)"
          : hover ? "rgba(255,255,255,0.03)" : "transparent",
        border: "1px solid",
        borderColor: active ? "rgba(255,255,255,0.06)" : "transparent",
        borderRadius: 4,
        color: "rgba(255,255,255,0.92)",
        fontFamily: "inherit",
        fontSize: 13,
        fontWeight: active ? 500 : 400,
        cursor: "pointer",
        textAlign: "left",
        transition: "background 100ms ease",
      }}
    >
      {/* Active indicator pill — Windows 11 nav style */}
      {active && (
        <span style={{
          position: "absolute",
          left: 0,
          top: "50%",
          transform: "translateY(-50%)",
          width: 3,
          height: 18,
          background: accent,
          borderRadius: 2,
        }}/>
      )}
      <span style={{ color: active ? accent : "rgba(255,255,255,0.78)", display: "flex" }}>
        {icon}
      </span>
      <span>{label}</span>
    </button>
  );
}

/* ============================== Tabs Layout ============================== */
function TabsLayout({ cats, active, onSelect, tweaks, children }) {
  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div style={{
        display: "flex",
        gap: 4,
        padding: "12px 24px 0",
        borderBottom: "1px solid rgba(255,255,255,0.05)",
        overflowX: "auto",
      }}>
        {cats.map((c) => {
          const IconC = c.icon;
          const isActive = c.id === active;
          return (
            <button
              key={c.id}
              onClick={() => onSelect(c.id)}
              style={{
                position: "relative",
                display: "inline-flex",
                alignItems: "center",
                gap: 8,
                padding: "10px 14px 12px",
                background: "transparent",
                color: isActive ? "#fff" : "rgba(255,255,255,0.65)",
                border: "none",
                fontFamily: "inherit",
                fontSize: 13,
                fontWeight: isActive ? 500 : 400,
                cursor: "pointer",
                whiteSpace: "nowrap",
              }}
            >
              <IconC size={15} />
              {c.label}
              {isActive && (
                <span style={{
                  position: "absolute",
                  left: 14,
                  right: 14,
                  bottom: -1,
                  height: 2,
                  borderRadius: 1,
                  background: tweaks.accent,
                }}/>
              )}
            </button>
          );
        })}
      </div>
      <main style={{
        flex: 1,
        minHeight: 0,
        overflow: "auto",
        padding: "28px 40px 32px",
      }}>
        <div style={{ maxWidth: 760, margin: "0 auto" }}>{children}</div>
      </main>
    </div>
  );
}

/* ============================== Single Page Layout (improved 2-col) ============================== */
function SinglePageLayout({ s, set, t }) {
  return (
    <main style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "26px 32px 32px" }}>
      <div style={{
        display: "grid",
        gridTemplateColumns: "1fr 1fr",
        gap: 24,
        maxWidth: 1000,
      }}>
        <div><GeneralSection s={s} set={set} t={t} /></div>
        <div><ArtworkSection s={s} set={set} t={t} /></div>
        <div><SyncSection s={s} set={set} t={t} /></div>
        <div><LanSection s={s} set={set} t={t} /></div>
        <div><DownloadsSection s={s} set={set} t={t} /></div>
        <div><SourcesSection s={s} set={set} t={t} /></div>
      </div>
    </main>
  );
}

/* ============================== Action Bar ============================== */
function ActionBar({ dirty, accent, onSave }) {
  return (
    <div style={{
      height: 60,
      flexShrink: 0,
      borderTop: "1px solid rgba(255,255,255,0.06)",
      background: "rgba(0,0,0,0.18)",
      padding: "0 24px",
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
    }}>
      <div style={{
        fontSize: 12,
        color: dirty ? "#ffc278" : "rgba(255,255,255,0.5)",
        display: "flex",
        alignItems: "center",
        gap: 8,
      }}>
        <span style={{
          width: 6, height: 6, borderRadius: 3,
          background: dirty ? "#ffc278" : "#7ee2a4",
        }}/>
        {dirty ? "Unsaved changes" : "All changes saved"}
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <Button variant="secondary" accent={accent}>Cancel</Button>
        <Button
          variant="primary"
          accent={accent}
          onClick={onSave}
          icon={<IconCheck size={13} />}
        >
          Save &amp; Continue
        </Button>
      </div>
    </div>
  );
}

/* ============================== Mount ============================== */
ReactDOM.createRoot(document.getElementById("root")).render(<App />);
