/* Spool · Brand surfaces.
   - Identity sheet (the design system at a glance)
   - Mark variants (mark, wordmark, lockup, icon tile)
   - Splash / boot screen
   - Empty library state
*/

/* ─────────────────────────── Identity sheet ─────────────────────────── */
function IdentitySheet() {
  const swatches = [
    ["bg0", "#0b0c0e", "Window void"],
    ["bg1", "#101216", "Pane"],
    ["bg2", "#15181d", "Card"],
    ["bg3", "#1c2027", "Selection"],
    ["ink0", "#f4f4f5", "Primary ink"],
    ["ink2", "rgba(244,244,245,0.56)", "Secondary"],
    ["spool", TOK.c.spool, "Spool oxide"],
    ["ok", TOK.c.ok, "Status · ok"],
    ["warn", TOK.c.warn, "Status · warn"],
    ["info", TOK.c.info, "Status · info"],
    ["bad", TOK.c.bad, "Status · bad"],
  ];

  return (
    <div style={{
      width: 920, height: 600,
      background: TOK.c.bg0,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      display: "grid",
      gridTemplateColumns: "1fr 1fr",
      gridTemplateRows: "auto 1fr",
      gap: 0,
    }}>
      {/* header band */}
      <div style={{
        gridColumn: "1 / -1",
        padding: "22px 28px 18px",
        borderBottom: `1px solid ${TOK.c.line}`,
        display: "flex", alignItems: "center", justifyContent: "space-between",
      }}>
        <div>
          <MonoLabel size={10.5}>Cassette / cross-platform / dark-only</MonoLabel>
          <div style={{
            fontFamily: TOK.font.display,
            fontSize: 36,
            fontWeight: 600,
            letterSpacing: "-0.02em",
            marginTop: 6,
          }}>Spool design system</div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
          <SpoolMark size={48} color={TOK.c.ink0} tape={TOK.c.spool} />
          <div>
            <div style={{ fontFamily: TOK.font.display, fontSize: 22, fontWeight: 700, letterSpacing: "-0.02em" }}>Spool</div>
            <MonoLabel size={9}>v3.0 · 2026.05</MonoLabel>
          </div>
        </div>
      </div>

      {/* Type */}
      <div style={{ padding: "20px 28px", borderRight: `1px solid ${TOK.c.line}` }}>
        <MonoLabel size={10}>Type</MonoLabel>
        <div style={{ marginTop: 14, display: "flex", flexDirection: "column", gap: 14 }}>
          <TypeRow stack="Space Grotesk · display" sample="Your Library" size={28} font={TOK.font.display} weight={600} ls="-0.02em" />
          <TypeRow stack="Geist · ui · 13" sample="Restore saves before launch, back up on exit." size={13} font={TOK.font.ui} weight={400} />
          <TypeRow stack="Geist · ui · 11 · muted" sample="across all profiles · compressed on disk" size={11} font={TOK.font.ui} color={TOK.c.ink2} />
          <TypeRow stack="JetBrains Mono · label · uppercase" sample="SIDE A · CATALOG · 0012" size={10.5} font={TOK.font.mono} ls="0.16em" upper color={TOK.c.spool} />
          <TypeRow stack="JetBrains Mono · path" sample="D:\\Games\\Nightreign\\nightreign.exe" size={11.5} font={TOK.font.mono} color={TOK.c.ink1} />
        </div>

        <div style={{ marginTop: 22 }}>
          <MonoLabel size={10}>Voice</MonoLabel>
          <ul style={{
            margin: "10px 0 0", padding: 0, listStyle: "none",
            display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6,
            fontSize: 11.5, color: TOK.c.ink1,
          }}>
            <li>· Lowercase, unfussy</li>
            <li>· "Library" not "Games"</li>
            <li>· Mono for facts & ids</li>
            <li>· No exclamation marks</li>
            <li>· Verbs over nouns ("Back up", not "Backup")</li>
            <li>· Quiet, never marketing</li>
          </ul>
        </div>
      </div>

      {/* Color + components */}
      <div style={{ padding: "20px 28px" }}>
        <MonoLabel size={10}>Color</MonoLabel>
        <div style={{
          marginTop: 12,
          display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 8,
        }}>
          {swatches.map(([k, v, n]) => (
            <div key={k}>
              <div style={{
                aspectRatio: "1.4 / 1",
                background: v,
                borderRadius: TOK.r.sm,
                border: `1px solid ${TOK.c.line2}`,
              }} />
              <div style={{ fontFamily: TOK.font.mono, fontSize: 9, marginTop: 4, color: TOK.c.ink2, letterSpacing: "0.06em" }}>
                {k.toUpperCase()}
              </div>
              <div style={{ fontSize: 10.5, color: TOK.c.ink1 }}>{n}</div>
            </div>
          ))}
        </div>

        <div style={{ marginTop: 22 }}>
          <MonoLabel size={10}>Status meters</MonoLabel>
          <div style={{ marginTop: 10, display: "flex", flexWrap: "wrap", gap: 6 }}>
            <Pill kind="ok">Synced</Pill>
            <Pill kind="ok">Detected</Pill>
            <Pill kind="warn">Local newer</Pill>
            <Pill kind="info">Cloud newer</Pill>
            <Pill kind="bad">Locked elsewhere</Pill>
            <Pill kind="off">Not shared</Pill>
            <Pill kind="info" soft>v3.8.2 · LAN</Pill>
          </div>
        </div>

        <div style={{ marginTop: 18 }}>
          <MonoLabel size={10}>Buttons</MonoLabel>
          <div style={{ marginTop: 10, display: "flex", flexWrap: "wrap", gap: 6 }}>
            <Btn variant="primary" accent={TOK.c.spool} icon={ICN.play}>Play</Btn>
            <Btn variant="secondary" icon={ICN.folder}>Open folder</Btn>
            <Btn icon={ICN.cog}>Settings</Btn>
            <Btn danger icon={ICN.trash}>Remove</Btn>
          </div>
        </div>
      </div>
    </div>
  );
}

function TypeRow({ stack, sample, size, font, weight = 400, ls, upper, color }) {
  return (
    <div>
      <MonoLabel size={9}>{stack}</MonoLabel>
      <div style={{
        marginTop: 4,
        fontFamily: font,
        fontSize: size,
        fontWeight: weight,
        letterSpacing: ls,
        textTransform: upper ? "uppercase" : "none",
        color: color || TOK.c.ink0,
        lineHeight: 1.2,
      }}>{sample}</div>
    </div>
  );
}

/* ─────────────────────────── Mark variants ─────────────────────────── */
function MarkSheet() {
  return (
    <div style={{
      width: 920, height: 460,
      background: TOK.c.bg0,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      padding: "28px",
      display: "grid",
      gridTemplateColumns: "repeat(4, 1fr)",
      gap: 16,
    }}>
      {/* Mark */}
      <MarkTile label="Mark" sub="22 × 16 unit">
        <SpoolMark size={120} color={TOK.c.ink0} tape={TOK.c.spool} />
      </MarkTile>

      {/* Wordmark */}
      <MarkTile label="Wordmark">
        <div style={{
          fontFamily: TOK.font.display,
          fontSize: 56,
          fontWeight: 700,
          letterSpacing: "-0.035em",
        }}>Spool</div>
      </MarkTile>

      {/* Lockup */}
      <MarkTile label="Lockup · horizontal">
        <div style={{ display: "inline-flex", alignItems: "center", gap: 14 }}>
          <SpoolMark size={56} color={TOK.c.ink0} tape={TOK.c.spool} />
          <div style={{
            fontFamily: TOK.font.display,
            fontSize: 40,
            fontWeight: 700,
            letterSpacing: "-0.03em",
          }}>Spool</div>
        </div>
      </MarkTile>

      {/* Icon tile */}
      <MarkTile label="Icon tile · OS launchers">
        <div style={{
          width: 132, height: 132, borderRadius: 26,
          background: `linear-gradient(155deg, ${TOK.c.spoolDeep} 0%, #322820 100%)`,
          display: "flex", alignItems: "center", justifyContent: "center",
          boxShadow: "0 12px 32px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.05)",
          position: "relative",
        }}>
          <SpoolMark size={88} color={TOK.c.ink0} tape={TOK.c.spool} />
          <div style={{
            position: "absolute", bottom: 9, fontFamily: TOK.font.mono, fontSize: 7,
            letterSpacing: "0.18em", color: "rgba(255,255,255,0.4)",
          }}>SPOOL · A</div>
        </div>
      </MarkTile>

      {/* Mark · small */}
      <MarkTile label="Mark · small" sub="≤ 18px">
        <SpoolMark size={48} color={TOK.c.ink0} tape={TOK.c.spool} />
      </MarkTile>

      {/* Mono */}
      <MarkTile label="Mark · mono inverse">
        <div style={{
          background: TOK.c.ink0, padding: "16px 18px", borderRadius: TOK.r.sm,
        }}>
          <SpoolMark size={80} color={TOK.c.bg0} tape={TOK.c.bg0} />
        </div>
      </MarkTile>

      {/* Spinner motif */}
      <MarkTile label="Boot · rotating reels">
        <SpinningReels />
      </MarkTile>

      {/* Sticker / label */}
      <MarkTile label="Tape label">
        <div style={{
          width: 180, height: 90,
          background: TOK.c.spool,
          color: TOK.c.bg0,
          borderRadius: 2,
          padding: "8px 12px",
          fontFamily: TOK.font.mono,
          fontSize: 10,
          letterSpacing: "0.1em",
          textTransform: "uppercase",
          position: "relative",
          boxShadow: "0 1px 0 rgba(0,0,0,0.2)",
        }}>
          <div style={{ borderBottom: `1px solid ${TOK.c.bg0}66`, paddingBottom: 4, marginBottom: 6 }}>
            SPOOL · LIBRARY
          </div>
          <div style={{ fontFamily: TOK.font.display, fontSize: 16, textTransform: "none", letterSpacing: "-0.01em", fontWeight: 600 }}>Nightreign</div>
          <div style={{ fontSize: 8.5, opacity: 0.7, marginTop: 4 }}>SPL-0012 · 24-05-26</div>
        </div>
      </MarkTile>
    </div>
  );
}

function MarkTile({ label, sub, children }) {
  return (
    <div style={{
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line}`,
      borderRadius: TOK.r.md,
      padding: "16px 16px 14px",
      display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "space-between",
      minHeight: 180,
    }}>
      <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", width: "100%" }}>
        {children}
      </div>
      <div style={{
        marginTop: 12, width: "100%",
        borderTop: `1px dashed ${TOK.c.line}`, paddingTop: 8,
      }}>
        <MonoLabel size={9.5}>{label}</MonoLabel>
        {sub && <div style={{ fontSize: 10, color: TOK.c.ink3, marginTop: 2 }}>{sub}</div>}
      </div>
    </div>
  );
}

function SpinningReels() {
  return (
    <>
      <style>{`@keyframes spool-spin { to { transform: rotate(360deg) } }`}</style>
      <svg width="120" height="78" viewBox="0 0 120 78" fill="none">
        <rect x="2" y="2" width="116" height="74" rx="6"
              stroke={TOK.c.ink0} strokeWidth="2" fill="none" />
        {/* tape window */}
        <rect x="14" y="58" width="92" height="6" rx="1" fill={TOK.c.spool} opacity="0.8" />
        {/* reels */}
        <g style={{ transformOrigin: "36px 36px", animation: "spool-spin 1.4s linear infinite" }}>
          <circle cx="36" cy="36" r="16" stroke={TOK.c.ink0} strokeWidth="2" fill="none" />
          <circle cx="36" cy="36" r="4" fill={TOK.c.ink0} />
          <line x1="36" y1="22" x2="36" y2="32" stroke={TOK.c.ink0} strokeWidth="1.5" />
          <line x1="36" y1="40" x2="36" y2="50" stroke={TOK.c.ink0} strokeWidth="1.5" />
          <line x1="22" y1="36" x2="32" y2="36" stroke={TOK.c.ink0} strokeWidth="1.5" />
          <line x1="40" y1="36" x2="50" y2="36" stroke={TOK.c.ink0} strokeWidth="1.5" />
        </g>
        <g style={{ transformOrigin: "84px 36px", animation: "spool-spin 1.4s linear infinite" }}>
          <circle cx="84" cy="36" r="16" stroke={TOK.c.ink0} strokeWidth="2" fill="none" />
          <circle cx="84" cy="36" r="4" fill={TOK.c.ink0} />
          <line x1="84" y1="22" x2="84" y2="32" stroke={TOK.c.ink0} strokeWidth="1.5" />
          <line x1="84" y1="40" x2="84" y2="50" stroke={TOK.c.ink0} strokeWidth="1.5" />
          <line x1="70" y1="36" x2="80" y2="36" stroke={TOK.c.ink0} strokeWidth="1.5" />
          <line x1="88" y1="36" x2="98" y2="36" stroke={TOK.c.ink0} strokeWidth="1.5" />
        </g>
      </svg>
    </>
  );
}

/* ─────────────────────────── Splash / Boot ─────────────────────────── */
function SplashScreen() {
  return (
    <div style={{
      width: 600, height: 380,
      background: `radial-gradient(ellipse at 50% 40%, #1a1c22 0%, ${TOK.c.bg0} 70%)`,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      position: "relative",
      borderRadius: TOK.r.lg,
      overflow: "hidden",
      display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
      gap: 18,
    }}>
      {/* corner tape labels */}
      <div style={{ position: "absolute", top: 18, left: 22 }}>
        <MonoLabel size={9}>SPOOL · BOOT</MonoLabel>
      </div>
      <div style={{ position: "absolute", top: 18, right: 22 }}>
        <MonoLabel size={9}>v3.0.1</MonoLabel>
      </div>

      <SpinningReels />

      <div style={{
        fontFamily: TOK.font.display,
        fontSize: 32, fontWeight: 700,
        letterSpacing: "-0.025em",
      }}>Spool</div>

      <div style={{ display: "flex", alignItems: "center", gap: 10, fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.06em" }}>
        <span>Loading library</span>
        <span style={{ width: 60, height: 2, background: TOK.c.bg3, borderRadius: 1, overflow: "hidden", position: "relative" }}>
          <span style={{
            position: "absolute", left: 0, top: 0, height: "100%", width: "62%",
            background: TOK.c.spool,
          }} />
        </span>
        <span style={{ color: TOK.c.ink3 }}>14 / 22</span>
      </div>

      <div style={{
        position: "absolute", bottom: 16, left: 0, right: 0,
        textAlign: "center",
        fontFamily: TOK.font.mono, fontSize: 9, color: TOK.c.ink3, letterSpacing: "0.12em",
      }}>POWERED BY LUDUSAVI · SIDE A</div>
    </div>
  );
}

/* ─────────────────────────── Empty Library ─────────────────────────── */
function EmptyLibrary() {
  return (
    <div style={{
      width: 940, height: 560,
      background: TOK.c.bg1,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      display: "flex", flexDirection: "column",
    }}>
      <WindowChrome />
      <div style={{
        flex: 1,
        display: "grid",
        gridTemplateColumns: "320px 1fr",
      }}>
        {/* sidebar empty */}
        <div style={{ borderRight: `1px solid ${TOK.c.line}`, padding: "16px 16px", display: "flex", flexDirection: "column", gap: 10 }}>
          <div style={{
            background: TOK.c.bg2,
            border: `1px solid ${TOK.c.line}`,
            borderRadius: TOK.r.sm,
            height: 30, display: "flex", alignItems: "center", gap: 8, padding: "0 10px",
            color: TOK.c.ink3, fontSize: 12.5,
          }}>
            <span style={{ color: TOK.c.ink3, display: "flex" }}>{ICN.search}</span>
            Search 0 games
          </div>
          <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: TOK.c.ink3, fontSize: 11.5, padding: 16, textAlign: "center", lineHeight: 1.6 }}>
            <div>
              <MonoLabel size={9}>No reels loaded</MonoLabel>
              <div style={{ marginTop: 8 }}>Library entries appear here once you add a game.</div>
            </div>
          </div>
        </div>

        {/* hero */}
        <div style={{
          padding: "60px 56px",
          display: "flex", flexDirection: "column", alignItems: "flex-start", justifyContent: "center",
          gap: 22,
          position: "relative",
          overflow: "hidden",
        }}>
          {/* faint tape across background */}
          <div style={{
            position: "absolute", inset: 0,
            backgroundImage: `repeating-linear-gradient(135deg, transparent 0 18px, rgba(255,255,255,0.012) 18px 19px)`,
            pointerEvents: "none",
          }} />

          <div style={{ display: "flex", alignItems: "center", gap: 14, position: "relative" }}>
            <div style={{
              width: 60, height: 60, borderRadius: TOK.r.sm,
              background: TOK.c.bg2, border: `1px solid ${TOK.c.line2}`,
              display: "flex", alignItems: "center", justifyContent: "center",
            }}>
              <SpoolMark size={36} color={TOK.c.ink1} tape={TOK.c.spool} />
            </div>
            <div>
              <MonoLabel size={10}>Spool · empty library</MonoLabel>
              <div style={{
                fontFamily: TOK.font.display, fontSize: 32, fontWeight: 600,
                letterSpacing: "-0.02em", marginTop: 4,
              }}>Side A · blank</div>
            </div>
          </div>

          <p style={{
            fontSize: 14, lineHeight: 1.6, color: TOK.c.ink1,
            maxWidth: 460, margin: 0, textWrap: "pretty",
          }}>
            A cassette without tape is just a shell. Pick a game executable to start your library —
            Spool will identify it via ludusavi and back up saves automatically.
          </p>

          <div style={{ display: "flex", gap: 8 }}>
            <Btn variant="primary" accent={TOK.c.spool} icon={ICN.plus}>Add a game</Btn>
            <Btn icon={ICN.wifi}>Browse LAN peers</Btn>
            <Btn icon={ICN.download}>Restore from sync</Btn>
          </div>

          <div style={{ display: "flex", gap: 6, marginTop: 8, color: TOK.c.ink3, fontSize: 11.5 }}>
            <span>Or browse <span style={{ fontFamily: TOK.font.mono, color: TOK.c.ink2 }}>source feeds</span> for something new.</span>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  IdentitySheet, MarkSheet, SplashScreen, EmptyLibrary,
  TypeRow, MarkTile, SpinningReels,
});
