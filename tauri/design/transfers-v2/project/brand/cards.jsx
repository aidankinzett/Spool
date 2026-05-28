/* Artboard contents for each branding direction. */

const COLORS = {
  ink: "#18181b",
  inkSoft: "#3f3f46",
  inkMute: "#71717a",
  paper: "#fafaf9",
  paperWarm: "#f6f5f3",
  paperLine: "#e7e5e1",
  dark: "#161618",
  darkSoft: "#1f1f22",
  darkLine: "#2a2a2e",
  /* Sample Windows 11 system accents */
  win11Blue: "#0078d4",
  win11Yellow: "#ce8a00",
  win11Plum: "#a64191",
  win11Mint: "#118c46",
  win11Orange: "#d97706",
};

/* ── Identity card: wordmark + mark + tagline + voice + angle ──────────── */
function IdentityCard({ dir }) {
  const { name, Mark, tagline, voice, angle } = dir;
  return (
    <div style={{
      width: "100%", height: "100%",
      padding: "36px 40px 30px",
      background: COLORS.paper,
      display: "flex",
      flexDirection: "column",
      gap: 0,
      boxSizing: "border-box",
    }}>
      {/* Mark + wordmark lockup */}
      <div style={{ display: "flex", alignItems: "center", gap: 18 }}>
        <Mark size={64} fg={COLORS.ink} bg="none" />
        <Wordmark name={name} fg={COLORS.ink} size={64} />
      </div>

      {/* Tagline */}
      <div style={{
        marginTop: 28,
        fontFamily: 'var(--font-display)',
        fontSize: 22,
        fontWeight: 400,
        color: COLORS.inkSoft,
        letterSpacing: "-0.01em",
        lineHeight: 1.25,
        textWrap: "pretty",
      }}>
        {tagline}
      </div>

      {/* Angle note */}
      <div style={{
        marginTop: 18,
        fontSize: 12.5,
        color: COLORS.inkMute,
        lineHeight: 1.5,
        textWrap: "pretty",
      }}>
        {angle}
      </div>

      <div style={{ flex: 1 }} />

      {/* Voice samples */}
      <div style={{
        borderTop: `1px solid ${COLORS.paperLine}`,
        paddingTop: 14,
        display: "flex",
        flexDirection: "column",
        gap: 5,
      }}>
        <div style={{
          fontSize: 10.5,
          textTransform: "uppercase",
          letterSpacing: "0.12em",
          color: COLORS.inkMute,
          fontWeight: 600,
          marginBottom: 4,
        }}>Voice samples</div>
        {voice.map((line, i) => (
          <div key={i} style={{
            fontFamily: 'var(--font-mono)',
            fontSize: 12,
            color: COLORS.inkSoft,
            lineHeight: 1.55,
          }}>
            <span style={{ color: COLORS.inkMute }}>›</span> {line}
          </div>
        ))}
      </div>
    </div>
  );
}

/* ── Marks card: tile, app icon, taskbar 32, favicon 16; light + dark.
       Plus the "monogram only" naked geometry. ────────────────────────── */
function MarksCard({ dir }) {
  const { Mark } = dir;
  return (
    <div style={{
      width: "100%", height: "100%",
      background: COLORS.paper,
      padding: "32px 36px",
      display: "flex",
      flexDirection: "column",
      gap: 24,
      boxSizing: "border-box",
    }}>
      {/* Row 1 — flagship tile on light */}
      <div style={{ display: "flex", alignItems: "center", gap: 26 }}>
        <div style={{
          width: 140, height: 140,
          background: COLORS.ink,
          borderRadius: 28,
          display: "flex", alignItems: "center", justifyContent: "center",
          boxShadow: "0 2px 6px rgba(0,0,0,0.04), 0 12px 32px rgba(0,0,0,0.07)",
        }}>
          <Mark size={92} fg={COLORS.paper} bg="none" />
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          {/* app icon 56 */}
          <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
            <div style={{
              width: 56, height: 56,
              background: COLORS.ink,
              borderRadius: 12,
              display: "flex", alignItems: "center", justifyContent: "center",
            }}>
              <Mark size={38} fg={COLORS.paper} bg="none" />
            </div>
            <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10.5, color: COLORS.inkMute }}>
              56 px — app icon
            </span>
          </div>
          {/* taskbar 32 */}
          <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
            <div style={{
              width: 32, height: 32,
              background: COLORS.ink,
              borderRadius: 7,
              display: "flex", alignItems: "center", justifyContent: "center",
            }}>
              <Mark size={22} fg={COLORS.paper} bg="none" />
            </div>
            <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10.5, color: COLORS.inkMute }}>
              32 px — taskbar / dock
            </span>
          </div>
          {/* favicon 16 */}
          <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
            <div style={{
              width: 16, height: 16,
              background: COLORS.ink,
              borderRadius: 3.5,
              display: "flex", alignItems: "center", justifyContent: "center",
            }}>
              <Mark size={12} fg={COLORS.paper} bg="none" />
            </div>
            <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10.5, color: COLORS.inkMute }}>
              16 px — favicon / tray
            </span>
          </div>
        </div>
      </div>

      {/* Row 2 — light tile + inverted on dark surface (Steam Deck context) */}
      <div style={{ display: "flex", gap: 14, alignItems: "stretch" }}>
        {/* Bright tile (paper bg, ink mark) */}
        <div style={{
          flex: 1,
          background: COLORS.paperWarm,
          border: `1px solid ${COLORS.paperLine}`,
          borderRadius: 14,
          padding: "16px 14px",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 8,
        }}>
          <div style={{
            width: 72, height: 72,
            background: COLORS.paper,
            border: `1px solid ${COLORS.paperLine}`,
            borderRadius: 16,
            display: "flex", alignItems: "center", justifyContent: "center",
          }}>
            <Mark size={48} fg={COLORS.ink} bg="none" />
          </div>
          <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10, color: COLORS.inkMute }}>
            light
          </span>
        </div>

        {/* Dark Steam Deck-ish tile */}
        <div style={{
          flex: 1,
          background: "#0d1116",
          borderRadius: 14,
          padding: "16px 14px",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 8,
        }}>
          <div style={{
            width: 72, height: 72,
            background: "#1b2129",
            borderRadius: 16,
            display: "flex", alignItems: "center", justifyContent: "center",
          }}>
            <Mark size={48} fg="#eaeaea" bg="none" />
          </div>
          <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10, color: "rgba(255,255,255,0.4)" }}>
            handheld tile
          </span>
        </div>

        {/* Win11 accent variants — same mark, accent fill */}
        <div style={{
          flex: 1.2,
          background: COLORS.paperWarm,
          border: `1px solid ${COLORS.paperLine}`,
          borderRadius: 14,
          padding: "16px 14px",
          display: "flex",
          flexDirection: "column",
          gap: 10,
        }}>
          <div style={{
            display: "flex", gap: 8, justifyContent: "center",
          }}>
            {[COLORS.win11Blue, COLORS.win11Yellow, COLORS.win11Plum, COLORS.win11Mint].map((c, i) => (
              <div key={i} style={{
                width: 40, height: 40,
                background: c,
                borderRadius: 9,
                display: "flex", alignItems: "center", justifyContent: "center",
              }}>
                <Mark size={26} fg="#fff" bg="none" />
              </div>
            ))}
          </div>
          <span style={{
            fontFamily: 'var(--font-mono)', fontSize: 10, color: COLORS.inkMute,
            textAlign: "center",
          }}>
            picks up Win11 accent
          </span>
        </div>
      </div>
    </div>
  );
}

/* ── In-Context card: titlebar + a slice of library content ───────────── */
function InContextCard({ dir }) {
  const { Mark, name } = dir;
  const games = window.LIBRARY.slice(0, 3);
  const accent = COLORS.win11Blue;

  return (
    <div style={{
      width: "100%", height: "100%",
      background: "#0d1116",
      borderRadius: 0,
      display: "flex",
      flexDirection: "column",
      overflow: "hidden",
      boxSizing: "border-box",
    }}>
      {/* Titlebar */}
      <div style={{
        height: 38,
        display: "flex",
        alignItems: "center",
        padding: "0 14px",
        borderBottom: "1px solid rgba(255,255,255,0.04)",
        background: "rgba(255,255,255,0.02)",
        gap: 10,
      }}>
        <Mark size={18} fg="#ffffff" bg="none" />
        <span style={{
          fontFamily: 'var(--font-display)',
          fontSize: 13,
          fontWeight: 600,
          color: "#fff",
          letterSpacing: "-0.01em",
        }}>{name}</span>
        <span style={{
          fontFamily: 'var(--font)',
          fontSize: 11,
          color: "rgba(255,255,255,0.4)",
          marginLeft: 2,
        }}>
          — Library
        </span>
        <div style={{ flex: 1 }} />
        {/* window controls */}
        <div style={{ display: "flex", gap: 12, marginLeft: 8 }}>
          {["minimize", "max", "close"].map((k) => (
            <div key={k} style={{
              width: 11, height: 11,
              border: "1px solid rgba(255,255,255,0.25)",
              borderRadius: k === "minimize" ? 0 : 1,
              ...(k === "minimize" ? { height: 0, borderTop: "none", borderLeft: "none", borderRight: "none" } : {}),
            }} />
          ))}
        </div>
      </div>

      {/* Content area */}
      <div style={{
        flex: 1,
        padding: "20px 18px",
        display: "flex",
        flexDirection: "column",
        gap: 14,
        minHeight: 0,
      }}>
        {/* Section header */}
        <div style={{ display: "flex", alignItems: "baseline", gap: 10 }}>
          <span style={{
            fontFamily: 'var(--font-display)',
            fontSize: 17,
            fontWeight: 600,
            color: "#fff",
            letterSpacing: "-0.01em",
          }}>Recently played</span>
          <span style={{
            fontSize: 11,
            color: "rgba(255,255,255,0.45)",
          }}>
            3 of 12 {dir.sample}
          </span>
        </div>

        {/* Mini card row */}
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 12 }}>
          {games.map((g) => (
            <div key={g.id} style={{
              borderRadius: 6,
              overflow: "hidden",
              background: "rgba(255,255,255,0.03)",
              border: "1px solid rgba(255,255,255,0.05)",
            }}>
              {/* cover */}
              <div style={{
                aspectRatio: "3/4",
                background: `linear-gradient(155deg, ${g.art.from} 0%, ${g.art.to} 100%)`,
                position: "relative",
              }}>
                <div style={{
                  position: "absolute",
                  top: -10, right: -16,
                  width: 80, height: 80,
                  borderRadius: "50%",
                  background: `radial-gradient(circle, ${g.art.accent}66, transparent 70%)`,
                }}/>
                <div style={{
                  position: "absolute",
                  bottom: 6, left: 8, right: 8,
                  fontSize: 9.5,
                  fontWeight: 600,
                  color: "#fff",
                  textShadow: "0 1px 2px rgba(0,0,0,0.6)",
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}>{g.short}</div>
              </div>
            </div>
          ))}
        </div>

        {/* Toast / status line — uses voice */}
        <div style={{
          marginTop: "auto",
          background: "rgba(255,255,255,0.04)",
          border: "1px solid rgba(255,255,255,0.06)",
          borderRadius: 6,
          padding: "8px 11px",
          display: "flex",
          alignItems: "center",
          gap: 9,
        }}>
          <div style={{
            width: 6, height: 6, borderRadius: "50%",
            background: accent,
            boxShadow: `0 0 0 3px ${accent}33`,
          }}/>
          <span style={{
            fontFamily: 'var(--font-mono)',
            fontSize: 11,
            color: "rgba(255,255,255,0.78)",
          }}>{dir.voice[0]}</span>
          <div style={{ flex: 1 }} />
          <span style={{
            fontSize: 10,
            color: "rgba(255,255,255,0.35)",
            fontFamily: 'var(--font-mono)',
          }}>just now</span>
        </div>
      </div>
    </div>
  );
}

/* ── Intro / system card (sits at top of canvas) ──────────────────────── */
function IntroCard() {
  return (
    <div style={{
      width: "100%", height: "100%",
      background: COLORS.paper,
      padding: "40px 44px",
      display: "flex",
      flexDirection: "column",
      gap: 18,
      boxSizing: "border-box",
    }}>
      <div style={{
        fontSize: 10.5,
        textTransform: "uppercase",
        letterSpacing: "0.18em",
        color: COLORS.inkMute,
        fontWeight: 600,
      }}>Brief</div>

      <div style={{
        fontFamily: 'var(--font-display)',
        fontSize: 32,
        fontWeight: 700,
        letterSpacing: "-0.025em",
        lineHeight: 1.1,
        color: COLORS.ink,
        textWrap: "balance",
      }}>
        Rebrand the wrapper.
      </div>

      <div style={{
        fontSize: 14,
        lineHeight: 1.55,
        color: COLORS.inkSoft,
        textWrap: "pretty",
        maxWidth: 620,
      }}>
        The app has grown past being a Ludusavi wrapper: it's now a personal
        game shelf with cover art, LAN sharing, cross-device save lock, and a
        handheld-first launcher. Five candidate identities below — each
        emphasises a different facet. Type chrome stays neutral so cover art
        leads; the icon is a single-colour monogram that picks up the
        Windows 11 accent and reads at 16 px.
      </div>

      <div style={{ flex: 1 }} />

      <div style={{
        display: "flex",
        gap: 22,
        borderTop: `1px solid ${COLORS.paperLine}`,
        paddingTop: 16,
      }}>
        {[
          ["Type", "Segoe UI Variable"],
          ["Ink", "#18181b"],
          ["Paper", "#fafaf9"],
          ["Accent", "system"],
          ["Mark grid", "64\u00d764, 1-color"],
        ].map(([k, v], i) => (
          <div key={i} style={{ display: "flex", flexDirection: "column", gap: 3 }}>
            <span style={{
              fontSize: 9.5,
              textTransform: "uppercase",
              letterSpacing: "0.12em",
              color: COLORS.inkMute,
              fontWeight: 600,
            }}>{k}</span>
            <span style={{
              fontFamily: 'var(--font-mono)',
              fontSize: 11.5,
              color: COLORS.ink,
            }}>{v}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

Object.assign(window, { IdentityCard, MarksCard, InContextCard, IntroCard, COLORS });

/* ── Spool variant studies ──────────────────────────────────────────── */

/* Grid: 8 variants on tiles + names + notes */
function SpoolGridCard() {
  return (
    <div style={{
      width: "100%", height: "100%",
      background: COLORS.paper,
      padding: "30px 32px",
      boxSizing: "border-box",
      display: "flex",
      flexDirection: "column",
      gap: 18,
    }}>
      <div>
        <div style={{
          fontSize: 10.5,
          textTransform: "uppercase",
          letterSpacing: "0.18em",
          color: COLORS.inkMute,
          fontWeight: 600,
        }}>Spool · alternates</div>
        <div style={{
          fontFamily: 'var(--font-display)',
          fontSize: 22,
          fontWeight: 600,
          color: COLORS.ink,
          letterSpacing: "-0.02em",
          marginTop: 4,
        }}>Eight takes on the same idea</div>
      </div>

      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(4, 1fr)",
        gap: 16,
        flex: 1,
      }}>
        {SPOOL_VARIANTS.map((v) => {
          const { Mark } = v;
          return (
            <div key={v.id} style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "stretch",
              gap: 8,
            }}>
              <div style={{
                width: "100%",
                aspectRatio: "1 / 1",
                background: COLORS.ink,
                borderRadius: 16,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}>
                <Mark size={68} fg={COLORS.paper} bg="none" />
              </div>
              <div style={{
                fontFamily: 'var(--font-display)',
                fontSize: 12.5,
                fontWeight: 600,
                color: COLORS.ink,
                letterSpacing: "-0.01em",
              }}>{v.name}</div>
              <div style={{
                fontSize: 10.5,
                color: COLORS.inkMute,
                lineHeight: 1.45,
                textWrap: "pretty",
              }}>{v.note}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* Tiny-size legibility — each variant rendered at 32 / 20 / 16, in a row,
   next to a windowed title for taskbar comparison. */
function SpoolTinyCard() {
  return (
    <div style={{
      width: "100%", height: "100%",
      background: COLORS.paper,
      padding: "30px 36px",
      boxSizing: "border-box",
      display: "flex",
      flexDirection: "column",
      gap: 20,
    }}>
      <div>
        <div style={{
          fontSize: 10.5,
          textTransform: "uppercase",
          letterSpacing: "0.18em",
          color: COLORS.inkMute,
          fontWeight: 600,
        }}>Tiny-size test</div>
        <div style={{
          fontFamily: 'var(--font-display)',
          fontSize: 20,
          fontWeight: 600,
          color: COLORS.ink,
          letterSpacing: "-0.02em",
          marginTop: 4,
        }}>Does it hold up at 16 px?</div>
      </div>

      {/* Column headers */}
      <div style={{
        display: "grid",
        gridTemplateColumns: "100px 48px 36px 28px 1fr",
        alignItems: "center",
        gap: 16,
        fontSize: 9.5,
        textTransform: "uppercase",
        letterSpacing: "0.12em",
        color: COLORS.inkMute,
        fontWeight: 600,
        paddingBottom: 4,
        borderBottom: `1px solid ${COLORS.paperLine}`,
      }}>
        <div>Variant</div>
        <div style={{ textAlign: "center" }}>32</div>
        <div style={{ textAlign: "center" }}>20</div>
        <div style={{ textAlign: "center" }}>16</div>
        <div>Taskbar mock</div>
      </div>

      {SPOOL_VARIANTS.map((v) => {
        const { Mark } = v;
        return (
          <div key={v.id} style={{
            display: "grid",
            gridTemplateColumns: "100px 48px 36px 28px 1fr",
            alignItems: "center",
            gap: 16,
            fontSize: 12,
            color: COLORS.inkSoft,
          }}>
            <div style={{ fontWeight: 500, color: COLORS.ink }}>{v.name}</div>
            <div style={{ display: "flex", justifyContent: "center" }}>
              <Mark size={32} fg={COLORS.ink} bg="none" />
            </div>
            <div style={{ display: "flex", justifyContent: "center" }}>
              <Mark size={20} fg={COLORS.ink} bg="none" />
            </div>
            <div style={{ display: "flex", justifyContent: "center" }}>
              <Mark size={16} fg={COLORS.ink} bg="none" />
            </div>
            <div style={{
              background: "#0d1116",
              borderRadius: 6,
              padding: "4px 8px",
              display: "flex",
              alignItems: "center",
              gap: 8,
              height: 26,
              boxSizing: "border-box",
            }}>
              <Mark size={14} fg="#ffffff" bg="none" />
              <span style={{
                fontSize: 10.5,
                color: "rgba(255,255,255,0.85)",
                fontFamily: 'var(--font)',
                fontWeight: 500,
              }}>Spool</span>
              <span style={{
                fontSize: 10,
                color: "rgba(255,255,255,0.4)",
              }}>{"\u2014 Library"}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

/* Win11 accent contexts — each variant on the four shared system accents. */
function SpoolAccentCard() {
  const accents = [COLORS.win11Blue, COLORS.win11Yellow, COLORS.win11Plum, COLORS.win11Mint];
  return (
    <div style={{
      width: "100%", height: "100%",
      background: COLORS.paper,
      padding: "30px 32px",
      boxSizing: "border-box",
      display: "flex",
      flexDirection: "column",
      gap: 16,
    }}>
      <div>
        <div style={{
          fontSize: 10.5,
          textTransform: "uppercase",
          letterSpacing: "0.18em",
          color: COLORS.inkMute,
          fontWeight: 600,
        }}>Accent test</div>
        <div style={{
          fontFamily: 'var(--font-display)',
          fontSize: 20,
          fontWeight: 600,
          color: COLORS.ink,
          letterSpacing: "-0.02em",
          marginTop: 4,
        }}>Picking up Win11 accent</div>
      </div>

      <div style={{
        display: "flex",
        flexDirection: "column",
        gap: 10,
        flex: 1,
      }}>
        {SPOOL_VARIANTS.map((v) => {
          const { Mark } = v;
          return (
            <div key={v.id} style={{
              display: "grid",
              gridTemplateColumns: "82px repeat(4, 1fr)",
              alignItems: "center",
              gap: 10,
            }}>
              <div style={{
                fontSize: 11.5,
                fontWeight: 500,
                color: COLORS.ink,
              }}>{v.name}</div>
              {accents.map((c, i) => (
                <div key={i} style={{
                  aspectRatio: "1 / 1",
                  background: c,
                  borderRadius: 9,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}>
                  <Mark size={32} fg="#ffffff" bg="none" />
                </div>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}

Object.assign(window, { SpoolGridCard, SpoolTinyCard, SpoolAccentCard });

/* ── Reel-to-reel & Cassette refinements ────────────────────────────── */

const ALL_REELS = [...REEL_ORIGINALS, ...REEL_REFINEMENTS];

function ReelGridCard() {
  return (
    <div style={{
      width: "100%", height: "100%",
      background: COLORS.paper,
      padding: "30px 32px",
      boxSizing: "border-box",
      display: "flex",
      flexDirection: "column",
      gap: 18,
    }}>
      <div>
        <div style={{
          fontSize: 10.5,
          textTransform: "uppercase",
          letterSpacing: "0.18em",
          color: COLORS.inkMute,
          fontWeight: 600,
        }}>Reel-to-reel · refinements</div>
        <div style={{
          fontFamily: 'var(--font-display)',
          fontSize: 22,
          fontWeight: 600,
          color: COLORS.ink,
          letterSpacing: "-0.02em",
          marginTop: 4,
        }}>Where the tape actually goes</div>
        <div style={{
          fontSize: 12,
          color: COLORS.inkMute,
          marginTop: 4,
          maxWidth: 540,
        }}>
          On real decks the tape leaves the bottom of each reel and runs along
          a lower path past a head. These iterations honour that, plus a
          handful of cassette riffs.
        </div>
      </div>

      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(5, 1fr)",
        gap: 14,
        flex: 1,
      }}>
        {ALL_REELS.map((v, idx) => {
          const isOriginal = idx < 2;
          const { Mark } = v;
          return (
            <div key={v.id} style={{
              display: "flex",
              flexDirection: "column",
              gap: 6,
              position: "relative",
            }}>
              <div style={{
                width: "100%",
                aspectRatio: "1 / 1",
                background: COLORS.ink,
                borderRadius: 14,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                position: "relative",
              }}>
                <Mark size={62} fg={COLORS.paper} bg="none" />
                {isOriginal && (
                  <span style={{
                    position: "absolute",
                    top: 6, right: 6,
                    fontFamily: 'var(--font-mono)',
                    fontSize: 8.5,
                    color: "rgba(255,255,255,0.55)",
                    textTransform: "uppercase",
                    letterSpacing: "0.1em",
                    background: "rgba(255,255,255,0.08)",
                    padding: "2px 5px",
                    borderRadius: 3,
                  }}>orig</span>
                )}
              </div>
              <div style={{
                fontFamily: 'var(--font-display)',
                fontSize: 11.5,
                fontWeight: 600,
                color: COLORS.ink,
                letterSpacing: "-0.01em",
              }}>{v.name}</div>
              <div style={{
                fontSize: 10,
                color: COLORS.inkMute,
                lineHeight: 1.4,
                textWrap: "pretty",
              }}>{v.note}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ReelTinyCard() {
  return (
    <div style={{
      width: "100%", height: "100%",
      background: COLORS.paper,
      padding: "30px 36px",
      boxSizing: "border-box",
      display: "flex",
      flexDirection: "column",
      gap: 16,
    }}>
      <div>
        <div style={{
          fontSize: 10.5,
          textTransform: "uppercase",
          letterSpacing: "0.18em",
          color: COLORS.inkMute,
          fontWeight: 600,
        }}>Tiny-size test</div>
        <div style={{
          fontFamily: 'var(--font-display)',
          fontSize: 20,
          fontWeight: 600,
          color: COLORS.ink,
          letterSpacing: "-0.02em",
          marginTop: 4,
        }}>Does it hold up at 16 px?</div>
      </div>

      <div style={{
        display: "grid",
        gridTemplateColumns: "110px 48px 36px 28px 1fr",
        alignItems: "center",
        gap: 14,
        fontSize: 9.5,
        textTransform: "uppercase",
        letterSpacing: "0.12em",
        color: COLORS.inkMute,
        fontWeight: 600,
        paddingBottom: 4,
        borderBottom: `1px solid ${COLORS.paperLine}`,
      }}>
        <div>Variant</div>
        <div style={{ textAlign: "center" }}>32</div>
        <div style={{ textAlign: "center" }}>20</div>
        <div style={{ textAlign: "center" }}>16</div>
        <div>Taskbar mock</div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 7, flex: 1 }}>
        {ALL_REELS.map((v) => {
          const { Mark } = v;
          return (
            <div key={v.id} style={{
              display: "grid",
              gridTemplateColumns: "110px 48px 36px 28px 1fr",
              alignItems: "center",
              gap: 14,
              fontSize: 12,
              color: COLORS.inkSoft,
            }}>
              <div style={{ fontWeight: 500, color: COLORS.ink, fontSize: 11.5 }}>{v.name}</div>
              <div style={{ display: "flex", justifyContent: "center" }}>
                <Mark size={32} fg={COLORS.ink} bg="none" />
              </div>
              <div style={{ display: "flex", justifyContent: "center" }}>
                <Mark size={20} fg={COLORS.ink} bg="none" />
              </div>
              <div style={{ display: "flex", justifyContent: "center" }}>
                <Mark size={16} fg={COLORS.ink} bg="none" />
              </div>
              <div style={{
                background: "#0d1116",
                borderRadius: 6,
                padding: "4px 8px",
                display: "flex",
                alignItems: "center",
                gap: 8,
                height: 26,
                boxSizing: "border-box",
              }}>
                <Mark size={14} fg="#ffffff" bg="none" />
                <span style={{
                  fontSize: 10.5,
                  color: "rgba(255,255,255,0.85)",
                  fontFamily: 'var(--font)',
                  fontWeight: 500,
                }}>Spool</span>
                <span style={{
                  fontSize: 10,
                  color: "rgba(255,255,255,0.4)",
                }}>{"\u2014 Library"}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* Lockup studies — pick three favourites and show as proper wordmark lockups. */
function ReelLockupCard() {
  const picks = ALL_REELS.filter((v) =>
    ["underslung", "underslung-head", "cassette-tape"].includes(v.id)
  );
  return (
    <div style={{
      width: "100%", height: "100%",
      background: COLORS.paper,
      padding: "32px 40px",
      boxSizing: "border-box",
      display: "flex",
      flexDirection: "column",
      gap: 20,
    }}>
      <div>
        <div style={{
          fontSize: 10.5,
          textTransform: "uppercase",
          letterSpacing: "0.18em",
          color: COLORS.inkMute,
          fontWeight: 600,
        }}>Lockups</div>
        <div style={{
          fontFamily: 'var(--font-display)',
          fontSize: 20,
          fontWeight: 600,
          color: COLORS.ink,
          letterSpacing: "-0.02em",
          marginTop: 4,
        }}>Mark + wordmark together</div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 22, flex: 1 }}>
        {picks.map((v) => {
          const { Mark } = v;
          return (
            <div key={v.id} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
                <Mark size={52} fg={COLORS.ink} bg="none" />
                <Wordmark name="Spool" fg={COLORS.ink} size={52} />
              </div>
              <div style={{
                fontFamily: 'var(--font-mono)',
                fontSize: 10,
                color: COLORS.inkMute,
                textTransform: "uppercase",
                letterSpacing: "0.1em",
              }}>{v.name}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

Object.assign(window, { ReelGridCard, ReelTinyCard, ReelLockupCard });
