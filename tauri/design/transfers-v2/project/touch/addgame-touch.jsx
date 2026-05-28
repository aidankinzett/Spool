/* Add Game dialog — tokens-driven version. Mirrors v4 (drop-detect)
   showing the multi-match state, since it exercises the largest
   variety of touch targets (radio rows, expandable chevron, footer
   buttons, search, toggle). */

const { useState: useStateAT } = React;

const AT_DETECTED = {
  filename: "LegoBatmanLegacy.exe",
  fullPath: "C:\\Games\\LEGO Batman - Legacy of the Dark Knight\\LegoBatmanLegacy.exe",
  folder: "C:\\Games\\LEGO Batman - Legacy of the Dark Knight",
  sizeMB: 142.4, version: "1.2.0", arch: "x64",
};

const AT_GUESSES = [
  { name: "Lego Batman: Legacy of the Dark Knight", coverage: 41, confidence: 96, best: true },
  { name: "Lego Batman 3: Beyond Gotham", coverage: 28, confidence: 42 },
  { name: "Lego Batman: The Videogame", coverage: 18, confidence: 31 },
];

const AT_PATHS = {
  "Lego Batman: Legacy of the Dark Knight": [
    "%USERPROFILE%\\Documents\\WB Games\\LegoBatmanLegacy\\save",
    "%LOCALAPPDATA%\\LegoBatmanLegacy\\settings.cfg",
    "Steam Cloud · 322 KB",
  ],
};

/* ---------------- Frame ---------------- */
function AddGameDialog({ tokens, accent = "#4cc2ff", width }) {
  const t = tokens;
  const w = width ?? (t.pointer === "coarse" ? 760 : 680);

  const [picked, setPicked] = useStateAT("Lego Batman: Legacy of the Dark Knight");
  const [expanded, setExpanded] = useStateAT("Lego Batman: Legacy of the Dark Knight");
  const [admin, setAdmin] = useStateAT(false);
  const [more, setMore] = useStateAT(false);

  return (
    <div style={{
      width: w,
      background: "linear-gradient(180deg, rgba(28,28,28,0.96) 0%, rgba(22,22,22,0.98) 100%)",
      borderRadius: 8, border: "1px solid rgba(255,255,255,0.06)",
      boxShadow: "0 20px 60px rgba(0,0,0,0.55), 0 4px 16px rgba(0,0,0,0.4)",
      display: "flex", flexDirection: "column", overflow: "hidden",
      color: "#fff",
      fontFamily: `"Segoe UI Variable Text","Segoe UI Variable","Segoe UI","Inter",-apple-system,sans-serif`,
      position: "relative",
    }}>
      <div style={{
        position: "absolute", top: -240, left: -160,
        width: 600, height: 600,
        background: `radial-gradient(circle, ${accent}0e, transparent 60%)`,
        pointerEvents: "none",
      }}/>

      {/* Title bar */}
      <div style={{
        height: t.titleBar, display: "flex", alignItems: "center",
        justifyContent: "space-between", padding: "0 0 0 16px",
        flexShrink: 0,
        borderBottom: "1px solid rgba(255,255,255,0.04)",
        position: "relative", zIndex: 2,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <SpoolMark size={t.pointer === "coarse" ? 18 : 14} fg="rgba(255,255,255,0.85)" />
          <span style={{ fontSize: t.sm + 1, color: "rgba(255,255,255,0.88)", fontWeight: 500 }}>Add Game</span>
        </div>
        <div style={{ display: "flex" }}>
          <ATTitleBtn t={t}><IconMinimize size={t.titleBtnIcon - 1} /></ATTitleBtn>
          <ATTitleBtn t={t} danger><IconClose size={t.titleBtnIcon - 1} /></ATTitleBtn>
        </div>
      </div>

      <div style={{ padding: `${t.cardPad}px ${t.pageGutter - 4}px ${t.rowPadY - 4}px` }}>
        <div style={{ fontSize: t.h2, fontWeight: 600, letterSpacing: "-0.01em" }}>
          Add a game
        </div>
        <div style={{ fontSize: t.sm + 1.5, color: "rgba(255,255,255,0.55)", marginTop: 4 }}>
          Drop the game's executable below — Spool identifies it with ludusavi so saves get backed up automatically.
        </div>
      </div>

      <div style={{
        padding: `${t.rowPadY}px ${t.pageGutter - 4}px ${t.rowPadY - 4}px`,
        display: "flex", flexDirection: "column", gap: t.sectionGap - 4,
      }}>
        <ATExeCard t={t} accent={accent} />
        <ATCandidates
          accent={accent} t={t}
          picked={picked} setPicked={setPicked}
          expanded={expanded} setExpanded={setExpanded}
        />

        {/* More options disclosure */}
        <div>
          <button
            onClick={() => setMore(!more)}
            style={{
              cursor: "pointer", background: "transparent", border: "none",
              fontSize: t.sm + 1, color: "rgba(255,255,255,0.6)",
              display: "inline-flex", alignItems: "center", gap: 6,
              fontFamily: "inherit",
              padding: t.pointer === "coarse" ? `8px 4px` : 0,
              minHeight: t.pointer === "coarse" ? 36 : "auto",
            }}
          >
            <span style={{
              display: "inline-flex",
              transform: more ? "rotate(90deg)" : "rotate(0)",
              transition: "transform 140ms ease",
            }}><IconChevron size={t.chevron - 2} /></span>
            More options
          </button>
          {more && (
            <div style={{
              marginTop: 10,
              display: "grid",
              gridTemplateColumns: t.pointer === "coarse" ? "160px 1fr" : "140px 1fr",
              gap: `${t.rowPadY}px ${t.rowGap}px`,
              alignItems: "center",
            }}>
              <div style={{ fontSize: t.sm + 1, color: "rgba(255,255,255,0.75)" }}>
                Install folder
                <div style={{ fontSize: t.xs - 0.5, color: "rgba(255,255,255,0.4)", marginTop: 1 }}>for LAN share</div>
              </div>
              <div style={{ display: "flex", gap: 8 }}>
                <ATTextField value={AT_DETECTED.folder} t={t} accent={accent} />
                <LTButton variant="secondary" t={t} accent={accent} style={{ minWidth: 86 }}>Browse</LTButton>
              </div>
              <div style={{ fontSize: t.sm + 1, color: "rgba(255,255,255,0.75)" }}>Permissions</div>
              <label style={{
                display: "inline-flex", alignItems: "center", gap: 12,
                cursor: "pointer", fontSize: t.sm + 1.5, color: "rgba(255,255,255,0.78)",
              }}>
                <LTToggle checked={admin} onChange={setAdmin} accent={accent} t={t} />
                Run as Administrator
              </label>
            </div>
          )}
        </div>
      </div>

      {/* Footer */}
      <div style={{
        padding: `${t.rowPadY + 4}px ${t.pageGutter - 4}px ${t.cardPad}px`,
        display: "flex", gap: 10, alignItems: "center",
        borderTop: "1px solid rgba(255,255,255,0.04)",
        background: "rgba(0,0,0,0.18)",
      }}>
        <LTButton variant="ghost" accent={accent} t={t}>Cancel</LTButton>
        <div style={{ flex: 1 }} />
        <LTButton variant="secondary" accent={accent} t={t}
          icon={<IconArmoury size={t.base} />}>Armoury Crate</LTButton>
        <LTButton variant="secondary" accent={accent} t={t}
          icon={<IconSteam size={t.base} />}>Add to Steam</LTButton>
        <LTButton variant="primary" accent={accent} t={t}
          style={{ minWidth: 140, fontWeight: 600 }}>Add to Library</LTButton>
      </div>
    </div>
  );
}

function ATTitleBtn({ children, danger, t }) {
  const [hover, setHover] = useStateAT(false);
  return (
    <button
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        width: t.titleBtnW, height: t.titleBar,
        background: hover ? (danger ? "#c42b1c" : "rgba(255,255,255,0.06)") : "transparent",
        color: hover && danger ? "#fff" : "rgba(255,255,255,0.78)",
        border: "none", cursor: "pointer",
        display: "flex", alignItems: "center", justifyContent: "center",
      }}
    >{children}</button>
  );
}

/* ---------------- Detected EXE card ---------------- */
function ATExeCard({ t, accent }) {
  const iconBox = t.pointer === "coarse" ? 48 : 36;
  return (
    <div style={{
      padding: `${t.rowPadY}px ${t.rowPadY + 4}px`,
      background: "rgba(255,255,255,0.03)",
      border: "1px solid rgba(255,255,255,0.08)",
      borderRadius: 8,
      display: "flex", alignItems: "center", gap: 12,
    }}>
      <div style={{
        width: iconBox, height: iconBox, borderRadius: 6,
        background: "rgba(255,255,255,0.04)",
        border: "1px solid rgba(255,255,255,0.08)",
        display: "flex", alignItems: "center", justifyContent: "center",
        color: "rgba(255,255,255,0.85)", flexShrink: 0,
      }}>
        <IconExe size={Math.round(iconBox * 0.5)} />
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: t.base, fontWeight: 500,
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
        }}>{AT_DETECTED.filename}</div>
        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          fontSize: t.sm, color: "rgba(255,255,255,0.5)", marginTop: 2,
        }}>
          <span style={{
            fontFamily: `"JetBrains Mono","Cascadia Code","SF Mono",ui-monospace,monospace`,
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
            minWidth: 0, flex: 1,
          }}>{AT_DETECTED.fullPath}</span>
          <span style={{ flexShrink: 0, color: "rgba(255,255,255,0.4)" }}>·</span>
          <span style={{ flexShrink: 0, fontVariantNumeric: "tabular-nums" }}>{AT_DETECTED.sizeMB.toFixed(1)} MB</span>
        </div>
      </div>
      <LTButton variant="ghost" t={t} accent={accent} size="sm">Change</LTButton>
    </div>
  );
}

/* ---------------- Candidate list ---------------- */
function ATCandidates({ accent, t, picked, setPicked, expanded, setExpanded }) {
  return (
    <div>
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        marginBottom: 10,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{
            display: "inline-flex", alignItems: "center", gap: 5,
            fontSize: t.xs, color: accent, fontWeight: 600,
            letterSpacing: "0.08em", textTransform: "uppercase",
          }}>
            <IconSparkle size={t.xs} /> Auto-matched
          </span>
          <span style={{ fontSize: t.sm + 1, color: "rgba(255,255,255,0.6)" }}>
            {AT_GUESSES.length} candidates in ludusavi's database
          </span>
        </div>
        <button style={{
          background: "transparent", border: "none",
          color: "rgba(255,255,255,0.6)", fontSize: t.sm + 1,
          cursor: "pointer", padding: t.pointer === "coarse" ? `8px 4px` : 0,
          display: "inline-flex", alignItems: "center", gap: 4,
          fontFamily: "inherit",
        }}>
          <IconSearch size={t.xs + 1} /> Search manually
        </button>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {AT_GUESSES.map(g => (
          <ATCandidateRow
            key={g.name} accent={accent} t={t} guess={g}
            picked={picked === g.name}
            onPick={() => setPicked(g.name)}
            expanded={expanded === g.name}
            onToggleExpand={() => setExpanded(expanded === g.name ? null : g.name)}
          />
        ))}
      </div>
    </div>
  );
}

function ATCandidateRow({ accent, t, guess, picked, onPick, expanded, onToggleExpand }) {
  const paths = AT_PATHS[guess.name] || [];
  const rowH = t.pointer === "coarse" ? 64 : 48;
  return (
    <div style={{
      background: picked ? `${accent}14` : "rgba(255,255,255,0.024)",
      border: `1px solid ${picked ? accent + "55" : "rgba(255,255,255,0.06)"}`,
      borderRadius: 8, overflow: "hidden",
      transition: "all 100ms ease",
    }}>
      <div style={{
        display: "flex", alignItems: "center",
        minHeight: rowH,
      }}>
        <button
          onClick={onPick}
          style={{
            flex: 1, display: "flex", alignItems: "center", gap: 14,
            padding: `${t.rowPadY - 2}px ${t.rowPadY + 4}px`,
            background: "transparent", border: "none",
            color: "#fff", fontFamily: "inherit", fontSize: t.base,
            cursor: "pointer", textAlign: "left",
          }}
        >
          <span style={{
            width: t.radio, height: t.radio, borderRadius: t.radio / 2,
            border: `1.5px solid ${picked ? accent : "rgba(255,255,255,0.3)"}`,
            display: "flex", alignItems: "center", justifyContent: "center",
            flexShrink: 0,
          }}>
            {picked && <span style={{
              width: t.radioDot, height: t.radioDot, borderRadius: t.radioDot / 2,
              background: accent,
            }}/>}
          </span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{
              display: "flex", alignItems: "center", gap: 8,
              fontSize: t.base, fontWeight: picked ? 500 : 400,
            }}>
              {guess.name}
              {guess.best && (
                <span style={{
                  fontSize: t.xs - 1, color: accent, fontWeight: 600,
                  letterSpacing: "0.08em", textTransform: "uppercase",
                }}>Best match</span>
              )}
            </div>
            <div style={{
              fontSize: t.xs, color: "rgba(255,255,255,0.5)", marginTop: 2,
            }}>{guess.coverage} save files tracked</div>
          </div>
          <div style={{
            display: "flex", flexDirection: "column", alignItems: "flex-end",
            gap: 4, minWidth: t.confidenceBar, flexShrink: 0,
          }}>
            <span style={{
              fontSize: t.xs, color: "rgba(255,255,255,0.6)",
              fontVariantNumeric: "tabular-nums",
            }}>{guess.confidence}% match</span>
            <div style={{
              width: t.confidenceBar, height: t.pointer === "coarse" ? 5 : 3,
              borderRadius: 2, background: "rgba(255,255,255,0.08)",
              overflow: "hidden",
            }}>
              <div style={{
                width: `${guess.confidence}%`, height: "100%",
                background: guess.confidence > 70 ? accent : "rgba(255,255,255,0.35)",
                borderRadius: 2,
              }}/>
            </div>
          </div>
        </button>
        <button
          onClick={onToggleExpand}
          style={{
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            width: t.pointer === "coarse" ? 52 : 36,
            height: rowH,
            background: "transparent", border: "none",
            color: "rgba(255,255,255,0.55)",
            cursor: "pointer",
            transform: expanded ? "rotate(180deg)" : "rotate(0)",
            transition: "transform 140ms ease",
            flexShrink: 0,
          }}
        >
          <IconChevronDown size={t.chevron} />
        </button>
      </div>

      {expanded && (
        <div style={{
          borderTop: `1px solid ${picked ? accent + "33" : "rgba(255,255,255,0.06)"}`,
          padding: `${t.rowPadY}px ${t.rowPadY + 4}px`,
          background: "rgba(0,0,0,0.18)",
        }}>
          <div style={{
            fontSize: t.xs, color: "rgba(255,255,255,0.5)",
            textTransform: "uppercase", letterSpacing: "0.08em",
            marginBottom: 6,
          }}>Save locations ludusavi will track</div>
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            {paths.length === 0 && (
              <div style={{ fontSize: t.sm, color: "rgba(255,255,255,0.5)" }}>None on file.</div>
            )}
            {paths.map((p, i) => (
              <div key={i} style={{
                display: "flex", alignItems: "center", gap: 8,
                fontSize: t.sm,
                fontFamily: p.startsWith("Steam") ? "inherit"
                  : `"JetBrains Mono","Cascadia Code","SF Mono",ui-monospace,monospace`,
                color: "rgba(255,255,255,0.7)",
              }}>
                <IconFolder size={t.xs + 1} />
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/* ---------------- Inline text field (touch-aware) ---------------- */
function ATTextField({ value, t, accent }) {
  return (
    <div style={{
      flex: 1, display: "flex", alignItems: "center", gap: 8,
      height: t.btnH, background: "rgba(255,255,255,0.04)",
      border: "1px solid rgba(255,255,255,0.10)",
      borderRadius: 6, padding: "0 12px",
    }}>
      <IconFolder size={t.sm + 2} />
      <span style={{
        flex: 1, fontSize: t.sm + 1, color: "rgba(255,255,255,0.85)",
        fontFamily: `"JetBrains Mono","Cascadia Code","SF Mono",ui-monospace,monospace`,
        whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
      }}>{value}</span>
    </div>
  );
}

Object.assign(window, { AddGameDialog });
