/* Spool · Add Game (cassette reskin) + Touch / Steam Deck shelf */

/* ─────────────────────────── ADD GAME ─────────────────────────── */
const ADD_DETECTED = {
  exe: "nightreign.exe",
  path: "D:\\Games\\Elden Ring - Nightreign\\nightreign.exe",
  sizeMB: 64512,
  version: "1.04.0",
  arch: "x64",
};
const ADD_CANDIDATES = [
  { name: "Elden Ring: Nightreign", saves: 41, conf: 96, best: true, paths: [
    "%APPDATA%\\EldenRingNightreign\\save",
    "%LOCALAPPDATA%\\EldenRingNightreign\\settings.cfg",
    "Steam Cloud · 1.4 MB",
  ]},
  { name: "Elden Ring",              saves: 28, conf: 44, paths: [
    "%APPDATA%\\EldenRing\\save",
    "Steam Cloud · 4.2 MB",
  ]},
  { name: "Elden Ring: Shadow of the Erdtree", saves: 19, conf: 31, paths: [
    "%APPDATA%\\EldenRing\\save",
  ]},
];

function AddGameWindow({ width = 720, height = 620 }) {
  const [picked, setPicked] = React.useState("Elden Ring: Nightreign");
  const [expanded, setExpanded] = React.useState("Elden Ring: Nightreign");
  const [admin, setAdmin] = React.useState(false);
  const [folder, setFolder] = React.useState("D:\\Games\\Elden Ring - Nightreign");

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
      <AddChrome />
      <div style={{ padding: "24px 28px 8px" }}>
        <MonoLabel size={10}>Spool · catalog new entry</MonoLabel>
        <h1 style={{
          fontFamily: TOK.font.display, fontSize: 26, fontWeight: 700,
          letterSpacing: "-0.02em", margin: "6px 0 4px",
        }}>Add a game</h1>
        <p style={{ margin: 0, fontSize: 12.5, color: TOK.c.ink2, lineHeight: 1.5 }}>
          Drop a game's executable below. Spool will identify it through ludusavi so saves back up automatically.
        </p>
      </div>

      <div style={{ flex: 1, padding: "16px 28px 0", display: "flex", flexDirection: "column", gap: 12, overflowY: "auto" }}>
        <ExeStrip />

        <div>
          <div style={{
            display: "flex", alignItems: "center", justifyContent: "space-between",
            marginBottom: 8,
          }}>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <MonoLabel size={10} color={TOK.c.spool}>{ICN.sparkle} AUTO-MATCHED</MonoLabel>
              <span style={{ fontSize: 12, color: TOK.c.ink2 }}>
                3 candidates in ludusavi's database
              </span>
            </div>
            <button style={{
              background: "transparent", border: "none",
              color: TOK.c.ink2, fontSize: 11.5, cursor: "pointer", padding: 0,
              display: "inline-flex", alignItems: "center", gap: 4,
            }}>
              {ICN.search} Search manually
            </button>
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {ADD_CANDIDATES.map(c => (
              <CandidateRow key={c.name}
                guess={c}
                picked={picked === c.name}
                onPick={() => setPicked(c.name)}
                expanded={expanded === c.name}
                onExpand={() => setExpanded(expanded === c.name ? null : c.name)}
              />
            ))}
          </div>
        </div>

        <details style={{ marginTop: 4 }}>
          <summary style={{
            cursor: "pointer", listStyle: "none",
            fontSize: 12, color: TOK.c.ink2,
            display: "inline-flex", alignItems: "center", gap: 6, userSelect: "none",
          }}>
            {ICN.chev} More options
          </summary>
          <div style={{
            marginTop: 12,
            background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`,
            borderRadius: TOK.r.sm,
            padding: 14, display: "flex", flexDirection: "column", gap: 12,
          }}>
            <div style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: 12, alignItems: "center" }}>
              <span style={{ fontSize: 12, color: TOK.c.ink1 }}>Install folder</span>
              <Row>
                <Input value={folder} onChange={setFolder} mono prefix={ICN.folder} />
                <Btn icon={ICN.folder}>Browse</Btn>
              </Row>
              <span style={{ fontSize: 12, color: TOK.c.ink1 }}>Permissions</span>
              <label style={{ display: "inline-flex", alignItems: "center", gap: 10, fontSize: 12, color: TOK.c.ink1 }}>
                <Toggle value={admin} onChange={setAdmin} />
                Run as administrator
              </label>
              <span style={{ fontSize: 12, color: TOK.c.ink1 }}>Catalog ID</span>
              <span style={{ display: "inline-flex", alignItems: "center", gap: 10 }}>
                <CatalogId id="SPL-0047" accent={TOK.c.spool} />
                <span style={{ fontSize: 11, color: TOK.c.ink3 }}>Auto-assigned · next in sequence</span>
              </span>
            </div>
          </div>
        </details>
      </div>

      <div style={{
        padding: "12px 20px 16px",
        borderTop: `1px solid ${TOK.c.line}`,
        background: "rgba(0,0,0,0.18)",
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <Btn style={{ color: TOK.c.ink2 }}>Cancel</Btn>
        <div style={{ flex: 1 }} />
        <Btn icon={ICN.sparkle}>Armoury Crate</Btn>
        <Btn icon={ICN.steam}>Add to Steam</Btn>
        <Btn variant="primary" accent={TOK.c.spool} style={{ minWidth: 140, height: 32, fontSize: 13 }}>
          Add to library
        </Btn>
      </div>
    </div>
  );
}

function AddChrome() {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 12,
      height: TOK.d.desktop.titleBar,
      padding: "0 8px 0 14px",
      background: "rgba(0,0,0,0.32)",
      borderBottom: `1px solid ${TOK.c.line}`,
      userSelect: "none",
    }}>
      <SpoolMark size={18} color={TOK.c.ink1} tape={TOK.c.spool} />
      <MonoLabel size={10.5}>SPOOL</MonoLabel>
      <span style={{ color: TOK.c.ink3, fontSize: 10 }}>/</span>
      <MonoLabel size={10.5} color={TOK.c.ink1}>ADD ENTRY</MonoLabel>
      <div style={{ flex: 1 }} />
      <ChromeBtn glyph="close" />
    </div>
  );
}

function ExeStrip() {
  return (
    <div style={{
      padding: "12px 14px",
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line}`,
      borderRadius: TOK.r.md,
      display: "flex", alignItems: "center", gap: 14,
      position: "relative",
      overflow: "hidden",
    }}>
      {/* tape strip on left edge */}
      <div style={{
        position: "absolute", left: 0, top: 0, bottom: 0, width: 3,
        background: TOK.c.spool,
      }} />
      <div style={{
        width: 38, height: 38, borderRadius: TOK.r.sm,
        background: TOK.c.bg2, border: `1px solid ${TOK.c.line2}`,
        display: "flex", alignItems: "center", justifyContent: "center",
        color: TOK.c.ink1, flexShrink: 0,
      }}>
        {ICN.exe}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: 13, fontWeight: 500,
          display: "flex", alignItems: "center", gap: 8,
        }}>
          {ADD_DETECTED.exe}
          <Pill kind="ok">Identified</Pill>
        </div>
        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3,
          marginTop: 3, letterSpacing: "0.04em",
        }}>
          <span style={{
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", minWidth: 0,
          }}>{ADD_DETECTED.path}</span>
          <span style={{ flexShrink: 0 }}>· {(ADD_DETECTED.sizeMB / 1024).toFixed(1)} GB</span>
          <span style={{ flexShrink: 0 }}>· v{ADD_DETECTED.version} {ADD_DETECTED.arch}</span>
        </div>
      </div>
      <Btn>Change</Btn>
    </div>
  );
}

function CandidateRow({ guess, picked, onPick, expanded, onExpand }) {
  return (
    <div style={{
      background: picked ? `${TOK.c.spool}14` : TOK.c.bg1,
      border: `1px solid ${picked ? TOK.c.spool + "66" : TOK.c.line}`,
      borderRadius: TOK.r.md,
      overflow: "hidden",
    }}>
      <button
        onClick={onPick}
        style={{
          display: "flex", alignItems: "center", gap: 12,
          width: "100%", padding: "10px 14px",
          background: "transparent", border: "none",
          color: TOK.c.ink0, fontFamily: TOK.font.ui, fontSize: 13,
          cursor: "pointer", textAlign: "left",
        }}
      >
        <span style={{
          width: 16, height: 16, borderRadius: 8,
          border: `1.5px solid ${picked ? TOK.c.spool : TOK.c.line3}`,
          display: "flex", alignItems: "center", justifyContent: "center",
          flexShrink: 0,
        }}>
          {picked && <span style={{ width: 7, height: 7, borderRadius: 4, background: TOK.c.spool }} />}
        </span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, fontWeight: picked ? 500 : 400 }}>
            {guess.name}
            {guess.best && <MonoLabel size={9} color={TOK.c.spool}>BEST MATCH</MonoLabel>}
          </div>
          <div style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, marginTop: 3, letterSpacing: "0.06em" }}>
            {guess.saves} save files tracked
          </div>
        </div>

        {/* Confidence meter — vertical bars */}
        <div style={{ display: "flex", alignItems: "flex-end", gap: 2, height: 16, marginRight: 10 }}>
          {[20, 40, 60, 80, 100].map(n => (
            <span key={n} style={{
              width: 3, height: 4 + (n / 100) * 12,
              background: guess.conf >= n ? TOK.c.spool : TOK.c.bg3,
              borderRadius: 1,
            }}/>
          ))}
        </div>
        <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.04em", minWidth: 32, textAlign: "right" }}>
          {guess.conf}%
        </span>

        <span
          onClick={(e) => { e.stopPropagation(); onExpand(); }}
          style={{
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            width: 22, height: 22, color: TOK.c.ink3,
            transform: expanded ? "rotate(180deg)" : "rotate(0)",
            transition: "transform 140ms ease", flexShrink: 0,
          }}
        >
          {ICN.chev}
        </span>
      </button>

      {expanded && (
        <div style={{
          padding: "10px 14px 12px 42px",
          borderTop: `1px dashed ${picked ? TOK.c.spool + "33" : TOK.c.line}`,
          background: "rgba(0,0,0,0.2)",
        }}>
          <MonoLabel size={9}>Save locations · ludusavi will track</MonoLabel>
          <div style={{ marginTop: 6, display: "flex", flexDirection: "column", gap: 4 }}>
            {guess.paths.map((p, i) => (
              <div key={i} style={{
                fontFamily: p.startsWith("Steam") ? TOK.font.ui : TOK.font.mono,
                fontSize: 11.5, color: TOK.c.ink1,
                display: "flex", alignItems: "center", gap: 8,
              }}>
                <span style={{ color: TOK.c.ink3, display: "flex" }}>{ICN.folder}</span>
                {p}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/* ─────────────────────────── TOUCH / STEAM DECK SHELF ─────────────────────────── */
function DeckShelf({ width = 1280, height = 800 }) {
  const [picked, setPicked] = React.useState("elden-ring-nightreign");
  const game = LIB.find(g => g.id === picked) || LIB[1];
  const acc = game.art.accent;

  return (
    <div style={{
      width, height,
      background: `linear-gradient(180deg, ${TOK.c.bg0} 0%, #06070a 100%)`,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      display: "flex", flexDirection: "column",
      borderRadius: 14,
      overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.08)",
      position: "relative",
    }}>
      {/* Hero halo from picked game */}
      <div style={{
        position: "absolute", left: 0, right: 0, top: 0, height: 480,
        background: `radial-gradient(800px 380px at 22% 30%, ${game.art.from}aa, transparent 70%),
                     radial-gradient(700px 320px at 78% 0%, ${acc}22, transparent 60%)`,
        pointerEvents: "none",
      }} />

      <DeckChrome game={game} />

      {/* Hero band */}
      <DeckHero game={game} acc={acc} />

      {/* Shelf */}
      <DeckRail picked={picked} setPicked={setPicked} />

      {/* Hint bar pinned at the bottom — Big Picture style */}
      <DeckHintBar />
    </div>
  );
}

function DeckChrome({ game }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 16,
      height: 44, padding: "0 22px",
      background: "rgba(0,0,0,0.35)",
      borderBottom: `1px solid ${TOK.c.line}`,
      position: "relative", zIndex: 2,
    }}>
      <SpoolMark size={22} color={TOK.c.ink1} tape={TOK.c.spool} />
      <MonoLabel size={11}>SPOOL · DECK</MonoLabel>
      <span style={{ color: TOK.c.ink3 }}>·</span>
      <MonoLabel size={11} color={TOK.c.ink1}>SIDE A · LIBRARY</MonoLabel>
      <div style={{ flex: 1 }} />
      <div style={{
        display: "inline-flex", alignItems: "center", gap: 8,
        padding: "0 10px", height: 24, borderRadius: TOK.r.sm,
        background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`,
        fontFamily: TOK.font.mono, fontSize: 10.5, letterSpacing: "0.08em", color: TOK.c.ink2,
      }}>
        <span style={{ width: 6, height: 6, borderRadius: 3, background: TOK.c.ok, boxShadow: `0 0 8px ${TOK.c.ok}88` }} />
        SYNCED · 4 DEVICES
      </div>
      <div style={{
        display: "inline-flex", alignItems: "center", gap: 6,
        fontFamily: TOK.font.mono, fontSize: 11, color: TOK.c.ink2,
      }}>
        {ICN.signal} 87%
      </div>
      <div style={{
        display: "inline-flex", alignItems: "center", gap: 4,
        padding: "2px 7px",
        background: TOK.c.bg2, borderRadius: TOK.r.sm,
        fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.06em",
      }}>
        21:43
      </div>
    </div>
  );
}

function DeckHero({ game, acc }) {
  return (
    <div style={{
      flex: 1, padding: "32px 56px 24px",
      display: "grid", gridTemplateColumns: "1fr 1.4fr", gap: 40, alignItems: "center",
      position: "relative", zIndex: 1, minHeight: 0,
    }}>
      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <DeckPoster game={game} />
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 16, maxWidth: 540 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <CatalogId id={game.catalog} accent={acc} />
          <MonoLabel size={11} color={acc}>SIDE A · {game.art.mood.toUpperCase()}</MonoLabel>
        </div>
        <h1 style={{
          margin: 0,
          fontFamily: TOK.font.display, fontSize: 52, fontWeight: 700,
          letterSpacing: "-0.025em", lineHeight: 1.02, textWrap: "balance",
          textShadow: "0 2px 18px rgba(0,0,0,0.5)",
        }}>{game.name}</h1>
        <div style={{ display: "flex", alignItems: "center", gap: 14, fontFamily: TOK.font.mono, fontSize: 12, color: TOK.c.ink2, letterSpacing: "0.06em" }}>
          <span>{game.dev}</span>
          <span style={{ color: TOK.c.ink3 }}>·</span>
          <span>{fmtPlay(game.playtime)}</span>
          <span style={{ color: TOK.c.ink3 }}>·</span>
          <span>{game.sessions} sessions</span>
          <span style={{ color: TOK.c.ink3 }}>·</span>
          <span>LAST · {game.lastPlayed ? relDate(game.lastPlayed) : "NEVER"}</span>
        </div>
        <p style={{
          margin: 0, fontSize: 14.5, lineHeight: 1.55, color: TOK.c.ink1,
          maxWidth: 500, textWrap: "pretty",
        }}>{game.description}</p>

        <div style={{ display: "flex", alignItems: "center", gap: 10, marginTop: 6 }}>
          <button style={{
            display: "inline-flex", alignItems: "center", gap: 10,
            padding: "0 28px", height: 56, minWidth: 200,
            background: acc, color: "#0b0c0e",
            border: "none", borderRadius: 10,
            fontFamily: TOK.font.display, fontSize: 18, fontWeight: 700,
            letterSpacing: "-0.01em", cursor: "pointer",
            boxShadow: `0 10px 30px ${acc}44`,
          }}>
            <I d="M5 3.2v9.6L14 8z" fill size={18} stroke={0} />
            Play
          </button>
          <DeckIconBtn icon={ICN.folder} label="Open" />
          <DeckIconBtn icon={ICN.share} label="Share" />
          <DeckIconBtn icon={ICN.cog} label="Settings" />
        </div>
      </div>
    </div>
  );
}

function DeckIconBtn({ icon, label }) {
  return (
    <button style={{
      display: "inline-flex", flexDirection: "column", alignItems: "center", gap: 3,
      width: 64, height: 56, padding: "8px 0",
      background: "rgba(255,255,255,0.04)",
      border: `1px solid ${TOK.c.line2}`,
      borderRadius: TOK.r.md,
      color: TOK.c.ink1,
      cursor: "pointer",
    }}>
      {icon}
      <span style={{ fontFamily: TOK.font.mono, fontSize: 9, letterSpacing: "0.08em" }}>
        {label.toUpperCase()}
      </span>
    </button>
  );
}

/* ─────────────────────────── Hint bar (bottom, Big-Picture-style) ─────────────────────────── */
function DeckHintBar() {
  /* Contextual bindings — left side relates to what's focused (a game tile),
     right side to the global actions. */
  const left = [
    { glyph: "A", label: "Play" },
    { glyph: "Y", label: "Details" },
    { glyph: "X", label: "Saves" },
    { glyph: "lstick", label: "Browse" },
    { glyph: "rstick", label: "Scroll detail" },
  ];
  const right = [
    { glyph: "L1", label: "Filter" },
    { glyph: "R1", label: "Sort" },
    { glyph: "menu", label: "Menu" },
    { glyph: "view", label: "Search" },
    { glyph: "B", label: "Back" },
  ];
  return (
    <div style={{
      height: 44, flexShrink: 0,
      display: "flex", alignItems: "center", justifyContent: "space-between",
      padding: "0 32px",
      background: "rgba(0,0,0,0.55)",
      borderTop: `1px solid ${TOK.c.line2}`,
      backdropFilter: "blur(12px)",
      position: "relative", zIndex: 2,
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 22 }}>
        {left.map(h => <CtrlHint key={h.glyph} {...h} />)}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 22 }}>
        {right.map(h => <CtrlHint key={h.glyph} {...h} />)}
      </div>
    </div>
  );
}

function DeckPoster({ game }) {
  return (
    <div style={{
      width: 280, height: 380,
      borderRadius: 12,
      background: `linear-gradient(160deg, ${game.art.from} 0%, ${game.art.to} 100%)`,
      position: "relative", overflow: "hidden",
      boxShadow: `0 28px 80px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.06), 0 0 60px ${game.art.accent}33`,
    }}>
      {/* halo */}
      <div style={{
        position: "absolute", left: "-30%", top: "10%",
        width: 320, height: 320, borderRadius: "50%",
        background: `radial-gradient(circle at 30% 30%, ${game.art.accent}66, transparent 60%)`,
        mixBlendMode: "screen",
      }} />
      <div style={{
        position: "absolute", right: "-25%", bottom: "-15%",
        width: 260, height: 260, borderRadius: "50%",
        border: `1px dashed ${game.art.accent}55`,
      }} />
      {/* tape label band */}
      <div style={{
        position: "absolute", top: 0, left: 0, right: 0, height: 26,
        background: `linear-gradient(to bottom, ${game.art.accent}, ${game.art.accent}cc)`,
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "0 14px",
        fontFamily: TOK.font.mono, fontSize: 10, letterSpacing: "0.14em",
        color: "rgba(0,0,0,0.7)", fontWeight: 600,
      }}>
        <span>SPOOL · LIBRARY</span>
        <span>{game.catalog}</span>
      </div>
      {/* grain */}
      <div style={{
        position: "absolute", inset: 0,
        backgroundImage: "radial-gradient(rgba(255,255,255,0.05) 1px, transparent 1px)",
        backgroundSize: "3px 3px",
        opacity: 0.5,
        mixBlendMode: "overlay",
      }} />
      {/* title at bottom */}
      <div style={{ position: "absolute", left: 18, right: 18, bottom: 18 }}>
        <MonoLabel size={9} color="rgba(255,255,255,0.65)">SIDE A · {game.art.mood.toUpperCase()}</MonoLabel>
        <div style={{
          marginTop: 4,
          fontFamily: TOK.font.display, fontSize: 28, fontWeight: 700,
          letterSpacing: "-0.02em", lineHeight: 1.04,
          textShadow: "0 1px 12px rgba(0,0,0,0.6)",
          textWrap: "balance",
        }}>{game.short}</div>
      </div>
    </div>
  );
}

function DeckRail({ picked, setPicked }) {
  return (
    <div style={{
      padding: "18px 56px 28px",
      borderTop: `1px solid ${TOK.c.line}`,
      background: "rgba(0,0,0,0.32)",
      backdropFilter: "blur(20px)",
      position: "relative", zIndex: 1,
    }}>
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        marginBottom: 10,
      }}>
        <MonoLabel size={10}>Library · Side A · {LIB.length} entries</MonoLabel>
        <div style={{ display: "flex", alignItems: "center", gap: 8, fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.08em" }}>
          ALL · RECENT · SHARED
        </div>
      </div>
      <div style={{
        display: "flex", gap: 14, overflowX: "auto",
        paddingBottom: 6, scrollSnapType: "x mandatory",
      }}>
        {LIB.map(g => (
          <DeckTile
            key={g.id}
            game={g}
            active={picked === g.id}
            onClick={() => setPicked(g.id)}
          />
        ))}
      </div>
    </div>
  );
}

function DeckTile({ game, active, onClick }) {
  return (
    <button onClick={onClick} style={{
      width: 132, flexShrink: 0,
      padding: 0, border: "none", background: "transparent",
      cursor: "pointer",
      scrollSnapAlign: "start",
      display: "flex", flexDirection: "column", gap: 6,
      transform: active ? "translateY(-4px)" : "translateY(0)",
      transition: "transform 140ms ease",
    }}>
      <div style={{
        outline: active ? `2px solid ${game.art.accent}` : "2px solid transparent",
        outlineOffset: 3,
        borderRadius: TOK.r.md,
      }}>
        <Cover game={game} w={132} h={186} />
      </div>
      <div style={{ paddingTop: 2 }}>
        <div style={{
          fontSize: 12.5, fontWeight: 500,
          color: active ? TOK.c.ink0 : TOK.c.ink1,
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
          textAlign: "left",
        }}>{game.short}</div>
        <div style={{
          fontFamily: TOK.font.mono, fontSize: 9, color: TOK.c.ink3, letterSpacing: "0.06em",
          textAlign: "left", marginTop: 1,
        }}>{game.catalog} · {game.lastPlayed ? relDate(game.lastPlayed) : "unplayed"}</div>
      </div>
    </button>
  );
}

Object.assign(window, { AddGameWindow, DeckShelf });
