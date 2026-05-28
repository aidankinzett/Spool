/* Spool · Batch 2 dialogs: Manual identify · Deck in-game overlay · Errors */

/* ─────────────────────────── MANUAL IDENTIFY ───────────────────────────
   When ludusavi's auto-match fails (or you reject every candidate),
   you can search its full database manually. */

const LUDUSAVI_HITS = [
  { name: "Hollow Knight",            entries: 12, year: "2017", langs: 4, slot: "AppData/Roaming/com.Team_Cherry.HollowKnight" },
  { name: "Hollow Knight: Voidheart Edition", entries: 6,  year: "2018", langs: 2, slot: "Steam Cloud" },
  { name: "Hollow Knight: Silksong",  entries: 18, year: "2025", langs: 5, slot: "AppData/Local/SilksongTeamCherry/saves", best: true },
  { name: "Hollow Cocoon",            entries: 3,  year: "2023", langs: 2, slot: "AppData/LocalLow/MoeNovelStudio/HollowCocoon" },
  { name: "Pseudoregalia",            entries: 5,  year: "2023", langs: 1, slot: "AppData/Local/Pseudoregalia/saves" },
  { name: "Animal Well",              entries: 9,  year: "2024", langs: 3, slot: "AppData/Local/AnimalWell" },
  { name: "Blasphemous II",           entries: 7,  year: "2023", langs: 4, slot: "AppData/LocalLow/TheGameKitchen/Blasphemous2" },
];

function ManualIdentify({ width = 720, height = 640 }) {
  const [query, setQuery] = React.useState("hollow");
  const [picked, setPicked] = React.useState("Hollow Knight: Silksong");

  const list = LUDUSAVI_HITS.filter(h => h.name.toLowerCase().includes(query.toLowerCase()));
  const sel = list.find(h => h.name === picked) || list[0];

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
      display: "flex", flexDirection: "column",
    }}>
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
        <MonoLabel size={10.5} color={TOK.c.ink1}>IDENTIFY · MANUAL</MonoLabel>
        <div style={{ flex: 1 }} />
        <ChromeBtn glyph="close" />
      </div>

      {/* Header — the file we're identifying */}
      <div style={{
        padding: "16px 22px",
        borderBottom: `1px solid ${TOK.c.line}`,
      }}>
        <MonoLabel size={10}>Spool · couldn't auto-match this file</MonoLabel>
        <h1 style={{
          margin: "6px 0 4px",
          fontFamily: TOK.font.display, fontSize: 22, fontWeight: 700,
          letterSpacing: "-0.02em",
        }}>Pick the matching game from ludusavi's database.</h1>
        <p style={{ margin: 0, fontSize: 12, color: TOK.c.ink2, lineHeight: 1.5, maxWidth: 540 }}>
          Saves only get tracked once we know which entry to use. If yours isn't here, file an issue against ludusavi — it learns new games regularly.
        </p>

        <div style={{
          marginTop: 14,
          padding: "10px 14px",
          background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.md,
          display: "flex", alignItems: "center", gap: 12,
          position: "relative", overflow: "hidden",
        }}>
          <div style={{
            position: "absolute", left: 0, top: 0, bottom: 0, width: 3,
            background: TOK.c.warn,
          }} />
          <div style={{
            width: 32, height: 32, borderRadius: TOK.r.sm,
            background: TOK.c.bg2, border: `1px solid ${TOK.c.line2}`,
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            color: TOK.c.ink1,
          }}>{ICN.exe}</div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 12.5, fontWeight: 500 }}>silksong-launcher.exe</div>
            <div style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em", marginTop: 2 }}>
              D:\Games\Silksong\Bin\silksong-launcher.exe · 7.2 GB · v1.0.2 · x64
            </div>
          </div>
          <Pill kind="warn">No auto-match</Pill>
        </div>
      </div>

      {/* Search + list */}
      <div style={{ flex: 1, display: "grid", gridTemplateColumns: "260px 1fr", minHeight: 0 }}>
        <div style={{
          borderRight: `1px solid ${TOK.c.line}`,
          background: TOK.c.bg1,
          display: "flex", flexDirection: "column", minHeight: 0,
        }}>
          <div style={{ padding: "10px 12px 8px" }}>
            <div style={{
              display: "flex", alignItems: "center", gap: 8,
              height: 28, padding: "0 10px",
              background: TOK.c.bg2,
              border: `1px solid ${TOK.c.line2}`,
              borderRadius: TOK.r.sm,
            }}>
              <span style={{ color: TOK.c.ink2, display: "flex" }}>{ICN.search}</span>
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search ludusavi…"
                style={{
                  flex: 1, background: "transparent", border: "none", outline: "none",
                  color: TOK.c.ink0, fontFamily: TOK.font.ui, fontSize: 12,
                }}
              />
            </div>
            <div style={{
              marginTop: 8, fontFamily: TOK.font.mono, fontSize: 9.5,
              color: TOK.c.ink3, letterSpacing: "0.06em",
            }}>{list.length} of 18 042 entries</div>
          </div>
          <div style={{ flex: 1, overflowY: "auto" }}>
            {list.map(h => (
              <button
                key={h.name}
                onClick={() => setPicked(h.name)}
                style={{
                  display: "block", width: "100%", textAlign: "left",
                  padding: "9px 12px",
                  background: picked === h.name ? `${TOK.c.spool}10` : "transparent",
                  borderLeft: `2px solid ${picked === h.name ? TOK.c.spool : "transparent"}`,
                  border: "none", cursor: "pointer", color: "inherit",
                  fontFamily: TOK.font.ui,
                }}>
                <div style={{
                  display: "flex", alignItems: "center", gap: 6,
                  fontSize: 12.5, fontWeight: picked === h.name ? 500 : 400,
                  color: picked === h.name ? TOK.c.ink0 : TOK.c.ink1,
                }}>
                  <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{h.name}</span>
                  {h.best && <MonoLabel size={8.5} color={TOK.c.spool}>BEST</MonoLabel>}
                </div>
                <div style={{
                  fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.04em",
                  marginTop: 2,
                }}>{h.year} · {h.entries} files · {h.langs} langs</div>
              </button>
            ))}
          </div>
        </div>

        {/* Preview */}
        <div style={{ padding: "16px 22px", overflowY: "auto" }}>
          {sel ? (
            <>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{ fontFamily: TOK.font.display, fontSize: 18, fontWeight: 600, letterSpacing: "-0.012em" }}>
                  {sel.name}
                </span>
                {sel.best && <Pill kind="info" soft>Suggested</Pill>}
              </div>
              <div style={{
                fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3,
                letterSpacing: "0.04em", marginTop: 3,
              }}>{sel.year} · {sel.entries} files in manifest · {sel.langs} languages</div>

              <div style={{
                marginTop: 16, padding: "10px 12px",
                background: TOK.c.bg1, border: `1px dashed ${TOK.c.line}`, borderRadius: TOK.r.sm,
              }}>
                <MonoLabel size={9}>WILL TRACK</MonoLabel>
                <div style={{
                  marginTop: 6, display: "flex", flexDirection: "column", gap: 3,
                  fontFamily: TOK.font.mono, fontSize: 11, color: TOK.c.ink1,
                }}>
                  <span>{sel.slot}</span>
                  <span style={{ color: TOK.c.ink3 }}>%LOCALAPPDATA%/SilksongTeamCherry/settings.cfg</span>
                  <span style={{ color: TOK.c.ink3 }}>Steam Cloud · 0.4 MB</span>
                </div>
              </div>

              <div style={{
                marginTop: 14, padding: "10px 12px",
                background: "rgba(126,198,255,0.06)",
                border: `1px solid ${TOK.c.info}33`, borderRadius: TOK.r.sm,
                fontSize: 11.5, color: TOK.c.ink1, lineHeight: 1.5,
              }}>
                <span style={{ color: TOK.c.info, display: "inline-flex", verticalAlign: "middle", marginRight: 6 }}>{ICN.shield}</span>
                You can still tweak tracked paths after adding — Edit → Saves → Add override.
              </div>
            </>
          ) : (
            <div style={{
              padding: 30, textAlign: "center", color: TOK.c.ink3,
              border: `1px dashed ${TOK.c.line}`, borderRadius: TOK.r.sm,
            }}>
              <MonoLabel size={10}>No matches</MonoLabel>
              <div style={{ marginTop: 6, fontSize: 12 }}>Try a shorter query, or skip — Spool can still launch the game without save tracking.</div>
            </div>
          )}
        </div>
      </div>

      <div style={{
        padding: "12px 20px", borderTop: `1px solid ${TOK.c.line}`,
        background: "rgba(0,0,0,0.18)",
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <Btn style={{ color: TOK.c.ink2 }}>← Back</Btn>
        <div style={{ flex: 1 }} />
        <Btn icon={ICN.external}>Open ludusavi · file an issue</Btn>
        <Btn>Add without save tracking</Btn>
        <Btn variant="primary" accent={TOK.c.spool} style={{ minWidth: 140, height: 32, fontSize: 13 }}>
          Add as “{sel ? sel.name.split(":")[0] : "—"}”
        </Btn>
      </div>
    </div>
  );
}

/* ─────────────────────────── DECK IN-GAME OVERLAY ───────────────────────────
   What appears when the user holds the Steam button while a game is running.
   It's the cassette equivalent of Steam's Quick Access Menu, but tuned
   to what Spool actually knows: save status, sync state, peers. */

function DeckInGameOverlay({ width = 1280, height = 800 }) {
  const game = LIB.find(g => g.id === "elden-ring-nightreign");
  const acc = game.art.accent;
  return (
    <div style={{
      width, height, position: "relative",
      borderRadius: 14, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55)",
      background: "#000",
    }}>
      {/* Faux game frame underneath */}
      <FakeGameBackdrop game={game} />

      {/* Dim layer */}
      <div style={{
        position: "absolute", inset: 0,
        background: `linear-gradient(90deg, rgba(0,0,0,0.85) 0%, rgba(0,0,0,0.55) 60%, rgba(0,0,0,0) 100%)`,
      }} />

      {/* Side panel */}
      <aside style={{
        position: "absolute", top: 0, left: 0, bottom: 44, width: 440,
        background: "rgba(8,10,12,0.92)",
        backdropFilter: "blur(18px)",
        borderRight: `1px solid ${TOK.c.line2}`,
        display: "flex", flexDirection: "column",
      }}>
        {/* Header */}
        <div style={{
          padding: "20px 24px 14px",
          borderBottom: `1px dashed ${TOK.c.line}`,
        }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <SpoolMark size={20} color={TOK.c.ink1} tape={acc} />
            <MonoLabel size={11} color={TOK.c.ink1}>SPOOL · DECK · OVERLAY</MonoLabel>
            <div style={{ flex: 1 }} />
            <span style={{
              display: "inline-flex", alignItems: "center", gap: 6,
              fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.06em",
            }}>
              <span style={{ width: 6, height: 6, borderRadius: 3, background: TOK.c.ok, boxShadow: `0 0 8px ${TOK.c.ok}88` }} />
              RUNNING · 1h 14m
            </span>
          </div>
          <h1 style={{
            margin: "10px 0 4px",
            fontFamily: TOK.font.display, fontSize: 28, fontWeight: 700,
            letterSpacing: "-0.022em", textWrap: "balance",
          }}>{game.name}</h1>
          <div style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3, letterSpacing: "0.06em" }}>
            {game.catalog} · SIDE A · {game.art.mood.toUpperCase()}
          </div>
        </div>

        {/* Save card */}
        <div style={{ padding: "18px 24px" }}>
          <MonoLabel size={10}>Save status · ludusavi</MonoLabel>
          <div style={{
            marginTop: 8, padding: "14px 16px",
            background: TOK.c.bg1, border: `1px solid ${acc}33`, borderRadius: TOK.r.md,
          }}>
            <div style={{ display: "flex", alignItems: "center", gap: 9 }}>
              <span style={{
                width: 9, height: 9, borderRadius: 5,
                background: TOK.c.ok, boxShadow: `0 0 10px ${TOK.c.ok}88`,
              }} />
              <span style={{ fontSize: 14, fontWeight: 600 }}>Saves are current</span>
            </div>
            <div style={{ fontSize: 11.5, color: TOK.c.ink2, marginTop: 6, lineHeight: 1.5 }}>
              Last revision <strong>2 minutes ago</strong> on this device. Next auto-backup when you quit.
            </div>
            <div style={{
              marginTop: 12, display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10,
            }}>
              <DeckChip
                glyph="A"
                title="Back up now"
                sub="Snapshot before risky run"
              />
              <DeckChip
                glyph="X"
                title="Save history"
                sub="Open the timeline"
              />
            </div>
          </div>
        </div>

        {/* Devices card */}
        <div style={{ padding: "0 24px 14px" }}>
          <MonoLabel size={10}>Devices · 3 online</MonoLabel>
          <div style={{
            marginTop: 8,
            background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.md,
          }}>
            {[
              { n: "Living room · Deck", you: true,  state: "running" },
              { n: "Workshop · Desktop", state: "idle", last: "synced 2m ago" },
              { n: "Office · ThinkPad",  state: "idle", last: "synced 4h ago" },
            ].map((d, i, arr) => (
              <div key={d.n} style={{
                display: "flex", alignItems: "center", gap: 10,
                padding: "10px 12px",
                borderBottom: i < arr.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
              }}>
                <span style={{
                  width: 8, height: 8, borderRadius: 4,
                  background: d.state === "running" ? TOK.c.ok : TOK.c.ink2,
                  boxShadow: d.state === "running" ? `0 0 8px ${TOK.c.ok}88` : "none",
                }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 12.5, fontWeight: d.you ? 500 : 400 }}>
                    {d.n} {d.you && <MonoLabel size={8.5} color={TOK.c.ink3}>· YOU</MonoLabel>}
                  </div>
                  <div style={{ fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em", marginTop: 2 }}>
                    {d.state === "running" ? "PLAYING NOW · UPLOADING ON EXIT" : d.last.toUpperCase()}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>

        <div style={{ flex: 1 }} />

        {/* Quit row */}
        <div style={{
          padding: "16px 24px",
          borderTop: `1px solid ${TOK.c.line}`,
          background: "rgba(0,0,0,0.4)",
          display: "flex", alignItems: "center", gap: 8,
        }}>
          <DeckChip glyph="B" title="Resume" sub="Back to game" small />
          <div style={{ flex: 1 }} />
          <Btn danger icon={ICN.close} style={{ height: 32, fontSize: 12.5 }}>Save & quit</Btn>
        </div>
      </aside>

      {/* Right side: quick performance + tips */}
      <aside style={{
        position: "absolute", top: 24, right: 24, width: 280,
        display: "flex", flexDirection: "column", gap: 12,
      }}>
        <PerfTile />
        <TipTile acc={acc} />
      </aside>

      {/* Bottom hint bar */}
      <DeckOverlayHints />
    </div>
  );
}

function FakeGameBackdrop({ game }) {
  /* Cinematic faux-screenshot — no real assets */
  return (
    <div style={{
      position: "absolute", inset: 0,
      background: `linear-gradient(180deg, ${game.art.from} 0%, ${game.art.to} 100%)`,
      overflow: "hidden",
    }}>
      <div style={{
        position: "absolute", inset: "-10% -5% 30%",
        background: `radial-gradient(circle at 30% 60%, ${game.art.accent}55, transparent 60%)`,
        filter: "blur(40px)",
      }} />
      <div style={{
        position: "absolute", bottom: "30%", right: "10%",
        width: 200, height: 200, borderRadius: "50%",
        background: `radial-gradient(circle, ${game.art.accent}, transparent 70%)`,
        filter: "blur(50px)", opacity: 0.6,
      }} />
      <div style={{
        position: "absolute", inset: 0,
        backgroundImage: "radial-gradient(rgba(255,255,255,0.05) 1px, transparent 1px)",
        backgroundSize: "3px 3px",
        opacity: 0.4, mixBlendMode: "overlay",
      }} />
      <div style={{
        position: "absolute", inset: 0,
        background: `linear-gradient(180deg, transparent 60%, rgba(0,0,0,0.5) 100%)`,
      }} />
    </div>
  );
}

function DeckChip({ glyph, title, sub, small }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 10,
      padding: small ? "6px 10px" : "10px 12px",
      background: "rgba(255,255,255,0.04)",
      border: `1px solid ${TOK.c.line2}`,
      borderRadius: TOK.r.md,
    }}>
      <CtrlGlyph kind={glyph} size={small ? 22 : 26} />
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: small ? 12 : 13, fontWeight: 500, lineHeight: 1.2 }}>{title}</div>
        {sub && <div style={{ fontSize: 10.5, color: TOK.c.ink3, marginTop: 1 }}>{sub}</div>}
      </div>
    </div>
  );
}

function PerfTile() {
  const bars = [62, 58, 60, 55, 59, 60, 60, 58, 61, 60, 60, 62, 60, 59, 60, 60, 60, 60, 58, 60];
  return (
    <div style={{
      background: "rgba(8,10,12,0.85)",
      border: `1px solid ${TOK.c.line2}`,
      borderRadius: TOK.r.md,
      padding: "12px 14px",
      backdropFilter: "blur(18px)",
    }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <MonoLabel size={10}>Performance</MonoLabel>
        <Pill kind="ok">60 FPS</Pill>
      </div>
      <div style={{
        marginTop: 10, display: "flex", alignItems: "flex-end", gap: 2, height: 28,
      }}>
        {bars.map((b, i) => (
          <span key={i} style={{
            flex: 1, height: `${b}%`,
            background: TOK.c.ok,
            opacity: 0.6 + (i / bars.length) * 0.4,
            borderRadius: 1,
          }} />
        ))}
      </div>
      <div style={{
        marginTop: 8, display: "grid", gridTemplateColumns: "1fr 1fr 1fr",
        gap: 8, fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink2, letterSpacing: "0.04em",
      }}>
        <div><div style={{ color: TOK.c.ink3 }}>BATTERY</div><div style={{ color: TOK.c.ink0, marginTop: 2 }}>71%</div></div>
        <div><div style={{ color: TOK.c.ink3 }}>TDP</div><div style={{ color: TOK.c.ink0, marginTop: 2 }}>9.2W</div></div>
        <div><div style={{ color: TOK.c.ink3 }}>TEMP</div><div style={{ color: TOK.c.ink0, marginTop: 2 }}>62°C</div></div>
      </div>
    </div>
  );
}

function TipTile({ acc }) {
  return (
    <div style={{
      background: `linear-gradient(155deg, ${acc}22 0%, rgba(8,10,12,0.85) 100%)`,
      border: `1px solid ${acc}44`,
      borderRadius: TOK.r.md,
      padding: "12px 14px",
      backdropFilter: "blur(18px)",
    }}>
      <MonoLabel size={10} color={acc}>{ICN.wifi} PEER ACTIVITY</MonoLabel>
      <div style={{ marginTop: 8, fontSize: 12, color: TOK.c.ink1, lineHeight: 1.45 }}>
        Workshop · Desktop just pushed a new save 2 minutes ago. Next launch will pick up their progress.
      </div>
    </div>
  );
}

function DeckOverlayHints() {
  return (
    <div style={{
      position: "absolute", left: 0, right: 0, bottom: 0,
      height: 44, padding: "0 32px",
      display: "flex", alignItems: "center", justifyContent: "space-between",
      background: "rgba(0,0,0,0.78)",
      borderTop: `1px solid ${TOK.c.line2}`,
      backdropFilter: "blur(12px)",
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 22 }}>
        <CtrlHint glyph="A" label="Back up" />
        <CtrlHint glyph="X" label="History" />
        <CtrlHint glyph="Y" label="Open in Spool" />
        <CtrlHint glyph="lstick" label="Navigate" />
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 22 }}>
        <CtrlHint glyph="steam" label="Close overlay" />
        <CtrlHint glyph="B" label="Resume" />
      </div>
    </div>
  );
}

/* ─────────────────────────── ERROR STATES ───────────────────────────
   Four critical errors as inline banners — these appear in their
   natural contexts (library top, sync card, panel, etc.) */

function ErrorStatesGrid() {
  return (
    <div style={{
      width: 1100, height: 760,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      padding: 28,
      display: "grid",
      gridTemplateColumns: "1fr 1fr",
      gridTemplateRows: "auto auto",
      gap: 22,
    }}>
      <ErrorScene
        kicker="LIBRARY"
        title="Ludusavi · missing"
        sub="No saves can be tracked until you point Spool at the executable."
      >
        <LudusaviMissingBanner />
      </ErrorScene>

      <ErrorScene
        kicker="LIBRARY DETAIL"
        title="Sync server · unreachable"
        sub="Surface near the sync indicator on the title bar."
      >
        <SyncOfflineBanner />
      </ErrorScene>

      <ErrorScene
        kicker="DOWNLOAD PANEL"
        title="Disk · full"
        sub="Halts in-flight downloads until reclaimed."
      >
        <DiskFullBanner />
      </ErrorScene>

      <ErrorScene
        kicker="LAN PEER"
        title="Peer · auth failed"
        sub="Peer wants a pairing code before sharing this game."
      >
        <PeerAuthBanner />
      </ErrorScene>
    </div>
  );
}

function ErrorScene({ kicker, title, sub, children }) {
  return (
    <div>
      <div style={{ marginBottom: 8 }}>
        <MonoLabel size={9.5}>{kicker}</MonoLabel>
        <div style={{ fontSize: 13.5, fontWeight: 600, marginTop: 3 }}>{title}</div>
        <div style={{ fontSize: 11.5, color: TOK.c.ink3, marginTop: 2 }}>{sub}</div>
      </div>
      <div>{children}</div>
    </div>
  );
}

function ErrorBanner({ kind = "bad", kicker, title, blurb, primary, secondary, extras }) {
  const tape = kind === "bad" ? TOK.c.bad : kind === "warn" ? TOK.c.warn : TOK.c.info;
  return (
    <div style={{
      background: TOK.c.bg1,
      border: `1px solid ${tape}55`,
      borderRadius: TOK.r.md, overflow: "hidden",
      display: "flex",
      boxShadow: `0 0 0 1px ${tape}11`,
    }}>
      <div style={{
        width: 4, alignSelf: "stretch", background: tape,
      }} />
      <div style={{ padding: "14px 16px 14px 16px", flex: 1 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            width: 22, height: 22, borderRadius: 11,
            background: `${tape}22`, color: tape,
          }}>
            <I d="M8 4v5M8 11h.01M8 1.5 14.5 14h-13L8 1.5Z" stroke={1.5} size={13} />
          </span>
          <MonoLabel size={10} color={tape}>{kicker}</MonoLabel>
        </div>
        <div style={{
          marginTop: 8, fontFamily: TOK.font.display, fontSize: 18, fontWeight: 600,
          letterSpacing: "-0.012em",
        }}>{title}</div>
        <p style={{ margin: "4px 0 12px", fontSize: 12, color: TOK.c.ink2, lineHeight: 1.5 }}>
          {blurb}
        </p>

        {extras && <div style={{ marginBottom: 12 }}>{extras}</div>}

        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          {primary}
          {secondary}
        </div>
      </div>
    </div>
  );
}

function LudusaviMissingBanner() {
  return (
    <ErrorBanner
      kind="bad"
      kicker="LUDUSAVI · NOT DETECTED"
      title="Spool needs ludusavi to back up your saves."
      blurb="Without it, games will still launch — but no save revisions are kept. Point Spool at the executable to fix."
      extras={
        <div style={{
          padding: "8px 12px",
          background: TOK.c.bg0,
          border: `1px dashed ${TOK.c.line2}`, borderRadius: TOK.r.sm,
          display: "flex", alignItems: "center", gap: 8,
          fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3, letterSpacing: "0.04em",
        }}>
          <span>Last attempt:</span>
          <span style={{ color: TOK.c.bad }}>which ludusavi → not found</span>
        </div>
      }
      primary={<Btn variant="primary" accent={TOK.c.spool} icon={ICN.folder} style={{ height: 30 }}>Browse for ludusavi</Btn>}
      secondary={<Btn icon={ICN.external} style={{ height: 30 }}>Install instructions</Btn>}
    />
  );
}

function SyncOfflineBanner() {
  return (
    <ErrorBanner
      kind="warn"
      kicker="SYNC · OFFLINE · 4 MIN"
      title="Couldn't reach the sync server."
      blurb="Saves are still backed up locally. Retrying every 30 seconds. Other devices won't see your latest revisions until this clears."
      extras={
        <div style={{
          padding: "8px 12px",
          background: TOK.c.bg0,
          border: `1px dashed ${TOK.c.line2}`, borderRadius: TOK.r.sm,
          fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3, letterSpacing: "0.04em",
          display: "flex", flexDirection: "column", gap: 2,
        }}>
          <span><span style={{ color: TOK.c.ink2 }}>GET</span> http://nas.local:47633/v1/ping → <span style={{ color: TOK.c.bad }}>timeout (5 s)</span></span>
          <span><span style={{ color: TOK.c.ink2 }}>Retry</span> in 26 s · 8 attempts</span>
        </div>
      }
      primary={<Btn variant="primary" accent={TOK.c.spool} style={{ height: 30 }}>Retry now</Btn>}
      secondary={<Btn icon={ICN.cog} style={{ height: 30 }}>Diagnose</Btn>}
    />
  );
}

function DiskFullBanner() {
  return (
    <ErrorBanner
      kind="bad"
      kicker="DOWNLOADS · PAUSED"
      title="Out of room on D:\\"
      blurb="Two downloads paused. Free up 8.2 GB or change the install directory to continue."
      extras={
        <div>
          <div style={{
            display: "flex", justifyContent: "space-between",
            fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em",
            marginBottom: 5,
          }}>
            <span>D:\ · 922.4 / 931.0 GB used</span>
            <span style={{ color: TOK.c.bad }}>99%</span>
          </div>
          <div style={{
            height: 4, background: TOK.c.bg0, borderRadius: 2, overflow: "hidden",
          }}>
            <div style={{ height: "100%", width: "99%", background: TOK.c.bad }} />
          </div>
        </div>
      }
      primary={<Btn variant="primary" accent={TOK.c.spool} icon={ICN.trash} style={{ height: 30 }}>Reclaim space…</Btn>}
      secondary={<Btn icon={ICN.folder} style={{ height: 30 }}>Change install dir</Btn>}
    />
  );
}

function PeerAuthBanner() {
  return (
    <ErrorBanner
      kind="warn"
      kicker="PEER · PAIRING REQUIRED"
      title="Office · ThinkPad wants a pairing code."
      blurb="This device hasn't shared with you before. Enter the 6-digit code shown on the peer to authorize this and future transfers."
      extras={
        <div style={{
          display: "flex", alignItems: "center", gap: 8,
        }}>
          {Array.from({ length: 6 }).map((_, i) => (
            <span key={i} style={{
              width: 36, height: 44,
              background: TOK.c.bg0,
              border: `1px solid ${i < 3 ? TOK.c.spool : TOK.c.line2}`,
              borderRadius: TOK.r.sm,
              display: "inline-flex", alignItems: "center", justifyContent: "center",
              fontFamily: TOK.font.mono, fontSize: 22, fontWeight: 600,
              color: i < 3 ? TOK.c.ink0 : TOK.c.ink3,
            }}>{i < 3 ? ["4","8","2"][i] : "·"}</span>
          ))}
        </div>
      }
      primary={<Btn variant="primary" accent={TOK.c.spool} style={{ height: 30 }}>Pair</Btn>}
      secondary={<Btn style={{ height: 30 }}>Reject</Btn>}
    />
  );
}

Object.assign(window, {
  LUDUSAVI_HITS,
  ManualIdentify,
  DeckInGameOverlay,
  ErrorStatesGrid,
  ErrorBanner,
  LudusaviMissingBanner, SyncOfflineBanner, DiskFullBanner, PeerAuthBanner,
});
