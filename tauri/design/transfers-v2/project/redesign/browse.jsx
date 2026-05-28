/* Spool · Browse Games (Hydra-style source aggregator) */

const BROWSE_FEEDS = [
  { id: "elamigos", name: "elamigos.json",     count: 1842, color: "#ff9a4f" },
  { id: "repacks",  name: "repacks.json",      count:  978, color: "#bf6cf5" },
  { id: "lan",      name: "lan-sources.json",  count:  124, color: "#7ee2a4", note: "self-hosted" },
];

const BROWSE_GAMES = [
  { title: "Elden Ring: Nightreign",  releases: 4, top: { repacker: "FitGirl",  size: 38_400, date: "2026-05-21" }, sources: ["elamigos","repacks"], inLibrary: true, art: { from: "#3c1a0d", to: "#080304", accent: "#e8a444", mood: "Ember" } },
  { title: "Clair Obscur: Expedition 33", releases: 3, top: { repacker: "DODI",     size: 32_100, date: "2026-05-19" }, sources: ["elamigos"], art: { from: "#1a2333", to: "#040608", accent: "#a3c2ff", mood: "Wandering" } },
  { title: "Avowed",                   releases: 5, top: { repacker: "FitGirl",  size: 42_300, date: "2026-05-12" }, sources: ["elamigos","repacks"], art: { from: "#1d2e1a", to: "#050805", accent: "#8fd47a", mood: "Verdant" } },
  { title: "Indiana Jones and the Great Circle", releases: 2, top: { repacker: "EMPRESS", size: 88_200, date: "2026-05-08" }, sources: ["elamigos"], art: { from: "#2e1c0d", to: "#080503", accent: "#e8b86e", mood: "Sunset" } },
  { title: "Path of Exile 2",          releases: 6, top: { repacker: "FitGirl",  size: 62_400, date: "2026-05-04" }, sources: ["repacks","lan"], art: { from: "#1a1422", to: "#060309", accent: "#c97aff", mood: "Sigil" } },
  { title: "Half-Life: Alyx",          releases: 1, top: { repacker: "DODI",     size: 48_100, date: "2026-05-02" }, sources: ["elamigos"], art: { from: "#0e1f2e", to: "#020608", accent: "#7ec6ff", mood: "City 17" } },
  { title: "Black Myth: Wukong",       releases: 4, top: { repacker: "FitGirl",  size: 95_000, date: "2026-04-28" }, sources: ["elamigos","repacks","lan"], art: { from: "#33180b", to: "#080302", accent: "#f4a85a", mood: "Mountain" } },
  { title: "Spider-Man 2",             releases: 3, top: { repacker: "EMPRESS", size: 132_000, date: "2026-04-20" }, sources: ["elamigos"], art: { from: "#0a1230", to: "#020414", accent: "#ff7a7a", mood: "Skyline" } },
  { title: "Stalker 2: Heart of Chornobyl", releases: 2, top: { repacker: "DODI", size: 142_000, date: "2026-04-12" }, sources: ["elamigos","repacks"], art: { from: "#1f2b1a", to: "#040605", accent: "#c2c97a", mood: "Zone" } },
  { title: "Persona 6",                releases: 5, top: { repacker: "FitGirl",  size: 38_900, date: "2026-04-08" }, sources: ["elamigos","repacks"], art: { from: "#220b2e", to: "#050309", accent: "#ff9ed6", mood: "Velvet" } },
  { title: "Ghost of Yōtei",           releases: 1, top: { repacker: "DODI",     size: 72_400, date: "2026-04-04" }, sources: ["elamigos"], art: { from: "#1d1411", to: "#070403", accent: "#e8b366", mood: "Maple" } },
  { title: "Fields of Mistria",        releases: 2, top: { repacker: "GOG",      size:    800, date: "2026-03-28" }, sources: ["lan"], art: { from: "#1c2e1a", to: "#040605", accent: "#f4d35e", mood: "Pasture" } },
];

function BrowseGamesWindow({ width = 1280, height = 800 }) {
  const [feed, setFeed] = React.useState("all");
  const [picked, setPicked] = React.useState("Elden Ring: Nightreign");
  const game = BROWSE_GAMES.find(g => g.title === picked) || BROWSE_GAMES[0];
  const list = feed === "all" ? BROWSE_GAMES : BROWSE_GAMES.filter(g => g.sources.includes(feed));

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      display: "flex", flexDirection: "column",
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
    }}>
      <BrowseChrome />
      <div style={{ flex: 1, display: "grid", gridTemplateColumns: "240px 1fr 380px", minHeight: 0 }}>
        <BrowseSidebar feed={feed} setFeed={setFeed} />
        <BrowseList list={list} picked={picked} setPicked={setPicked} />
        <BrowseDetail game={game} />
      </div>
    </div>
  );
}

function BrowseChrome() {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 12,
      height: TOK.d.desktop.titleBar,
      padding: "0 8px 0 14px",
      background: "rgba(0,0,0,0.32)",
      borderBottom: `1px solid ${TOK.c.line}`,
    }}>
      <SpoolMark size={18} color={TOK.c.ink1} tape={TOK.c.spool} />
      <MonoLabel size={10.5}>SPOOL</MonoLabel>
      <span style={{ color: TOK.c.ink3, fontSize: 10 }}>/</span>
      <MonoLabel size={10.5} color={TOK.c.ink1}>BROWSE · SOURCES</MonoLabel>
      <span style={{ color: TOK.c.ink3, fontSize: 10 }}>·</span>
      <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.06em" }}>
        {BROWSE_FEEDS.reduce((a, b) => a + b.count, 0).toLocaleString()} ENTRIES
      </span>
      <div style={{ flex: 1 }} />
      <ChromeIcon icon={ICN.cog} title="Manage feeds" />
      <div style={{ width: 6 }} />
      <ChromeBtn glyph="min" />
      <ChromeBtn glyph="max" />
      <ChromeBtn glyph="close" />
    </div>
  );
}

function BrowseSidebar({ feed, setFeed }) {
  return (
    <aside style={{
      borderRight: `1px solid ${TOK.c.line}`,
      background: TOK.c.bg1,
      padding: "16px 0",
      display: "flex", flexDirection: "column", gap: 20,
      overflowY: "auto",
    }}>
      <div style={{ padding: "0 14px" }}>
        <MonoLabel size={10}>Sources</MonoLabel>
        <div style={{ marginTop: 8, display: "flex", flexDirection: "column" }}>
          <FeedRow id="all" name="All feeds" count={BROWSE_FEEDS.reduce((a, b) => a + b.count, 0)} color={TOK.c.spool} active={feed === "all"} onClick={() => setFeed("all")} />
          {BROWSE_FEEDS.map(f => (
            <FeedRow key={f.id} {...f} active={feed === f.id} onClick={() => setFeed(f.id)} />
          ))}
        </div>
      </div>

      <div style={{ padding: "0 14px" }}>
        <MonoLabel size={10}>Filters</MonoLabel>
        <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 12 }}>
          <FilterSection label="Repacker" items={[
            { l: "FitGirl",       n: 612, on: true },
            { l: "DODI",          n: 314 },
            { l: "EMPRESS",       n:  88 },
            { l: "GOG / Vanilla", n: 412 },
          ]} />
          <FilterSection label="Size" items={[
            { l: "< 5 GB",   n: 401 },
            { l: "5–25 GB",  n: 1102 },
            { l: "25–80 GB", n: 1014 },
            { l: "80+ GB",   n: 227 },
          ]} />
          <FilterSection label="Posted" items={[
            { l: "Last 24h",   n: 18 },
            { l: "Last week",  n: 142 },
            { l: "Last month", n: 612 },
          ]} />
        </div>
      </div>
    </aside>
  );
}

function FeedRow({ id, name, count, color, active, onClick, note }) {
  return (
    <button onClick={onClick} style={{
      display: "flex", alignItems: "center", gap: 8,
      padding: "7px 8px",
      background: active ? TOK.c.bg3 : "transparent",
      borderLeft: `2px solid ${active ? color : "transparent"}`,
      border: "none", cursor: "pointer",
      width: "100%", textAlign: "left",
      color: active ? TOK.c.ink0 : TOK.c.ink1,
    }}>
      <span style={{ width: 6, height: 6, borderRadius: 3, background: color, flexShrink: 0 }} />
      <span style={{
        flex: 1, minWidth: 0,
        fontFamily: id === "all" ? TOK.font.ui : TOK.font.mono,
        fontSize: id === "all" ? 13 : 11.5,
        fontWeight: active ? 500 : 400,
        whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
      }}>{name}</span>
      {note && <MonoLabel size={8.5} color={TOK.c.ink3}>{note}</MonoLabel>}
      <span style={{
        fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.04em",
      }}>{count.toLocaleString()}</span>
    </button>
  );
}

function FilterSection({ label, items }) {
  return (
    <div>
      <div style={{
        fontFamily: TOK.font.mono, fontSize: 9, color: TOK.c.ink3, letterSpacing: "0.1em",
        textTransform: "uppercase", marginBottom: 6,
      }}>{label}</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        {items.map(i => (
          <label key={i.l} style={{
            display: "flex", alignItems: "center", gap: 8,
            padding: "3px 8px",
            background: i.on ? TOK.c.bg3 : "transparent",
            borderRadius: TOK.r.sm, cursor: "pointer",
            fontSize: 11.5, color: i.on ? TOK.c.ink0 : TOK.c.ink1,
          }}>
            <span style={{
              width: 12, height: 12, borderRadius: 2,
              background: i.on ? TOK.c.spool : "transparent",
              border: `1.4px solid ${i.on ? TOK.c.spool : TOK.c.line3}`,
              display: "inline-flex", alignItems: "center", justifyContent: "center", flexShrink: 0,
            }}>
              {i.on && <svg width="8" height="8" viewBox="0 0 8 8"><path d="M1.5 4.2 3 5.8 6.5 2" fill="none" stroke={TOK.c.bg0} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" /></svg>}
            </span>
            <span style={{ flex: 1 }}>{i.l}</span>
            <span style={{ fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3 }}>{i.n}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

function BrowseList({ list, picked, setPicked }) {
  const [sort, setSort] = React.useState("posted");
  return (
    <div style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div style={{
        padding: "10px 14px",
        borderBottom: `1px solid ${TOK.c.line}`,
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 8,
          flex: 1, height: 30, padding: "0 10px",
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
        }}>
          <span style={{ color: TOK.c.ink2, display: "flex" }}>{ICN.search}</span>
          <input
            placeholder={`Search ${BROWSE_GAMES.length.toLocaleString()} titles across 3 feeds…`}
            style={{
              flex: 1, background: "transparent", border: "none", outline: "none",
              color: TOK.c.ink0, fontFamily: TOK.font.ui, fontSize: 12.5,
            }}
          />
        </div>
        <div style={{
          display: "inline-flex",
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
          padding: 2,
        }}>
          {[{ id: "posted", l: "Newest" }, { id: "size", l: "Size" }, { id: "az", l: "A–Z" }].map(o => (
            <button key={o.id} onClick={() => setSort(o.id)} style={{
              padding: "4px 10px", height: 22,
              background: sort === o.id ? TOK.c.bg3 : "transparent",
              color: sort === o.id ? TOK.c.ink0 : TOK.c.ink2,
              border: "none", borderRadius: 2,
              fontFamily: TOK.font.ui, fontSize: 11, fontWeight: 500, cursor: "pointer",
            }}>{o.l}</button>
          ))}
        </div>
      </div>

      <div style={{
        padding: "8px 14px",
        display: "grid", gridTemplateColumns: "1fr 96px 88px 80px", gap: 10,
        alignItems: "center",
        borderBottom: `1px solid ${TOK.c.line}`,
        background: TOK.c.bg1,
      }}>
        <MonoLabel size={9}>Title</MonoLabel>
        <MonoLabel size={9}>Releases</MonoLabel>
        <MonoLabel size={9}>Top size</MonoLabel>
        <MonoLabel size={9}>Posted</MonoLabel>
      </div>

      <div style={{ flex: 1, overflowY: "auto" }}>
        {list.map(g => (
          <BrowseListRow key={g.title} game={g} picked={picked === g.title} onClick={() => setPicked(g.title)} />
        ))}
      </div>
    </div>
  );
}

function BrowseListRow({ game, picked, onClick }) {
  return (
    <button
      onClick={onClick}
      style={{
        display: "grid", gridTemplateColumns: "1fr 96px 88px 80px",
        gap: 10, alignItems: "center",
        width: "100%", padding: "9px 14px",
        background: picked
          ? `linear-gradient(90deg, ${game.art.accent}18, ${game.art.accent}06)`
          : "transparent",
        borderLeft: `2px solid ${picked ? game.art.accent : "transparent"}`,
        border: "none", borderBottom: `1px dashed ${TOK.c.line}`,
        cursor: "pointer", textAlign: "left", color: "inherit",
        fontFamily: TOK.font.ui,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10, minWidth: 0 }}>
        <Cover game={{ short: game.title.split(":")[0], art: game.art }} w={28} h={40} sleeve={false} label={false} />
        <div style={{ minWidth: 0 }}>
          <div style={{
            fontSize: 12.5, fontWeight: picked ? 500 : 400,
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
            display: "flex", alignItems: "center", gap: 6,
          }}>
            {game.title}
            {game.inLibrary && <Pill kind="ok">In library</Pill>}
          </div>
          <div style={{ display: "flex", gap: 8, marginTop: 3 }}>
            {game.sources.map(s => {
              const f = BROWSE_FEEDS.find(ff => ff.id === s);
              return f && (
                <span key={s} style={{
                  display: "inline-flex", alignItems: "center", gap: 4,
                  fontFamily: TOK.font.mono, fontSize: 9, color: TOK.c.ink2, letterSpacing: "0.06em",
                }}>
                  <span style={{ width: 5, height: 5, borderRadius: 3, background: f.color }} />
                  {f.name.replace(".json", "")}
                </span>
              );
            })}
          </div>
        </div>
      </div>
      <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.04em" }}>
        {game.releases} releases
      </span>
      <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.04em" }}>
        {fmtSize(game.top.size)}
      </span>
      <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3, letterSpacing: "0.04em" }}>
        {relDate(game.top.date)}
      </span>
    </button>
  );
}

function BrowseDetail({ game }) {
  const acc = game.art.accent;
  const releases = [
    { repacker: "FitGirl",   size: game.top.size,        version: "v1.04",  lang: "MULTi13", date: "2026-05-21", health: "fast", best: true },
    { repacker: "DODI",      size: game.top.size * 1.08, version: "v1.04",  lang: "MULTi8",  date: "2026-05-21", health: "fast" },
    { repacker: "EMPRESS",   size: game.top.size * 0.96, version: "v1.03",  lang: "EN/JP",   date: "2026-05-20", health: "med" },
    { repacker: "Vanilla",   size: game.top.size * 1.45, version: "v1.04",  lang: "MULTi",   date: "2026-05-20", health: "slow", note: "GOG" },
  ];

  return (
    <aside style={{
      borderLeft: `1px solid ${TOK.c.line}`,
      background: TOK.c.bg1,
      display: "flex", flexDirection: "column", minHeight: 0,
    }}>
      <div style={{
        position: "relative", height: 200,
        background: `linear-gradient(135deg, ${game.art.from} 0%, ${game.art.to} 100%)`,
        overflow: "hidden", flexShrink: 0,
      }}>
        <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: 3, background: acc }} />
        <div style={{
          position: "absolute", right: "-30%", top: "-20%",
          width: 280, height: 280, borderRadius: "50%",
          background: `radial-gradient(circle at 30% 30%, ${acc}55, transparent 60%)`,
        }} />
        <div style={{
          position: "absolute", inset: 0,
          background: `linear-gradient(180deg, transparent 40%, ${TOK.c.bg1} 100%)`,
        }} />
        <div style={{ position: "absolute", left: 18, right: 18, bottom: 14 }}>
          <MonoLabel size={9.5} color={acc}>SIDE A · {(game.art.mood || "").toUpperCase()}</MonoLabel>
          <div style={{
            marginTop: 5,
            fontFamily: TOK.font.display, fontSize: 22, fontWeight: 700,
            letterSpacing: "-0.018em", lineHeight: 1.06,
            textShadow: "0 2px 14px rgba(0,0,0,0.5)", textWrap: "balance",
          }}>{game.title}</div>
        </div>
      </div>

      <div style={{
        padding: "12px 18px",
        borderBottom: `1px solid ${TOK.c.line}`,
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <Btn variant="primary" accent={acc} icon={ICN.download} style={{ flex: 1, height: 34, fontSize: 13 }}>
          Download · best match
        </Btn>
        <Btn icon={ICN.eye} style={{ height: 34 }} />
        <Btn icon={ICN.share} style={{ height: 34 }} />
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: "12px 18px 18px" }}>
        <MonoLabel size={9.5}>Releases · {releases.length}</MonoLabel>
        <div style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 6 }}>
          {releases.map(r => <ReleaseRow key={r.repacker} r={r} acc={acc} />)}
        </div>

        <div style={{ marginTop: 16, borderTop: `1px dashed ${TOK.c.line}`, paddingTop: 12 }}>
          <MonoLabel size={9.5}>Or pull from your LAN</MonoLabel>
          <div style={{
            marginTop: 8, padding: 10,
            background: "rgba(126,226,164,0.06)",
            border: `1px solid ${TOK.c.ok}33`, borderRadius: TOK.r.sm,
            display: "flex", alignItems: "center", gap: 10,
          }}>
            <span style={{ color: TOK.c.ok, display: "flex" }}>{ICN.wifi}</span>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12 }}>Living room · Deck has this game</div>
              <div style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em", marginTop: 2 }}>
                Pre-installed · {fmtSize(game.top.size)} · 11 ms
              </div>
            </div>
            <Btn style={{ height: 24, fontSize: 11.5 }}>Pull</Btn>
          </div>
        </div>
      </div>
    </aside>
  );
}

function ReleaseRow({ r, acc }) {
  const healthColor = r.health === "fast" ? TOK.c.ok : r.health === "med" ? TOK.c.warn : TOK.c.ink3;
  return (
    <div style={{
      padding: "10px 12px",
      background: r.best ? `${acc}10` : TOK.c.bg2,
      border: `1px solid ${r.best ? acc + "44" : TOK.c.line}`,
      borderRadius: TOK.r.sm,
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontSize: 12.5, fontWeight: 500 }}>{r.repacker}</span>
        {r.best && <MonoLabel size={9} color={acc}>BEST</MonoLabel>}
        {r.note && <Pill kind="off" soft>{r.note}</Pill>}
        <div style={{ flex: 1 }} />
        <span style={{
          display: "inline-flex", alignItems: "center", gap: 5,
          fontFamily: TOK.font.mono, fontSize: 9.5, color: healthColor, letterSpacing: "0.06em",
        }}>
          <span style={{ width: 5, height: 5, borderRadius: 3, background: healthColor, boxShadow: `0 0 6px ${healthColor}66` }} />
          {r.health.toUpperCase()}
        </span>
      </div>
      <div style={{
        marginTop: 6,
        display: "grid", gridTemplateColumns: "auto 1fr auto", gap: 8,
        fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink2, letterSpacing: "0.04em",
        alignItems: "center",
      }}>
        <span>{fmtSize(r.size)}</span>
        <span style={{ color: TOK.c.ink3 }}>{r.version} · {r.lang}</span>
        <span style={{ color: TOK.c.ink3 }}>{relDate(r.date)}</span>
      </div>
    </div>
  );
}

Object.assign(window, { BROWSE_FEEDS, BROWSE_GAMES, BrowseGamesWindow });
