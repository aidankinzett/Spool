/* Spool · Add Game (v2.1 — with ludusavi manifest signals)
   Per-row signals we DO have from ludusavi's manifest:
     • Name
     • Match score (shown only when < 95%; high-confidence rows hide it)
     • Save location hint — parsed from manifest `files` entry. Most useful disambiguator.
     • Store badges — Steam ID, GOG presence (hover for the ID)
     • Cloud sync flag — small ☁ when cloud.steam = true
   Not from ludusavi (don't surface here): genre, developer, year, cover art, playtime.
*/

/* ─────────────────────────── Mock candidate data ─────────────────────────── */
const NIGHTREIGN_CANDIDATES = [
  { name: "Elden Ring: Nightreign",                 savePath: "%APPDATA%/EldenRingNightreign/save",   steamId: 2622380, cloudSteam: true,  score: 98 },
  { name: "Elden Ring",                             savePath: "%APPDATA%/EldenRing/save",             steamId: 1245620, cloudSteam: true,  score: 84 },
  { name: "Elden Ring: Shadow of the Erdtree",      savePath: "%APPDATA%/EldenRing/save",             steamId: 2778580, cloudSteam: true,  score: 79 },
];

const OUTER_SEARCH = [
  { name: "Outer Wilds",                                 savePath: "%APPDATA%/.../OuterWilds_Data/saves",  steamId:  753640, cloudSteam: true,  gog: true,  score: 100 },
  { name: "Outer Wilds: Echoes of the Eye",              savePath: "%APPDATA%/.../OuterWilds_Data/saves",  steamId: 1622100, cloudSteam: true,             score: 92 },
  { name: "The Outer Worlds",                            savePath: "%LOCALAPPDATA%/Indiana/Saved/SaveGames", steamId: 578650, cloudSteam: true,  gog: true,  score: 71 },
  { name: "The Outer Worlds: Spacer's Choice Edition",   savePath: "%LOCALAPPDATA%/Indiana/Saved/SaveGames", steamId: 2120920, cloudSteam: true,           score: 68 },
  { name: "The Outer Worlds 2",                          savePath: "%LOCALAPPDATA%/IndianaII/Saved/SaveGames", steamId: 1449110, cloudSteam: true,         score: 64 },
  { name: "Outer Wilds (demo)",                          savePath: "%APPDATA%/.../OuterWilds_Data/saves",  steamId:  994710, cloudSteam: false,            score: 60 },
  { name: "Outer Terror",                                savePath: "%USERPROFILE%/Saved Games/OuterTerror", steamId: 2078500, cloudSteam: false,           score: 56 },
];

function AddGameV2({ state = "matches", width = 720, height = 560 }) {
  const file = state === "no-match"
    ? { name: "strangegame.exe", path: "D:\\Games\\StrangeGame\\strangegame.exe", size: 4.2 * 1024 }
    : state === "search"
      ? { name: "outerwilds.exe", path: "D:\\Games\\OuterWilds\\outerwilds.exe", size: 6.0 * 1024 }
      : { name: "nightreign.exe", path: "D:\\Games\\Elden Ring - Nightreign\\nightreign.exe", size: 64.5 * 1024 };

  const auto    = state === "no-match" || state === "search" ? [] : NIGHTREIGN_CANDIDATES;
  const results = state === "search"   ? OUTER_SEARCH         : [];
  const list    = state === "search"   ? results              : auto;

  const [picked, setPicked] = React.useState(list[0]?.name || null);

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
      display: "flex", flexDirection: "column",
    }}>
      <AddChromeV2 />

      <div style={{ padding: "20px 24px 12px" }}>
        <MonoLabel size={10}>Spool · catalog new entry</MonoLabel>
        <h1 style={{
          margin: "5px 0 4px",
          fontFamily: TOK.font.display, fontSize: 22, fontWeight: 700,
          letterSpacing: "-0.02em",
        }}>Add a game</h1>
        <p style={{ margin: 0, fontSize: 12, color: TOK.c.ink2, lineHeight: 1.5, maxWidth: 540 }}>
          Pick the game's executable. Spool runs it through ludusavi to identify it so saves back up automatically.
        </p>
      </div>

      <div style={{ flex: 1, padding: "8px 24px 0", display: "flex", flexDirection: "column", gap: 12, minHeight: 0 }}>
        <ExeStripV2 file={file} state={state} />

        {state !== "identifying" && (
          <SearchBar
            query={state === "search" ? "outer wild" : ""}
            resultCount={state === "search" ? results.length : null}
          />
        )}

        <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
          {state === "identifying" && <BodyIdentifying file={file} />}
          {state === "matches"     && <BodyList list={list} kind="auto"   picked={picked} setPicked={setPicked} />}
          {state === "search"      && <BodyList list={list} kind="search" picked={picked} setPicked={setPicked} />}
          {state === "no-match"    && <BodyNoMatch file={file} />}
        </div>
      </div>

      <AddFooterV2 picked={picked} state={state} />
    </div>
  );
}

function AddChromeV2() {
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

/* ─────────────────────────── EXE STRIP ─────────────────────────── */
function ExeStripV2({ file, state }) {
  const stripColor = state === "no-match" ? TOK.c.warn
                   : state === "identifying" ? TOK.c.info
                   : TOK.c.spool;
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
      <div style={{
        position: "absolute", left: 0, top: 0, bottom: 0, width: 3,
        background: stripColor,
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
          fontFamily: TOK.font.mono,
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
        }}>{file.name}</div>
        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3,
          marginTop: 3, letterSpacing: "0.04em",
        }}>
          <span style={{
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", minWidth: 0,
          }}>{file.path}</span>
          <span style={{ flexShrink: 0 }}>· {(file.size / 1024).toFixed(1)} GB</span>
        </div>
      </div>
      <Btn icon={ICN.folder}>Change file</Btn>
    </div>
  );
}

/* ─────────────────────────── SEARCH BAR ─────────────────────────── */
function SearchBar({ query, resultCount }) {
  const hasQuery = !!query;
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 8,
      height: 32, padding: "0 10px",
      background: TOK.c.bg2,
      border: `1px solid ${hasQuery ? TOK.c.line3 : TOK.c.line}`,
      borderRadius: TOK.r.sm,
    }}>
      <span style={{ color: TOK.c.ink2, display: "flex" }}>{ICN.search}</span>
      <input
        defaultValue={query}
        placeholder="Search ludusavi · 18 042 entries"
        style={{
          flex: 1, background: "transparent", border: "none", outline: "none",
          color: TOK.c.ink0, fontFamily: TOK.font.ui, fontSize: 12.5,
        }}
      />
      {hasQuery && resultCount != null && (
        <span style={{
          fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink2, letterSpacing: "0.06em",
        }}>{resultCount} results</span>
      )}
      <span style={{
        fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.08em",
        border: `1px solid ${TOK.c.line2}`, padding: "1px 5px", borderRadius: 2,
      }}>⌘K</span>
    </div>
  );
}

/* ─────────────────────────── BODIES ─────────────────────────── */
function BodyIdentifying({ file }) {
  return (
    <div style={{
      flex: 1,
      display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
      gap: 14, padding: "30px 0",
    }}>
      <SpinningReels />
      <div style={{ textAlign: "center" }}>
        <div style={{ fontFamily: TOK.font.display, fontSize: 17, fontWeight: 600, letterSpacing: "-0.01em" }}>
          Identifying through ludusavi…
        </div>
        <div style={{ fontSize: 12, color: TOK.c.ink3, marginTop: 4 }}>
          Usually takes 1–2 seconds.
        </div>
      </div>
    </div>
  );
}

function BodyList({ list, kind, picked, setPicked }) {
  return (
    <>
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        margin: "6px 2px 8px",
      }}>
        <MonoLabel size={10} color={kind === "auto" ? TOK.c.spool : TOK.c.ink2}>
          {kind === "auto" ? `LUDUSAVI · ${list.length} CANDIDATES` : `LUDUSAVI · SEARCH RESULTS · ${list.length}`}
        </MonoLabel>
        {kind === "auto" && (
          <span style={{ fontSize: 11, color: TOK.c.ink3 }}>
            Search above to widen to all 18 042 entries.
          </span>
        )}
      </div>
      <div style={{
        flex: 1, minHeight: 0, overflowY: "auto",
        background: TOK.c.bg1,
        border: `1px solid ${TOK.c.line}`,
        borderRadius: TOK.r.sm,
      }}>
        {list.map((c, i) => (
          <CandidateRowV2
            key={c.name}
            cand={c}
            index={i}
            picked={picked === c.name}
            onPick={() => setPicked(c.name)}
            last={i === list.length - 1}
          />
        ))}
      </div>
    </>
  );
}

/* ─────────────────────────── ROW + SIGNALS ─────────────────────────── */
function CandidateRowV2({ cand, index, picked, onPick, last }) {
  const [hover, setHover] = React.useState(false);
  const showScore = cand.score != null && cand.score < 95;
  return (
    <button
      onClick={onPick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: "grid",
        gridTemplateColumns: "20px 1fr auto auto",
        columnGap: 12, rowGap: 2,
        alignItems: "center",
        width: "100%", padding: "10px 14px",
        background: picked ? `${TOK.c.spool}14` : hover ? TOK.c.bg2 : "transparent",
        borderLeft: `2px solid ${picked ? TOK.c.spool : "transparent"}`,
        borderBottom: last ? "none" : `1px dashed ${TOK.c.line}`,
        border: "none",
        cursor: "pointer", textAlign: "left", color: "inherit",
        fontFamily: TOK.font.ui,
      }}
    >
      {/* Radio · spans both rows */}
      <span style={{
        gridRow: "1 / span 2",
        width: 16, height: 16, borderRadius: 8,
        border: `1.5px solid ${picked ? TOK.c.spool : TOK.c.line3}`,
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        flexShrink: 0,
      }}>
        {picked && <span style={{ width: 7, height: 7, borderRadius: 4, background: TOK.c.spool }} />}
      </span>

      {/* Name */}
      <span style={{
        fontSize: 13.5,
        fontWeight: picked ? 500 : 400,
        color: picked ? TOK.c.ink0 : TOK.c.ink1,
        whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", minWidth: 0,
      }}>{cand.name}</span>

      {/* Right cluster: badges */}
      <span style={{ display: "inline-flex", alignItems: "center", gap: 6, flexShrink: 0 }}>
        {showScore && <MatchScoreLabel score={cand.score} />}
        {cand.steamId && <StoreBadge kind="steam" id={cand.steamId} />}
        {cand.gog && <StoreBadge kind="gog" />}
        {cand.cloudSteam && <CloudBadge />}
      </span>

      {/* Keyboard hint — quiet digit, fades in on hover */}
      {index < 9 ? (
        <span style={{
          gridRow: "1 / span 2",
          fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3,
          letterSpacing: "0.04em",
          opacity: hover || picked ? 0.85 : 0,
          transition: "opacity 100ms ease",
          width: 12, textAlign: "right",
        }}>{index + 1}</span>
      ) : <span style={{ gridRow: "1 / span 2", width: 12 }} />}

      {/* Save path */}
      <span style={{
        gridColumn: "2 / 4",
        display: "inline-flex", alignItems: "center", gap: 5,
        fontFamily: TOK.font.mono, fontSize: 10.5,
        color: TOK.c.ink3, letterSpacing: "0.02em",
        whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", minWidth: 0,
      }}>
        <span style={{ color: TOK.c.ink3, display: "flex", flexShrink: 0 }}>{ICN.folder}</span>
        {cand.savePath}
      </span>
    </button>
  );
}

function MatchScoreLabel({ score }) {
  const color = score >= 75 ? TOK.c.ink2 : score >= 60 ? TOK.c.warn : TOK.c.ink3;
  return (
    <span title={`Match confidence ${score}%`} style={{
      fontFamily: TOK.font.mono, fontSize: 10, color, letterSpacing: "0.04em",
      padding: "0 2px",
    }}>{score}%</span>
  );
}

function StoreBadge({ kind, id }) {
  if (kind === "steam") {
    return (
      <span
        title={id ? `Steam · ${id}` : "Steam"}
        style={{
          display: "inline-flex", alignItems: "center", gap: 4,
          padding: "2px 5px",
          background: "rgba(255,255,255,0.04)",
          border: `1px solid ${TOK.c.line2}`,
          borderRadius: 3,
          color: TOK.c.ink2,
        }}>
        <SteamGlyph />
      </span>
    );
  }
  if (kind === "gog") {
    return (
      <span
        title="GOG"
        style={{
          display: "inline-flex", alignItems: "center", justifyContent: "center",
          width: 18, height: 18, padding: 0,
          background: "rgba(255,255,255,0.04)",
          border: `1px solid ${TOK.c.line2}`,
          borderRadius: 3,
          fontFamily: TOK.font.mono, fontSize: 8.5, fontWeight: 700,
          color: TOK.c.ink2, letterSpacing: "-0.02em",
        }}>GOG</span>
    );
  }
  return null;
}

function SteamGlyph() {
  return (
    <svg width="11" height="11" viewBox="0 0 12 12" fill="none">
      <circle cx="6" cy="6" r="5.4" stroke="currentColor" strokeWidth="1.1" />
      <circle cx="8.2" cy="4.2" r="1.5" stroke="currentColor" strokeWidth="1" />
      <circle cx="4" cy="8.4" r="1.1" stroke="currentColor" strokeWidth="1" />
      <line x1="6.7" y1="4.7" x2="4.6" y2="7.9" stroke="currentColor" strokeWidth="0.9" />
    </svg>
  );
}

function CloudBadge() {
  return (
    <span
      title="Steam Cloud sync available"
      style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: 18, height: 18,
        color: TOK.c.info, opacity: 0.85,
      }}>
      <I d="M4.5 11.5a3 3 0 0 1-.3-6 3.5 3.5 0 0 1 6.8-.6 2.8 2.8 0 0 1 .5 5.6Z" stroke={1.4} size={13} />
    </span>
  );
}

function BodyNoMatch({ file }) {
  return (
    <>
      <div style={{
        margin: "6px 2px 8px",
        display: "flex", alignItems: "center", justifyContent: "space-between",
      }}>
        <MonoLabel size={10} color={TOK.c.warn}>LUDUSAVI · NO AUTOMATIC MATCH</MonoLabel>
        <span style={{ fontSize: 11, color: TOK.c.ink3 }}>
          Try a different name above.
        </span>
      </div>
      <div style={{
        flex: 1, padding: "20px 22px",
        background: TOK.c.bg1,
        border: `1px solid ${TOK.c.line}`,
        borderRadius: TOK.r.sm,
        display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
        gap: 14, textAlign: "center",
      }}>
        <div style={{
          width: 40, height: 40, borderRadius: 20,
          background: `${TOK.c.warn}22`, color: TOK.c.warn,
          display: "inline-flex", alignItems: "center", justifyContent: "center",
        }}>
          <I d="M8 4v5M8 11h.01M8 1.5 14.5 14h-13L8 1.5Z" stroke={1.5} size={18} />
        </div>
        <div>
          <div style={{ fontFamily: TOK.font.display, fontSize: 18, fontWeight: 600, letterSpacing: "-0.012em" }}>
            Spool couldn't identify <span style={{ fontFamily: TOK.font.mono, fontSize: 16 }}>{file.name}</span>
          </div>
          <p style={{
            margin: "6px 0 0", fontSize: 12.5, color: TOK.c.ink2,
            lineHeight: 1.55, maxWidth: 460,
          }}>
            Try the search above with a shorter name. If ludusavi still doesn't know this game,
            you can add it without save tracking — Spool will launch it but won't back up saves.
          </p>
        </div>
        <div style={{ display: "flex", gap: 6, marginTop: 4 }}>
          <Btn icon={ICN.external} style={{ fontSize: 11.5, height: 26 }}>File an issue against ludusavi</Btn>
        </div>
      </div>
    </>
  );
}

/* ─────────────────────────── FOOTER ─────────────────────────── */
function AddFooterV2({ picked, state }) {
  const canTrack = state !== "identifying" && !!picked;
  const canSkip  = state !== "identifying";

  const primaryLabel = state === "identifying"
    ? "Identifying…"
    : picked
      ? `Add as "${shortenName(picked)}"`
      : "Pick a candidate";

  return (
    <div style={{
      padding: "12px 20px",
      borderTop: `1px solid ${TOK.c.line}`,
      background: "rgba(0,0,0,0.18)",
      display: "flex", alignItems: "center", gap: 8,
    }}>
      <Btn style={{ color: TOK.c.ink2 }}>Cancel</Btn>
      <div style={{ flex: 1 }} />
      <Btn style={{
        opacity: canSkip ? 1 : 0.4, pointerEvents: canSkip ? "auto" : "none",
        color: TOK.c.ink1,
      }}>Add without save tracking</Btn>
      <Btn variant="primary" accent={TOK.c.spool}
        style={{
          minWidth: 200, height: 32, fontSize: 13,
          opacity: canTrack ? 1 : 0.4, pointerEvents: canTrack ? "auto" : "none",
        }}>
        {primaryLabel}
      </Btn>
    </div>
  );
}

function shortenName(name) {
  if (!name) return "—";
  if (name.length <= 28) return name;
  return name.slice(0, 26) + "…";
}

/* ─────────────────────────── TOUCH VARIANT ─────────────────────────── */
function AddGameV2Touch({ width = 940, height = 760 }) {
  const [picked, setPicked] = React.useState(NIGHTREIGN_CANDIDATES[0].name);

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: 14, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.08)",
      display: "flex", flexDirection: "column",
    }}>
      <div style={{
        display: "flex", alignItems: "center", gap: 16,
        height: 56, padding: "0 24px",
        background: "rgba(0,0,0,0.35)",
        borderBottom: `1px solid ${TOK.c.line}`,
      }}>
        <SpoolMark size={22} color={TOK.c.ink1} tape={TOK.c.spool} />
        <MonoLabel size={11}>SPOOL · DECK · ADD ENTRY</MonoLabel>
        <div style={{ flex: 1 }} />
        <CtrlHint glyph="B" label="Cancel" />
      </div>

      <div style={{ flex: 1, padding: "26px 36px", display: "flex", flexDirection: "column", gap: 18, minHeight: 0 }}>
        <div>
          <h1 style={{
            margin: 0, fontFamily: TOK.font.display, fontSize: 30, fontWeight: 700,
            letterSpacing: "-0.022em",
          }}>Add a game</h1>
          <p style={{ margin: "6px 0 0", fontSize: 14, color: TOK.c.ink2, lineHeight: 1.5, maxWidth: 540 }}>
            Pick the executable. Spool will identify it through ludusavi.
          </p>
        </div>

        <div style={{
          padding: "16px 18px",
          background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.md,
          display: "flex", alignItems: "center", gap: 16,
          position: "relative", overflow: "hidden",
        }}>
          <div style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 4, background: TOK.c.spool }} />
          <div style={{
            width: 52, height: 52, borderRadius: TOK.r.sm,
            background: TOK.c.bg2, border: `1px solid ${TOK.c.line2}`,
            display: "flex", alignItems: "center", justifyContent: "center",
            color: TOK.c.ink1, flexShrink: 0,
          }}>{ICN.exe}</div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontFamily: TOK.font.mono, fontSize: 15, fontWeight: 500 }}>nightreign.exe</div>
            <div style={{
              fontFamily: TOK.font.mono, fontSize: 11.5, color: TOK.c.ink3, letterSpacing: "0.04em",
              marginTop: 4, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
            }}>/home/anna/Games/EldenRingNightreign · 64.5 GB</div>
          </div>
          <Btn icon={ICN.folder} style={{ height: 40, fontSize: 13 }}>Change</Btn>
        </div>

        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          height: 44, padding: "0 14px",
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
        }}>
          <span style={{ color: TOK.c.ink2, display: "flex" }}>{ICN.search}</span>
          <input placeholder="Search ludusavi · 18 042 entries"
                 style={{
                   flex: 1, background: "transparent", border: "none", outline: "none",
                   color: TOK.c.ink0, fontFamily: TOK.font.ui, fontSize: 14.5,
                 }}/>
          <CtrlHint glyph="X" label="Search" size={18} />
        </div>

        <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
          <MonoLabel size={11} color={TOK.c.spool}>LUDUSAVI · {NIGHTREIGN_CANDIDATES.length} CANDIDATES</MonoLabel>
          <div style={{
            marginTop: 10, flex: 1, minHeight: 0, overflowY: "auto",
            background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
          }}>
            {NIGHTREIGN_CANDIDATES.map((c, i) => {
              const showScore = c.score < 95;
              const isPicked = picked === c.name;
              return (
                <button
                  key={c.name}
                  onClick={() => setPicked(c.name)}
                  style={{
                    display: "grid", gridTemplateColumns: "26px 1fr auto",
                    columnGap: 14, rowGap: 4,
                    width: "100%", padding: "14px 18px",
                    background: isPicked ? `${TOK.c.spool}14` : "transparent",
                    borderLeft: `3px solid ${isPicked ? TOK.c.spool : "transparent"}`,
                    borderBottom: i < NIGHTREIGN_CANDIDATES.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
                    border: "none",
                    textAlign: "left", color: "inherit", cursor: "pointer",
                    fontFamily: TOK.font.ui,
                  }}
                >
                  <span style={{
                    gridRow: "1 / span 2",
                    width: 22, height: 22, borderRadius: 11,
                    border: `2px solid ${isPicked ? TOK.c.spool : TOK.c.line3}`,
                    display: "inline-flex", alignItems: "center", justifyContent: "center",
                    flexShrink: 0, alignSelf: "center",
                  }}>
                    {isPicked && <span style={{ width: 10, height: 10, borderRadius: 5, background: TOK.c.spool }} />}
                  </span>
                  <span style={{
                    fontSize: 16, fontWeight: isPicked ? 500 : 400,
                    color: isPicked ? TOK.c.ink0 : TOK.c.ink1,
                  }}>{c.name}</span>
                  <span style={{ display: "inline-flex", alignItems: "center", gap: 8, flexShrink: 0, alignSelf: "center" }}>
                    {showScore && <MatchScoreLabel score={c.score} />}
                    {c.steamId && <StoreBadge kind="steam" />}
                    {c.cloudSteam && <CloudBadge />}
                  </span>
                  <span style={{
                    gridColumn: 2,
                    display: "inline-flex", alignItems: "center", gap: 6,
                    fontFamily: TOK.font.mono, fontSize: 12, color: TOK.c.ink3, letterSpacing: "0.02em",
                    whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", minWidth: 0,
                  }}>
                    <span style={{ display: "flex", color: TOK.c.ink3 }}>{ICN.folder}</span>
                    {c.savePath}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <div style={{
        padding: "12px 36px", borderTop: `1px solid ${TOK.c.line2}`,
        background: "rgba(0,0,0,0.55)",
        display: "flex", alignItems: "center", justifyContent: "space-between",
      }}>
        <div style={{ display: "flex", gap: 20 }}>
          <CtrlHint glyph="A" label="Add" />
          <CtrlHint glyph="Y" label="Add without tracking" />
          <CtrlHint glyph="X" label="Search" />
          <CtrlHint glyph="dpad" label="Pick" />
        </div>
        <CtrlHint glyph="B" label="Cancel" />
      </div>
    </div>
  );
}

Object.assign(window, {
  AddGameV2, AddGameV2Touch,
  NIGHTREIGN_CANDIDATES, OUTER_SEARCH,
  CandidateRowV2, MatchScoreLabel, StoreBadge, CloudBadge,
});
