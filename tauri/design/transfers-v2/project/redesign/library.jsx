/* Spool · Library window
   Two-pane: sidebar list + detail.
   Cassette character:
     - Catalog IDs (SPL-NNNN)
     - Tape-strip across the top of the detail hero
     - Mono labels for stats / metadata / dates
     - Cover-art accent tints the hero, CTA, focus rings
     - "Now playing" indicator uses the rotating reels */

const LIB = [
  {
    id: "lego-batman-legacy", catalog: "SPL-0042", short: "LEGO Batman",
    name: "LEGO Batman: Legacy of the Dark Knight",
    art: { from: "#1b2a44", to: "#040611", accent: "#ffd23f", mood: "Moonlit" },
    genres: ["Action", "Co-op"], dev: "Traveller's Tales", pub: "Warner Bros. Games",
    release: "2025-10-14", added: "2026-04-02",
    lastPlayed: "2026-05-23T19:42:00", playtime: 1832, sessions: 18,
    installPath: "D:\\Games\\LegoBatmanLegacy", installSize: 78.4 * 1024,
    exe: "LegoBatmanLegacy.exe",
    backup: { last: "2026-05-23T19:42:18", size: 4.2, count: 18, status: "ok" },
    lan: true, sync: "ok",
    description:
      "A four-decade journey through every era of the Caped Crusader, retold one stud at a time. Build, smash and glide across 18 hand-crafted boroughs of Gotham — solo or in drop-in co-op.",
  },
  {
    id: "elden-ring-nightreign", catalog: "SPL-0044", short: "Nightreign",
    name: "Elden Ring: Nightreign",
    art: { from: "#3c1a0d", to: "#080304", accent: "#e8a444", mood: "Ember" },
    genres: ["Action RPG", "Roguelike"], dev: "FromSoftware", pub: "Bandai Namco",
    release: "2026-02-20", added: "2026-02-22",
    lastPlayed: "2026-05-24T22:10:00", playtime: 4421, sessions: 64,
    installPath: "D:\\Games\\Nightreign", installSize: 64.1 * 1024,
    exe: "nightreign.exe",
    backup: { last: "2026-05-24T22:11:02", size: 12.8, count: 42, status: "ok" },
    lan: true, sync: "ok",
    description:
      "A standalone descent into the Lands Between under an endless eclipse. Three players, three nights, and a labyrinth of bosses that re-rolls every run.",
  },
  {
    id: "hades-2", catalog: "SPL-0031", short: "Hades II",
    name: "Hades II",
    art: { from: "#2a0d3d", to: "#0a020f", accent: "#bf6cf5", mood: "Arcane" },
    genres: ["Roguelike", "Action"], dev: "Supergiant", pub: "Supergiant",
    release: "2025-09-12", added: "2025-09-13",
    lastPlayed: "2026-05-22T01:14:00", playtime: 3210, sessions: 41,
    installPath: "D:\\Games\\Hades2", installSize: 14.6 * 1024,
    exe: "Hades2.exe",
    backup: { last: "2026-05-22T01:14:33", size: 0.8, count: 26, status: "warn" },
    lan: false, sync: "warn",
    description:
      "Battle beyond the Underworld with the immortal Princess, wielding the dark sorceries of Witchcraft to challenge the sinister Titan of Time.",
  },
  {
    id: "silksong", catalog: "SPL-0028", short: "Silksong",
    name: "Hollow Knight: Silksong",
    art: { from: "#0e2a35", to: "#020607", accent: "#e8d9b3", mood: "Haunted" },
    genres: ["Metroidvania"], dev: "Team Cherry", pub: "Team Cherry",
    release: "2025-06-04", added: "2025-06-06",
    lastPlayed: "2026-05-19T20:33:00", playtime: 980, sessions: 22,
    installPath: "D:\\Games\\Silksong", installSize: 7.2 * 1024,
    exe: "Silksong.exe",
    backup: { last: "2026-05-19T20:34:00", size: 0.4, count: 11, status: "ok" },
    lan: false, sync: "ok",
    description: "Ascend a sprawling kingdom of needle-sharp threads as Hornet, princess-protector of Hallownest.",
  },
  {
    id: "baldurs-gate-3", catalog: "SPL-0007", short: "Baldur's Gate 3",
    name: "Baldur's Gate 3",
    art: { from: "#3d1c1c", to: "#0a0303", accent: "#c4a04a", mood: "Noir" },
    genres: ["RPG", "Turn-based"], dev: "Larian", pub: "Larian",
    release: "2023-08-03", added: "2023-08-10",
    lastPlayed: "2026-04-30T18:00:00", playtime: 8943, sessions: 122,
    installPath: "D:\\Games\\BG3", installSize: 142.0 * 1024,
    exe: "bg3.exe",
    backup: { last: "2026-04-30T22:11:00", size: 188.4, count: 64, status: "ok" },
    lan: true, sync: "ok",
    description: "Gather your party and return to the Forgotten Realms in a tale of fellowship, betrayal, and the lure of absolute power.",
  },
  {
    id: "outer-wilds", catalog: "SPL-0046", short: "Outer Wilds",
    name: "Outer Wilds",
    art: { from: "#10243d", to: "#02060b", accent: "#76c8ff", mood: "Cosmic" },
    genres: ["Exploration"], dev: "Mobius Digital", pub: "Annapurna",
    release: "2019-05-28", added: "2026-05-20",
    lastPlayed: null, playtime: 0, sessions: 0,
    installPath: "D:\\Games\\OuterWilds", installSize: 6.0 * 1024,
    exe: "OuterWilds.exe",
    backup: { last: null, size: 0, count: 0, status: "off" },
    lan: false, sync: "info",
    description: "Welcome to the Outer Wilds Ventures space program. Explore a strange solar system trapped in an endless time loop.",
  },
  {
    id: "stardew", catalog: "SPL-0014", short: "Stardew Valley",
    name: "Stardew Valley",
    art: { from: "#2b3d18", to: "#070d04", accent: "#f3c850", mood: "Pastoral" },
    genres: ["Farming"], dev: "ConcernedApe", pub: "ConcernedApe",
    release: "2016-02-26", added: "2024-01-14",
    lastPlayed: "2026-05-10T13:21:00", playtime: 5402, sessions: 88,
    installPath: "D:\\Games\\Stardew", installSize: 0.5 * 1024,
    exe: "Stardew Valley.exe",
    backup: { last: "2026-05-10T13:22:00", size: 2.3, count: 88, status: "ok" },
    lan: false, sync: "ok",
    description: "You've inherited your grandfather's old farm plot. Armed with hand-me-down tools and a few coins, you set out to begin your new life.",
  },
  {
    id: "pizza-tower", catalog: "SPL-0021", short: "Pizza Tower",
    name: "Pizza Tower",
    art: { from: "#c41e1e", to: "#3d0606", accent: "#fff2a8", mood: "Neon" },
    genres: ["Platformer"], dev: "Tour de Pizza", pub: "Tour de Pizza",
    release: "2023-01-26", added: "2025-12-01",
    lastPlayed: "2026-05-21T16:42:00", playtime: 720, sessions: 14,
    installPath: "D:\\Games\\PizzaTower", installSize: 0.7 * 1024,
    exe: "PizzaTower.exe",
    backup: { last: "2026-05-21T16:42:30", size: 0.1, count: 4, status: "ok" },
    lan: true, sync: "ok",
    description: "A 2D platformer inspired by Wario Land, with an emphasis on movement, exploration, and score-attack.",
  },
];

/* ─────────────────────────── Helpers ─────────────────────────── */
function relDate(s) {
  if (!s) return "—";
  const d = new Date(s); const now = new Date("2026-05-26T12:00:00");
  const diff = (now - d) / 1000;
  if (diff < 60) return "just now";
  if (diff < 3600) return Math.round(diff / 60) + "m ago";
  if (diff < 86400) return Math.round(diff / 3600) + "h ago";
  const days = Math.round(diff / 86400);
  if (days < 7) return days + "d ago";
  if (days < 30) return Math.round(days / 7) + "w ago";
  return Math.round(days / 30) + "mo ago";
}
function absDate(s) {
  if (!s) return "—";
  const d = new Date(s);
  return d.toLocaleDateString("en-GB", { year: "numeric", month: "short", day: "numeric" });
}
function absDateTime(s) {
  if (!s) return "—";
  const d = new Date(s);
  return d.toLocaleString("en-GB", { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}
function fmtPlay(mins) {
  if (!mins) return "—";
  const h = Math.floor(mins / 60); const m = mins % 60;
  if (h === 0) return m + "m";
  if (h < 100) return h + "h " + m + "m";
  return h + "h";
}
function fmtSize(mb) {
  if (!mb) return "—";
  if (mb < 1024) return mb.toFixed(1) + " MB";
  return (mb / 1024).toFixed(1) + " GB";
}

/* ─────────────────────────── Library window ─────────────────────────── */
function LibraryWindow({ initialId, width = 1280, height = 760, nowPlayingId }) {
  const [selectedId, setSelectedId] = React.useState(initialId || "elden-ring-nightreign");
  const game = LIB.find(g => g.id === selectedId) || LIB[1];

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      display: "flex", flexDirection: "column",
      borderRadius: TOK.r.lg,
      overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
    }}>
      <LibraryChrome game={game} nowPlayingId={nowPlayingId} />
      <div style={{ flex: 1, display: "grid", gridTemplateColumns: `${TOK.d.desktop.sidebar}px 1fr`, minHeight: 0 }}>
        <LibrarySidebar selectedId={selectedId} setSelectedId={setSelectedId} nowPlayingId={nowPlayingId} />
        <LibraryDetail game={game} isNowPlaying={nowPlayingId === game.id} />
      </div>
    </div>
  );
}

/* ─────────────────────────── Chrome (custom, cross-platform) ─────────────────────────── */
function LibraryChrome({ game, nowPlayingId }) {
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
      <MonoLabel size={10.5} color={TOK.c.ink1}>LIBRARY</MonoLabel>

      {nowPlayingId && (
        <>
          <span style={{ color: TOK.c.ink3, fontSize: 10, marginLeft: 4 }}>·</span>
          <div style={{ display: "inline-flex", alignItems: "center", gap: 7 }}>
            <RotatingDot color={TOK.c.ok} />
            <MonoLabel size={10} color={TOK.c.ok}>NOW PLAYING · {LIB.find(g => g.id === nowPlayingId)?.short || ""}</MonoLabel>
          </div>
        </>
      )}

      <div style={{ flex: 1 }} />

      {/* nav destinations */}
      <ChromeIcon icon={ICN.source} title="Browse sources" />
      <ChromeIcon icon={ICN.wifi} title="LAN peers · 2 online" badge="2" />
      <ChromeIcon icon={ICN.cloud} title="Sync server" status={TOK.c.ok} />
      <ChromeIcon icon={ICN.cog} title="Settings" />
      <div style={{ width: 6 }} />
      <ChromeBtn glyph="min" />
      <ChromeBtn glyph="max" />
      <ChromeBtn glyph="close" />
    </div>
  );
}

function ChromeIcon({ icon, title, badge, status }) {
  const [hover, setHover] = React.useState(false);
  return (
    <button
      title={title}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        position: "relative",
        width: 26, height: 22, borderRadius: TOK.r.sm,
        background: hover ? "rgba(255,255,255,0.06)" : "transparent",
        border: "none", color: hover ? TOK.c.ink0 : TOK.c.ink2, cursor: "pointer",
        display: "inline-flex", alignItems: "center", justifyContent: "center",
      }}
    >
      {icon}
      {badge && (
        <span style={{
          position: "absolute", top: 1, right: 1,
          width: 11, height: 11, borderRadius: 6,
          background: TOK.c.spool, color: TOK.c.bg0,
          fontFamily: TOK.font.mono, fontSize: 8, fontWeight: 700,
          display: "inline-flex", alignItems: "center", justifyContent: "center",
        }}>{badge}</span>
      )}
      {status && (
        <span style={{
          position: "absolute", bottom: 3, right: 4,
          width: 5, height: 5, borderRadius: 3,
          background: status, boxShadow: `0 0 5px ${status}`,
        }}/>
      )}
    </button>
  );
}

function RotatingDot({ color }) {
  return (
    <>
      <style>{`@keyframes lib-spin { to { transform: rotate(360deg) } }`}</style>
      <svg width="11" height="11" viewBox="0 0 11 11" style={{ animation: "lib-spin 1.6s linear infinite" }}>
        <circle cx="5.5" cy="5.5" r="4" fill="none" stroke={color} strokeWidth="1.4" />
        <circle cx="5.5" cy="5.5" r="1.4" fill={color} />
        <line x1="5.5" y1="1.5" x2="5.5" y2="3" stroke={color} strokeWidth="1.2" />
        <line x1="5.5" y1="8" x2="5.5" y2="9.5" stroke={color} strokeWidth="1.2" />
      </svg>
    </>
  );
}

/* ─────────────────────────── Sidebar ─────────────────────────── */
function LibrarySidebar({ selectedId, setSelectedId, nowPlayingId }) {
  const [filter, setFilter] = React.useState("all");
  const [query, setQuery] = React.useState("");

  const filters = [
    { id: "all", label: "All", count: LIB.length },
    { id: "recent", label: "Recent", count: LIB.filter(g => g.lastPlayed).length },
    { id: "lan", label: "Shared", count: LIB.filter(g => g.lan).length },
    { id: "unsynced", label: "Off-sync", count: 1 },
  ];

  let list = LIB;
  if (filter === "recent") {
    list = [...LIB].filter(g => g.lastPlayed).sort((a, b) => new Date(b.lastPlayed) - new Date(a.lastPlayed));
  } else if (filter === "lan") {
    list = LIB.filter(g => g.lan);
  } else if (filter === "unsynced") {
    list = LIB.filter(g => g.sync !== "ok");
  }
  if (query) {
    const q = query.toLowerCase();
    list = list.filter(g => g.name.toLowerCase().includes(q));
  }

  return (
    <div style={{
      display: "flex", flexDirection: "column",
      borderRight: `1px solid ${TOK.c.line}`,
      background: TOK.c.bg1,
      minHeight: 0,
    }}>
      {/* Search */}
      <div style={{ padding: "12px 12px 8px", display: "flex", flexDirection: "column", gap: 10 }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 8,
          height: 30, padding: "0 10px",
          background: TOK.c.bg2,
          border: `1px solid ${TOK.c.line}`,
          borderRadius: TOK.r.sm,
        }}>
          <span style={{ color: TOK.c.ink2, display: "flex" }}>{ICN.search}</span>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`Search ${LIB.length} games`}
            style={{
              flex: 1, background: "transparent", border: "none", outline: "none",
              color: TOK.c.ink0, fontFamily: TOK.font.ui, fontSize: 12.5,
            }}
          />
          <span style={{
            fontFamily: TOK.font.mono, fontSize: 9.5,
            color: TOK.c.ink3, letterSpacing: "0.08em",
            border: `1px solid ${TOK.c.line2}`, padding: "1px 5px", borderRadius: 2,
          }}>⌘K</span>
        </div>

        {/* Filter tabs */}
        <div style={{ display: "flex", gap: 4 }}>
          {filters.map(f => (
            <button
              key={f.id}
              onClick={() => setFilter(f.id)}
              style={{
                display: "inline-flex", alignItems: "center", gap: 6,
                padding: "4px 9px",
                background: filter === f.id ? TOK.c.bg3 : "transparent",
                border: `1px solid ${filter === f.id ? TOK.c.line2 : "transparent"}`,
                borderRadius: TOK.r.sm,
                color: filter === f.id ? TOK.c.ink0 : TOK.c.ink2,
                fontFamily: TOK.font.ui, fontSize: 11.5, fontWeight: 500,
                cursor: "pointer",
              }}
            >
              {f.label}
              <span style={{
                fontFamily: TOK.font.mono, fontSize: 9.5,
                color: filter === f.id ? TOK.c.ink2 : TOK.c.ink3,
              }}>{f.count}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Section header */}
      <div style={{
        padding: "10px 14px 6px",
        display: "flex", alignItems: "center", justifyContent: "space-between",
      }}>
        <MonoLabel size={9.5}>
          {filter === "recent" ? "By last played" : "By catalog"}
        </MonoLabel>
        <span style={{ color: TOK.c.ink3, fontSize: 11 }}>{list.length}</span>
      </div>

      {/* List */}
      <div style={{ flex: 1, overflowY: "auto", paddingBottom: 8 }}>
        {list.map((g, i) => (
          <SidebarRow
            key={g.id}
            game={g}
            selected={selectedId === g.id}
            playing={nowPlayingId === g.id}
            onClick={() => setSelectedId(g.id)}
          />
        ))}
      </div>

      {/* Footer */}
      <div style={{
        padding: "8px 12px",
        borderTop: `1px solid ${TOK.c.line}`,
        display: "flex", flexDirection: "column", gap: 6,
        background: TOK.c.bg0,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <Btn variant="primary" accent={TOK.c.spool} icon={ICN.plus} style={{ flex: 1 }}>Add a game</Btn>
          <Btn icon={ICN.filter} style={{ paddingLeft: 8, paddingRight: 8 }} />
        </div>
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "0 2px",
          fontSize: 10.5, color: TOK.c.ink3,
        }}>
          <a style={{ display: "inline-flex", alignItems: "center", gap: 5, color: TOK.c.ink2, cursor: "pointer" }}>
            <span style={{ display: "flex", color: TOK.c.ink2 }}>{ICN.source}</span>
            Browse sources
          </a>
          <a style={{ display: "inline-flex", alignItems: "center", gap: 5, color: TOK.c.ink2, cursor: "pointer" }}>
            <span style={{ display: "flex", color: TOK.c.ink2 }}>{ICN.wifi}</span>
            LAN · <span style={{ color: TOK.c.ok }}>2 peers</span>
          </a>
        </div>
      </div>
    </div>
  );
}

function SidebarRow({ game, selected, playing, onClick }) {
  const [hover, setHover] = React.useState(false);
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        display: "flex", alignItems: "center", gap: 10,
        width: "100%",
        padding: "8px 12px",
        background: selected
          ? `linear-gradient(90deg, ${game.art.accent}18, ${game.art.accent}06)`
          : hover ? TOK.c.bg2 : "transparent",
        borderLeft: `2px solid ${selected ? game.art.accent : "transparent"}`,
        border: "none",
        textAlign: "left",
        cursor: "pointer",
        color: "inherit",
        fontFamily: TOK.font.ui,
      }}
    >
      <Cover game={game} w={32} h={44} sleeve={false} label={false} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 6,
          fontSize: 12.5, fontWeight: 500,
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
          color: selected ? TOK.c.ink0 : TOK.c.ink0,
        }}>
          {playing && <RotatingDot color={TOK.c.ok} />}
          <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{game.short}</span>
        </div>
        <div style={{
          display: "flex", alignItems: "center", gap: 6,
          marginTop: 2,
          fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3,
          letterSpacing: "0.06em",
        }}>
          <span>{game.catalog}</span>
          <span>·</span>
          <span>{game.lastPlayed ? relDate(game.lastPlayed) : "unplayed"}</span>
        </div>
      </div>
      {game.lan && (
        <span title="Shared on LAN" style={{ color: TOK.c.ink3, display: "flex", flexShrink: 0 }}>
          {ICN.wifi}
        </span>
      )}
    </button>
  );
}

/* ─────────────────────────── Detail (right pane) ─────────────────────────── */
function LibraryDetail({ game, isNowPlaying }) {
  const acc = game.art.accent;
  return (
    <div style={{
      flex: 1, minWidth: 0,
      background: TOK.c.bg0,
      display: "flex", flexDirection: "column",
      overflowY: "auto",
    }}>
      <DetailHero game={game} isNowPlaying={isNowPlaying} />
      <StatsStrip game={game} acc={acc} />
      <ActionToolbar game={game} acc={acc} />
      <div style={{
        padding: "20px 28px 28px",
        display: "grid",
        gridTemplateColumns: "minmax(0,1.4fr) minmax(0,1fr)",
        gap: 14,
      }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, minWidth: 0 }}>
          <AboutCard game={game} acc={acc} />
          <SavesCard game={game} acc={acc} />
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, minWidth: 0 }}>
          <DetailsCard game={game} acc={acc} />
        </div>
      </div>
    </div>
  );
}

function DetailHero({ game, isNowPlaying }) {
  const acc = game.art.accent;
  return (
    <div style={{
      position: "relative",
      height: 280,
      background: `linear-gradient(135deg, ${game.art.from} 0%, ${game.art.to} 100%)`,
      overflow: "hidden",
      borderBottom: `1px solid ${TOK.c.line}`,
    }}>
      {/* tape strip across top */}
      <div style={{
        position: "absolute", top: 0, left: 0, right: 0, height: 4,
        background: `linear-gradient(90deg, ${acc} 0%, ${acc}99 50%, ${acc} 100%)`,
      }} />
      {/* tape-reel halo */}
      <div style={{
        position: "absolute", right: -120, top: -80,
        width: 420, height: 420, borderRadius: "50%",
        border: `1px solid ${acc}22`,
        background: `radial-gradient(circle at 35% 35%, ${acc}33, transparent 55%)`,
      }} />
      <div style={{
        position: "absolute", right: -40, top: 30,
        width: 260, height: 260, borderRadius: "50%",
        border: `1px dashed ${acc}33`,
      }} />
      {/* grain */}
      <div style={{
        position: "absolute", inset: 0,
        backgroundImage: "radial-gradient(rgba(255,255,255,0.05) 1px, transparent 1px)",
        backgroundSize: "3px 3px",
        opacity: 0.4,
        mixBlendMode: "overlay",
      }} />
      {/* bottom fade */}
      <div style={{
        position: "absolute", inset: 0,
        background: `linear-gradient(180deg, transparent 40%, ${TOK.c.bg0} 100%)`,
      }} />

      {/* content */}
      <div style={{
        position: "absolute", left: 28, right: 28, bottom: 22, top: 26,
        display: "flex", flexDirection: "column", justifyContent: "space-between",
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <CatalogId id={game.catalog} accent={acc} />
          <MonoLabel size={10} color={acc}>SIDE A · {game.art.mood.toUpperCase()}</MonoLabel>
          {isNowPlaying && (
            <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
              <RotatingDot color={TOK.c.ok} />
              <MonoLabel size={10} color={TOK.c.ok}>RUNNING · 00:43:12</MonoLabel>
            </span>
          )}
        </div>

        <div>
          <div style={{
            fontFamily: TOK.font.display,
            fontSize: 44, fontWeight: 700,
            letterSpacing: "-0.025em",
            color: TOK.c.ink0,
            lineHeight: 1.04,
            textShadow: "0 2px 16px rgba(0,0,0,0.4)",
            maxWidth: 720,
            textWrap: "balance",
          }}>{game.name}</div>

          <div style={{
            display: "flex", alignItems: "center", gap: 14,
            marginTop: 14,
          }}>
            <button style={{
              display: "inline-flex", alignItems: "center", gap: 9,
              padding: "0 22px", height: 42,
              background: acc, color: "#0b0c0e",
              border: "none", borderRadius: TOK.r.md,
              fontFamily: TOK.font.ui, fontWeight: 600, fontSize: 14,
              cursor: "pointer",
              boxShadow: `0 6px 20px ${acc}44`,
            }}>{ICN.play} Play</button>

            <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
              <MonoLabel size={9.5} color={acc}>LAST · {game.lastPlayed ? relDate(game.lastPlayed) : "NEVER"}</MonoLabel>
              <span style={{ fontSize: 11.5, color: TOK.c.ink2, fontFamily: TOK.font.mono, letterSpacing: "0.04em" }}>
                {fmtPlay(game.playtime)} · {game.sessions} session{game.sessions === 1 ? "" : "s"}
              </span>
            </div>

            <div style={{ flex: 1 }} />

            {game.lan && <Pill kind="ok">Shared · LAN</Pill>}
            <Pill kind={game.sync === "ok" ? "ok" : game.sync === "warn" ? "warn" : "info"}>
              {game.sync === "ok" ? "Sync · synced"
               : game.sync === "warn" ? "Local newer"
               : "Cloud newer"}
            </Pill>
          </div>
        </div>
      </div>
    </div>
  );
}

function StatsStrip({ game, acc }) {
  const items = [
    { label: "Last played", value: game.lastPlayed ? relDate(game.lastPlayed) : "Never", sub: game.lastPlayed ? absDateTime(game.lastPlayed) : "—" },
    { label: "Playtime",    value: fmtPlay(game.playtime), sub: game.sessions > 0 ? `${game.sessions} sessions` : "no sessions" },
    { label: "Install size", value: fmtSize(game.installSize), sub: "on D:\\" },
    { label: "Saves",       value: game.backup.count > 0 ? game.backup.count + " backups" : "—", sub: game.backup.count > 0 ? fmtSize(game.backup.size) + " · " + relDate(game.backup.last) : "no backups yet" },
  ];
  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: "repeat(4, 1fr)",
      borderBottom: `1px solid ${TOK.c.line}`,
      padding: "16px 28px",
    }}>
      {items.map((it, i) => (
        <div key={it.label} style={{
          padding: "0 18px",
          borderLeft: i === 0 ? "none" : `1px dashed ${TOK.c.line}`,
        }}>
          <MonoLabel size={9.5}>{it.label}</MonoLabel>
          <div style={{
            fontFamily: TOK.font.display,
            fontSize: 20, fontWeight: 600,
            letterSpacing: "-0.015em",
            marginTop: 4,
            color: TOK.c.ink0,
          }}>{it.value}</div>
          <div style={{ fontSize: 10.5, color: TOK.c.ink2, fontFamily: TOK.font.mono, marginTop: 2, letterSpacing: "0.04em" }}>{it.sub}</div>
        </div>
      ))}
    </div>
  );
}

function ActionToolbar({ game, acc }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 6,
      padding: "12px 28px",
      borderBottom: `1px solid ${TOK.c.line}`,
    }}>
      <Btn icon={ICN.folder}>Open folder</Btn>
      <Btn icon={ICN.sparkle}>Armoury Crate</Btn>
      <Btn icon={ICN.steam}>Add to Steam</Btn>
      <Btn icon={ICN.share}>Share on LAN</Btn>
      <div style={{ flex: 1 }} />
      <Btn icon={ICN.pencil}>Edit</Btn>
      <Btn danger icon={ICN.trash}>Remove</Btn>
    </div>
  );
}

/* ─────────────────────────── Detail cards ─────────────────────────── */
function DetailCard({ title, accent, action, children, mono }) {
  return (
    <section style={{
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line}`,
      borderRadius: TOK.r.md,
      overflow: "hidden",
      minWidth: 0,
    }}>
      <header style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        gap: 10,
        padding: "10px 14px",
        borderBottom: `1px dashed ${TOK.c.line}`,
        background: TOK.c.bg2,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ width: 4, height: 14, background: accent, borderRadius: 1 }} />
          <MonoLabel size={10}>{title}</MonoLabel>
        </div>
        {action}
      </header>
      <div style={{ padding: 14 }}>
        {children}
      </div>
    </section>
  );
}

function AboutCard({ game, acc }) {
  return (
    <DetailCard title="ABOUT" accent={acc}>
      <p style={{
        margin: 0,
        fontSize: 13, lineHeight: 1.6, color: TOK.c.ink1, textWrap: "pretty",
      }}>{game.description}</p>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6, marginTop: 12 }}>
        {game.genres.map(g => (
          <span key={g} style={{
            display: "inline-flex", alignItems: "center",
            padding: "3px 8px",
            background: TOK.c.bg2,
            border: `1px solid ${TOK.c.line2}`,
            borderRadius: TOK.r.sm,
            fontSize: 11, color: TOK.c.ink1,
          }}>{g}</span>
        ))}
      </div>
    </DetailCard>
  );
}

function SavesCard({ game, acc }) {
  const sb = game.backup;
  const has = sb.count > 0;
  return (
    <DetailCard
      title="SAVE BACKUP · LUDUSAVI"
      accent={acc}
      action={
        <div style={{ display: "flex", gap: 6 }}>
          <Btn icon={ICN.upload} style={{ height: 24, fontSize: 11.5 }}>Back up</Btn>
          <Btn icon={ICN.download} style={{ height: 24, fontSize: 11.5 }}>Restore…</Btn>
        </div>
      }
    >
      {has ? (
        <>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 18 }}>
            <Stat label="LAST BACKUP" value={relDate(sb.last)} sub={absDateTime(sb.last)} />
            <Stat label="REVISIONS" value={`${sb.count}`} sub="across all profiles" />
            <Stat label="TOTAL SIZE" value={fmtSize(sb.size)} sub="compressed" />
          </div>
          <div style={{
            marginTop: 14, padding: "9px 12px",
            background: "rgba(126,226,164,0.06)",
            border: `1px solid ${TOK.c.ok}33`,
            borderRadius: TOK.r.sm,
            display: "flex", alignItems: "center", gap: 8,
            fontSize: 11.5, color: TOK.c.ink1,
          }}>
            <span style={{ color: TOK.c.ok, display: "flex" }}>{ICN.check}</span>
            Saves restore before launch and back up on exit automatically.
          </div>
        </>
      ) : (
        <div style={{
          padding: "16px 12px", display: "flex", alignItems: "center", gap: 10,
          fontSize: 12.5, color: TOK.c.ink2,
        }}>
          <span style={{ color: TOK.c.ink3, display: "flex" }}>{ICN.cloud}</span>
          No backups yet — run the game once to detect save locations.
        </div>
      )}
    </DetailCard>
  );
}

function Stat({ label, value, sub }) {
  return (
    <div>
      <MonoLabel size={9}>{label}</MonoLabel>
      <div style={{ fontFamily: TOK.font.display, fontSize: 18, fontWeight: 600, letterSpacing: "-0.01em", marginTop: 3 }}>
        {value}
      </div>
      <div style={{ fontSize: 10.5, color: TOK.c.ink3, fontFamily: TOK.font.mono, marginTop: 2, letterSpacing: "0.04em" }}>
        {sub}
      </div>
    </div>
  );
}

function DetailsCard({ game, acc }) {
  const rows = [
    ["Developer",  game.dev],
    ["Publisher",  game.pub],
    ["Released",   absDate(game.release)],
    ["Added",      absDate(game.added)],
    ["Executable", game.exe, true],
    ["Install",    game.installPath, true, true],
    ["LAN",        game.lan ? "Visible to peers" : "Local only"],
  ];
  return (
    <DetailCard title="ENTRY · DETAILS" accent={acc}>
      <div style={{ display: "flex", flexDirection: "column" }}>
        {rows.map(([label, val, mono, copy], i) => (
          <div key={label} style={{
            display: "grid", gridTemplateColumns: "94px 1fr auto", gap: 10, alignItems: "center",
            padding: "8px 0",
            borderBottom: i < rows.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
          }}>
            <div style={{ fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.1em", textTransform: "uppercase" }}>
              {label}
            </div>
            <div style={{
              fontFamily: mono ? TOK.font.mono : TOK.font.ui,
              fontSize: mono ? 11.5 : 12.5,
              color: TOK.c.ink0,
              overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            }}>{val}</div>
            {copy && (
              <button style={{
                background: "transparent", border: "none", color: TOK.c.ink3, cursor: "pointer",
                display: "inline-flex", padding: 2,
              }} title="Copy">
                {ICN.copy}
              </button>
            )}
          </div>
        ))}
      </div>
    </DetailCard>
  );
}

Object.assign(window, {
  LIB, LibraryWindow, relDate, absDate, absDateTime, fmtPlay, fmtSize,
});
