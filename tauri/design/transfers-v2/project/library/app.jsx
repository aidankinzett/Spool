/* Library app — window frame + title bar + 2-pane split. */

const { useState: useStateL, useMemo: useMemoL } = React;

const TWEAK_DEFAULTS_LIB = /*EDITMODE-BEGIN*/{
  "accent": "#4cc2ff",
  "density": "comfortable",
  "backdrop": "mica",
  "heroHeight": 300
}/*EDITMODE-END*/;

function App() {
  const [tweaks, setTweak] = useTweaks(TWEAK_DEFAULTS_LIB);
  // Pre-select something engaging on first load; fall back to first game if
  // the chosen id ever drifts out of the data file.
  const [activeId, setActiveId] = useStateL(() => {
    const preferred = "elden-ring-nightreign";
    return (window.LIBRARY.find(g => g.id === preferred) || window.LIBRARY[0]).id;
  });
  const [query, setQuery] = useStateL("");
  const [filter, setFilter] = useStateL("all");
  const [sort, setSort] = useStateL("recent");

  const games = window.LIBRARY;

  // Per-game persistent overrides (Run as Administrator, etc.).
  // Keyed by game id so toggles survive switching the selection.
  const [overrides, setOverrides] = useStateL({});
  const updateGame = (id, patch) =>
    setOverrides((o) => ({ ...o, [id]: { ...(o[id] || {}), ...patch } }));

  // Merge data + overrides for any view that reads game state.
  const liveGames = useMemoL(
    () => games.map((g) => ({ ...g, ...(overrides[g.id] || {}) })),
    [games, overrides]
  );

  // Filter + search + sort pipeline
  const visibleGames = useMemoL(() => {
    const now = new Date("2026-05-25T12:00:00").getTime();
    let out = liveGames.slice();

    // Filter
    if (filter === "recent") {
      out = out.filter(g => g.lastPlayed
        && (now - new Date(g.lastPlayed).getTime()) < 14 * 86400 * 1000);
    } else if (filter === "shared") {
      out = out.filter(g => g.lanShared);
    } else if (filter === "unplayed") {
      out = out.filter(g => !g.lastPlayed || g.playtime < 60);
    }

    // Search (name, developer, publisher, genres)
    if (query.trim()) {
      const q = query.toLowerCase();
      out = out.filter(g =>
        g.name.toLowerCase().includes(q)
        || g.developer.toLowerCase().includes(q)
        || g.publisher.toLowerCase().includes(q)
        || g.genres.some(x => x.toLowerCase().includes(q))
      );
    }

    // Sort
    out.sort((a, b) => {
      switch (sort) {
        case "name":
          return a.name.localeCompare(b.name);
        case "added":
          return new Date(b.addedAt) - new Date(a.addedAt);
        case "playtime":
          return b.playtime - a.playtime;
        case "size":
          return b.installSize - a.installSize;
        case "recent":
        default: {
          const ta = a.lastPlayed ? new Date(a.lastPlayed).getTime() : 0;
          const tb = b.lastPlayed ? new Date(b.lastPlayed).getTime() : 0;
          return tb - ta;
        }
      }
    });
    return out;
  }, [liveGames, query, filter, sort]);

  const activeGame = useMemoL(() => {
    return liveGames.find(g => g.id === activeId) || null;
  }, [liveGames, activeId]);

  // If the active game was filtered out, keep it shown — selection is sticky.
  // Empty state only if explicitly cleared.

  return (
    <>
      <WindowFrameL backdrop={tweaks.backdrop} accent={tweaks.accent}>
        <TitleBarL accent={tweaks.accent} peers={3} />

        <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
          <LibrarySidebar
            games={visibleGames}
            activeId={activeId}
            onSelect={setActiveId}
            accent={tweaks.accent}
            density={tweaks.density}
            query={query}
            setQuery={setQuery}
            filter={filter}
            setFilter={setFilter}
            sort={sort}
            setSort={setSort}
          />
          {activeGame ? (
            <GameDetail
              game={activeGame}
              accent={tweaks.accent}
              onUpdate={(patch) => updateGame(activeGame.id, patch)}
            />
          ) : (
            <EmptyState games={liveGames} accent={tweaks.accent} />
          )}
        </div>
      </WindowFrameL>

      <TweaksPanel title="Tweaks">
        <TweakSection label="Library">
          <TweakRadio
            label="Density"
            value={tweaks.density}
            options={[
              { value: "comfortable", label: "Comfy" },
              { value: "compact", label: "Compact" },
            ]}
            onChange={(v) => setTweak("density", v)}
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
        <TweakSection label="Navigation">
          <TweakButton
            label="Selection"
            onClick={() => setActiveId(null)}
          >
            Clear (show overview)
          </TweakButton>
        </TweakSection>
      </TweaksPanel>
    </>
  );
}

/* ---------- Window frame (matches Settings.html, slightly larger) ---------- */
function WindowFrameL({ children, backdrop, accent }) {
  const bg = {
    mica: "linear-gradient(180deg, rgba(28,28,28,0.88) 0%, rgba(22,22,22,0.94) 100%)",
    acrylic: "rgba(36,36,36,0.72)",
    solid: "#1a1a1a",
  }[backdrop];

  return (
    <div style={{
      position: "relative",
      width: 1240,
      height: 800,
      background: bg,
      backdropFilter: backdrop === "acrylic" ? "blur(40px) saturate(140%)" : "blur(20px)",
      borderRadius: 8,
      border: "1px solid rgba(255,255,255,0.06)",
      boxShadow: "0 20px 60px rgba(0,0,0,0.55), 0 4px 16px rgba(0,0,0,0.4)",
      display: "flex",
      flexDirection: "column",
      overflow: "hidden",
    }}>
      {backdrop !== "solid" && (
        <div style={{
          position: "absolute",
          top: -300,
          left: -200,
          width: 700,
          height: 700,
          background: `radial-gradient(circle, ${accent}0e, transparent 60%)`,
          pointerEvents: "none",
        }}/>
      )}
      {children}
    </div>
  );
}

/* ---------- Title bar — Spool branding, peers indicator, profile/Settings ---------- */
function TitleBarL({ accent, peers }) {
  return (
    <div style={{
      height: 40,
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "0 0 0 16px",
      flexShrink: 0,
      position: "relative",
      zIndex: 2,
      borderBottom: "1px solid rgba(255,255,255,0.04)",
      WebkitUserSelect: "none",
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <SpoolMark size={18} fg="rgba(255,255,255,0.92)" />
        <SpoolWordmark size={13} fg="rgba(255,255,255,0.92)" />
      </div>

      <div style={{
        position: "absolute",
        left: "50%",
        top: "50%",
        transform: "translate(-50%, -50%)",
        display: "flex",
        alignItems: "center",
        gap: 8,
        fontSize: 11,
        color: "rgba(255,255,255,0.55)",
      }}>
        <span style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 6,
          padding: "3px 10px",
          borderRadius: 10,
          background: "rgba(255,255,255,0.04)",
          border: "1px solid rgba(255,255,255,0.06)",
        }}>
          <span style={{
            width: 6, height: 6, borderRadius: 3,
            background: peers > 0 ? "#7ee2a4" : "rgba(255,255,255,0.3)",
          }}/>
          <IconWifi size={11} />
          {peers} {peers === 1 ? "peer" : "peers"} on LAN
        </span>
      </div>

      <div style={{ display: "flex", alignItems: "center" }}>
        <TitleSettingsBtn />
        <TitleBarBtn><IconMinimize size={12} /></TitleBarBtn>
        <TitleBarBtn><IconMaximize size={12} /></TitleBarBtn>
        <TitleBarBtn danger><IconClose size={12} /></TitleBarBtn>
      </div>
    </div>
  );
}

function TitleSettingsBtn() {
  const [hover, setHover] = useStateL(false);
  return (
    <a
      href="Settings.html"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      title="Settings"
      style={{
        width: 38,
        height: 40,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        background: hover ? "rgba(255,255,255,0.06)" : "transparent",
        color: "rgba(255,255,255,0.7)",
        textDecoration: "none",
        transition: "background 100ms ease",
      }}
    >
      <IconGeneral size={14} />
    </a>
  );
}

function TitleBarBtn({ children, danger }) {
  const [hover, setHover] = useStateL(false);
  return (
    <button
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        width: 46,
        height: 40,
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

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
