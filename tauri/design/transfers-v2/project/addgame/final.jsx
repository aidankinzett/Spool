/* Add Game — final / polished V4
   "Drop the .exe, ludusavi identifies it" with every state designed:
     - empty       → waiting for a drop / browse
     - dragging    → file hovering over the drop zone
     - detecting   → looking up in ludusavi (brief)
     - multi       → 2-3 ranked candidates, pick one (expandable for save paths)
     - single      → one very high-confidence match auto-selected
     - nomatch     → ludusavi didn't recognise it; manual search + escape hatch */

const { useState: useStateF, useRef: useRefF, useEffect: useEffectF, useMemo: useMemoF } = React;

const TWEAK_DEFAULTS_ADDGAME = /*EDITMODE-BEGIN*/{
  "demoState": "multi",
  "accent": "#4cc2ff"
}/*EDITMODE-END*/;

/* ────────────────────────── Sample lookup data ────────────────────────── */
const SAVE_PATH_SAMPLES = {
  "Lego Batman: Legacy of the Dark Knight": [
    "%USERPROFILE%\\Documents\\WB Games\\LegoBatmanLegacy\\save",
    "%LOCALAPPDATA%\\LegoBatmanLegacy\\settings.cfg",
    "Steam Cloud · 322 KB",
  ],
  "Lego Batman 3: Beyond Gotham": [
    "%USERPROFILE%\\Documents\\WB Games\\LEGOBatman3\\save",
    "Steam Cloud · 84 KB",
  ],
  "Lego Batman: The Videogame": [
    "%USERPROFILE%\\Documents\\WB Games\\LegoBatman\\save",
  ],
};

const DETECTED = {
  filename: "LegoBatmanLegacy.exe",
  fullPath: "C:\\Games\\LEGO Batman - Legacy of the Dark Knight\\LegoBatmanLegacy.exe",
  folder: "C:\\Games\\LEGO Batman - Legacy of the Dark Knight",
  sizeMB: 142.4,
  version: "1.2.0",
  arch: "x64",
};

const MULTI_GUESSES = [
  { name: "Lego Batman: Legacy of the Dark Knight", coverage: 41, confidence: 96, best: true },
  { name: "Lego Batman 3: Beyond Gotham", coverage: 28, confidence: 42 },
  { name: "Lego Batman: The Videogame", coverage: 18, confidence: 31 },
];

const SINGLE_GUESSES = [
  { name: "Lego Batman: Legacy of the Dark Knight", coverage: 41, confidence: 96, best: true },
];

const NO_MATCH_MANUAL = [
  { name: "Lego Batman 3: Beyond Gotham", coverage: 28 },
  { name: "Lego Batman: Legacy of the Dark Knight", coverage: 41, best: true },
  { name: "Lego Batman: The Videogame", coverage: 18 },
  { name: "Lego Builder's Journey", coverage: 6 },
];

/* ────────────────────────── Main dialog ────────────────────────── */
function AddGameFinal() {
  const [tweaks, setTweak] = useTweaks(TWEAK_DEFAULTS_ADDGAME);
  const accent = tweaks.accent;
  const initialState = tweaks.demoState;

  // Local state, but keep it in sync if user flips the demo tweak.
  const [view, setView] = useStateF(initialState);
  useEffectF(() => { setView(initialState); }, [initialState]);

  const [picked, setPicked] = useStateF("Lego Batman: Legacy of the Dark Knight");
  const [expanded, setExpanded] = useStateF(null); // candidate name with paths showing
  const [admin, setAdmin] = useStateF(false);
  const [folder, setFolder] = useStateF(DETECTED.folder);
  const [manualQuery, setManualQuery] = useStateF("Lego batman");

  // Guesses for the active view
  const guesses = view === "multi" ? MULTI_GUESSES
                : view === "single" ? SINGLE_GUESSES
                : [];

  return (
    <>
      <DialogFrame width={680}>
        <DialogTitleBar title="Add Game" />

        <div style={{ padding: "20px 24px 4px" }}>
          <div style={{ fontSize: 18, fontWeight: 600, letterSpacing: "-0.01em" }}>
            Add a game
          </div>
          <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.55)", marginTop: 4 }}>
            Drop the game's executable below — Spool identifies it with ludusavi so saves get backed up automatically.
          </div>
        </div>

        <div style={{ padding: "16px 24px 4px", display: "flex", flexDirection: "column", gap: 14 }}>
          {view === "empty" && <DropZone accent={accent} onClickDemo={() => setView("multi")} />}
          {view === "dragging" && <DropZone accent={accent} dragging />}

          {(view !== "empty" && view !== "dragging") && (
            <ExeCard accent={accent} onChange={() => setView("empty")} detecting={view === "detecting"} />
          )}

          {view === "detecting" && <DetectingPanel accent={accent} />}

          {view === "single" && (
            <SingleMatch
              accent={accent}
              guess={guesses[0]}
              expanded={expanded === guesses[0].name}
              onToggleExpand={() => setExpanded(expanded === guesses[0].name ? null : guesses[0].name)}
              onShowAll={() => setView("multi")}
            />
          )}

          {view === "multi" && (
            <CandidateList
              accent={accent}
              guesses={guesses}
              picked={picked}
              onPick={setPicked}
              expanded={expanded}
              onToggleExpand={(n) => setExpanded(expanded === n ? null : n)}
            />
          )}

          {view === "nomatch" && (
            <NoMatchPanel
              accent={accent}
              query={manualQuery}
              onQuery={setManualQuery}
              picked={picked}
              onPick={setPicked}
            />
          )}

          {/* Options disclosure */}
          {(view === "multi" || view === "single" || view === "nomatch") && (
            <details style={{ marginTop: 2 }}>
              <summary style={{
                cursor: "pointer", listStyle: "none",
                fontSize: 12, color: "rgba(255,255,255,0.6)",
                display: "inline-flex", alignItems: "center", gap: 6,
                userSelect: "none",
              }}>
                <IconChevron size={11} /> More options
              </summary>
              <div style={{
                marginTop: 10,
                display: "grid", gridTemplateColumns: "140px 1fr",
                gap: "12px 12px", alignItems: "center",
              }}>
                <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>
                  Install folder
                  <div style={{ fontSize: 10.5, color: "rgba(255,255,255,0.4)", marginTop: 1 }}>for LAN share</div>
                </div>
                <div style={{ display: "flex", gap: 8 }}>
                  <TextField value={folder} onChange={setFolder} style={{ flex: 1 }} prefix={<IconFolder size={13} />} />
                  <Button variant="secondary" style={{ minWidth: 86 }}>Browse</Button>
                </div>

                <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>Permissions</div>
                <label style={{
                  display: "inline-flex", alignItems: "center", gap: 10, cursor: "pointer",
                  fontSize: 12.5, color: "rgba(255,255,255,0.78)",
                }}>
                  <ToggleSwitch checked={admin} onChange={setAdmin} accent={accent} />
                  Run as Administrator
                </label>
              </div>
            </details>
          )}
        </div>

        <div style={{ flex: 1 }} />

        <Footer
          accent={accent}
          canSubmit={(view === "multi" || view === "single") || (view === "nomatch" && picked)}
          state={view}
        />
      </DialogFrame>

      <TweaksPanel title="Tweaks">
        <TweakSection label="Demo">
          <TweakSelect
            label="State"
            value={tweaks.demoState}
            options={[
              { value: "empty",     label: "1 · Empty (waiting for drop)" },
              { value: "dragging",  label: "2 · Dragging over drop zone" },
              { value: "detecting", label: "3 · Detecting (loading)" },
              { value: "multi",     label: "4 · Multi-match (3 candidates)" },
              { value: "single",    label: "5 · Single high-confidence match" },
              { value: "nomatch",   label: "6 · No match (manual search)" },
            ]}
            onChange={(v) => setTweak("demoState", v)}
          />
        </TweakSection>
        <TweakSection label="Appearance">
          <TweakColor
            label="Accent"
            value={tweaks.accent}
            options={["#4cc2ff", "#7c5cff", "#21d07a", "#ff8a3d", "#ff5d8f"]}
            onChange={(v) => setTweak("accent", v)}
          />
        </TweakSection>
      </TweaksPanel>
    </>
  );
}

/* ────────────────────────── Drop zone ────────────────────────── */
function DropZone({ accent, dragging, onClickDemo }) {
  return (
    <div
      onClick={onClickDemo}
      style={{
        padding: "36px 20px",
        border: `2px dashed ${dragging ? accent : "rgba(255,255,255,0.18)"}`,
        borderRadius: 10,
        background: dragging ? `${accent}10` : "rgba(255,255,255,0.02)",
        display: "flex", flexDirection: "column", alignItems: "center", gap: 10,
        cursor: onClickDemo ? "pointer" : "default",
        textAlign: "center",
        transition: "background 140ms ease, border-color 140ms ease",
        position: "relative",
        overflow: "hidden",
      }}
    >
      {dragging && (
        <div style={{
          position: "absolute", inset: 0,
          background: `radial-gradient(400px 200px at 50% 50%, ${accent}22, transparent 70%)`,
          pointerEvents: "none",
        }}/>
      )}
      <div style={{
        width: 52, height: 52, borderRadius: 26,
        background: dragging ? `${accent}33` : `${accent}1a`,
        display: "flex", alignItems: "center", justifyContent: "center",
        color: accent,
        transform: dragging ? "scale(1.08)" : "scale(1)",
        transition: "transform 140ms ease, background 140ms ease",
      }}>
        <IconExe size={24} />
      </div>
      <div style={{ fontSize: 14, fontWeight: 500 }}>
        {dragging ? "Drop to identify" : "Drop a game.exe here"}
      </div>
      <div style={{ fontSize: 12, color: "rgba(255,255,255,0.55)" }}>
        {dragging
          ? <>Spool will look it up in ludusavi's database</>
          : <>or <span style={{ color: accent, textDecoration: "underline" }}>browse for one</span></>}
      </div>
    </div>
  );
}

/* ────────────────────────── Detected EXE card ────────────────────────── */
function ExeCard({ accent, detecting, onChange }) {
  return (
    <div style={{
      padding: "12px 14px",
      background: "rgba(255,255,255,0.03)",
      border: "1px solid rgba(255,255,255,0.08)",
      borderRadius: 8,
      display: "flex", alignItems: "center", gap: 12,
    }}>
      <div style={{
        width: 36, height: 36, borderRadius: 6,
        background: "rgba(255,255,255,0.04)",
        border: "1px solid rgba(255,255,255,0.08)",
        display: "flex", alignItems: "center", justifyContent: "center",
        color: "rgba(255,255,255,0.85)",
        flexShrink: 0,
      }}><IconExe size={18} /></div>

      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: 13, fontWeight: 500,
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
        }}>{DETECTED.filename}</div>
        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          fontSize: 11, color: "rgba(255,255,255,0.5)",
          marginTop: 2,
        }}>
          <span style={{
            fontFamily: `"JetBrains Mono","Cascadia Code","SF Mono",ui-monospace,monospace`,
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
            minWidth: 0, flex: 1,
          }}>{DETECTED.fullPath}</span>
          <span style={{ flexShrink: 0, color: "rgba(255,255,255,0.4)" }}>·</span>
          <span style={{ flexShrink: 0, fontVariantNumeric: "tabular-nums" }}>{DETECTED.sizeMB.toFixed(1)} MB</span>
          <span style={{ flexShrink: 0, color: "rgba(255,255,255,0.4)" }}>·</span>
          <span style={{ flexShrink: 0 }}>v{DETECTED.version} {DETECTED.arch}</span>
        </div>
      </div>

      {!detecting && (
        <Button variant="ghost" onClick={onChange} style={{ fontSize: 11.5 }}>Change</Button>
      )}
    </div>
  );
}

/* ────────────────────────── Detecting (loading) ────────────────────────── */
function DetectingPanel({ accent }) {
  return (
    <div style={{
      padding: "20px 16px",
      background: "rgba(255,255,255,0.024)",
      border: "1px solid rgba(255,255,255,0.06)",
      borderRadius: 8,
      display: "flex", alignItems: "center", gap: 12,
    }}>
      <Spinner accent={accent} />
      <div>
        <div style={{ fontSize: 13, fontWeight: 500 }}>Looking up in ludusavi</div>
        <div style={{ fontSize: 11.5, color: "rgba(255,255,255,0.55)", marginTop: 2 }}>
          Matching <span style={{ fontFamily: "monospace" }}>LegoBatmanLegacy.exe</span> against 17,820 known games…
        </div>
      </div>
    </div>
  );
}

function Spinner({ accent, size = 20 }) {
  return (
    <>
      <style>{`
        @keyframes ag-spin { to { transform: rotate(360deg); } }
      `}</style>
      <svg width={size} height={size} viewBox="0 0 24 24" style={{
        animation: "ag-spin 0.9s linear infinite", flexShrink: 0,
      }}>
        <circle cx="12" cy="12" r="9" fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth="2.5" />
        <path d="M12 3 a9 9 0 0 1 9 9" fill="none" stroke={accent} strokeWidth="2.5" strokeLinecap="round" />
      </svg>
    </>
  );
}

/* ────────────────────────── Multi-match list ────────────────────────── */
function CandidateList({ accent, guesses, picked, onPick, expanded, onToggleExpand }) {
  return (
    <div>
      <ListHeader accent={accent} count={guesses.length} />
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        {guesses.map((g) => (
          <CandidateRow
            key={g.name}
            accent={accent}
            guess={g}
            picked={picked === g.name}
            onPick={() => onPick(g.name)}
            expanded={expanded === g.name}
            onToggleExpand={() => onToggleExpand(g.name)}
          />
        ))}
      </div>
    </div>
  );
}

function ListHeader({ accent, count }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "space-between",
      marginBottom: 8,
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{
          display: "inline-flex", alignItems: "center", gap: 5,
          fontSize: 10.5, color: accent, fontWeight: 600,
          letterSpacing: "0.08em", textTransform: "uppercase",
        }}>
          <IconSparkle size={11} /> Auto-matched
        </span>
        <span style={{ fontSize: 12, color: "rgba(255,255,255,0.6)" }}>
          {count} candidate{count === 1 ? "" : "s"} in ludusavi's database
        </span>
      </div>
      <button style={{
        background: "transparent", border: "none",
        color: "rgba(255,255,255,0.6)", fontSize: 11.5,
        cursor: "pointer", padding: 0,
        display: "inline-flex", alignItems: "center", gap: 4,
      }}>
        <IconSearch size={11} /> Search manually
      </button>
    </div>
  );
}

function CandidateRow({ accent, guess, picked, onPick, expanded, onToggleExpand }) {
  const paths = SAVE_PATH_SAMPLES[guess.name] || [];
  return (
    <div style={{
      background: picked ? `${accent}14` : "rgba(255,255,255,0.024)",
      border: `1px solid ${picked ? accent + "55" : "rgba(255,255,255,0.06)"}`,
      borderRadius: 8,
      overflow: "hidden",
      transition: "all 100ms ease",
    }}>
      <button
        onClick={onPick}
        style={{
          display: "flex", alignItems: "center", gap: 12,
          padding: "10px 14px",
          width: "100%",
          background: "transparent", border: "none",
          color: "#fff", fontFamily: "inherit", fontSize: 13,
          cursor: "pointer", textAlign: "left",
        }}
      >
        {/* Radio dot */}
        <span style={{
          width: 16, height: 16, borderRadius: 8,
          border: `1.5px solid ${picked ? accent : "rgba(255,255,255,0.3)"}`,
          display: "flex", alignItems: "center", justifyContent: "center",
          flexShrink: 0,
        }}>
          {picked && <span style={{ width: 7, height: 7, borderRadius: 4, background: accent }} />}
        </span>

        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            display: "flex", alignItems: "center", gap: 8,
            fontSize: 13, fontWeight: picked ? 500 : 400,
          }}>
            {guess.name}
            {guess.best && (
              <span style={{
                fontSize: 9.5, color: accent, fontWeight: 600,
                letterSpacing: "0.08em", textTransform: "uppercase",
              }}>Best match</span>
            )}
          </div>
          <div style={{
            fontSize: 10.5, color: "rgba(255,255,255,0.5)",
            marginTop: 2,
          }}>
            {guess.coverage} save files tracked
          </div>
        </div>

        {/* Confidence bar */}
        <div style={{
          display: "flex", flexDirection: "column", alignItems: "flex-end",
          gap: 4, minWidth: 64, flexShrink: 0,
        }}>
          <span style={{
            fontSize: 10.5, color: "rgba(255,255,255,0.6)",
            fontVariantNumeric: "tabular-nums",
          }}>{guess.confidence}% match</span>
          <div style={{
            width: 64, height: 3, borderRadius: 2,
            background: "rgba(255,255,255,0.08)",
            overflow: "hidden",
          }}>
            <div style={{
              width: `${guess.confidence}%`, height: "100%",
              background: guess.confidence > 70 ? accent : "rgba(255,255,255,0.35)",
              borderRadius: 2,
            }}/>
          </div>
        </div>

        {/* Expand chevron */}
        <span
          onClick={(e) => { e.stopPropagation(); onToggleExpand(); }}
          style={{
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            width: 22, height: 22, borderRadius: 4,
            color: "rgba(255,255,255,0.55)",
            transform: expanded ? "rotate(180deg)" : "rotate(0)",
            transition: "transform 140ms ease",
            flexShrink: 0,
          }}
        >
          <IconChevronDown size={13} />
        </span>
      </button>

      {expanded && (
        <div style={{
          borderTop: `1px solid ${picked ? accent + "33" : "rgba(255,255,255,0.06)"}`,
          padding: "10px 14px 12px",
          background: "rgba(0,0,0,0.18)",
        }}>
          <div style={{
            fontSize: 10.5, color: "rgba(255,255,255,0.5)",
            textTransform: "uppercase", letterSpacing: "0.08em",
            marginBottom: 6,
          }}>Save locations ludusavi will track</div>
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            {paths.length === 0 && (
              <div style={{ fontSize: 11.5, color: "rgba(255,255,255,0.5)" }}>None on file.</div>
            )}
            {paths.map((p, i) => (
              <div key={i} style={{
                display: "flex", alignItems: "center", gap: 8,
                fontSize: 11.5,
                fontFamily: p.startsWith("Steam") ? "inherit" : `"JetBrains Mono","Cascadia Code","SF Mono",ui-monospace,monospace`,
                color: "rgba(255,255,255,0.7)",
              }}>
                <IconFolder size={11} />
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/* ────────────────────────── Single-match (auto-confirmed) ────────────────────────── */
function SingleMatch({ accent, guess, expanded, onToggleExpand, onShowAll }) {
  const paths = SAVE_PATH_SAMPLES[guess.name] || [];
  return (
    <div style={{
      background: `${accent}10`,
      border: `1px solid ${accent}55`,
      borderRadius: 8,
      overflow: "hidden",
    }}>
      <div style={{
        padding: "12px 14px",
        display: "flex", alignItems: "center", gap: 12,
      }}>
        <span style={{
          width: 28, height: 28, borderRadius: 14,
          background: `${accent}33`, color: accent,
          display: "flex", alignItems: "center", justifyContent: "center",
          flexShrink: 0,
        }}>
          <IconCheck size={14} />
        </span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 13.5, fontWeight: 500 }}>{guess.name}</span>
            <span style={{
              fontSize: 9.5, color: accent, fontWeight: 600,
              letterSpacing: "0.08em", textTransform: "uppercase",
            }}>{guess.confidence}% match</span>
          </div>
          <div style={{
            fontSize: 11.5, color: "rgba(255,255,255,0.6)", marginTop: 2,
            display: "flex", alignItems: "center", gap: 8,
          }}>
            <span>Identified by ludusavi · {guess.coverage} save files will be tracked</span>
          </div>
        </div>
        <button
          onClick={onToggleExpand}
          style={{
            background: "transparent", border: "none",
            color: "rgba(255,255,255,0.7)", fontSize: 11.5,
            cursor: "pointer",
            display: "inline-flex", alignItems: "center", gap: 4,
          }}
        >
          {expanded ? "Hide" : "Preview"} saves
          <span style={{
            display: "inline-flex",
            transform: expanded ? "rotate(180deg)" : "rotate(0)",
            transition: "transform 140ms ease",
          }}>
            <IconChevronDown size={11} />
          </span>
        </button>
      </div>

      {expanded && (
        <div style={{
          borderTop: `1px solid ${accent}33`,
          padding: "10px 14px 12px",
          background: "rgba(0,0,0,0.18)",
        }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            {paths.map((p, i) => (
              <div key={i} style={{
                display: "flex", alignItems: "center", gap: 8,
                fontSize: 11.5,
                fontFamily: p.startsWith("Steam") ? "inherit" : `"JetBrains Mono","Cascadia Code","SF Mono",ui-monospace,monospace`,
                color: "rgba(255,255,255,0.7)",
              }}>
                <IconFolder size={11} />
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      <div style={{
        padding: "8px 14px",
        borderTop: `1px solid ${accent}22`,
        background: "rgba(0,0,0,0.12)",
        display: "flex", alignItems: "center", justifyContent: "space-between",
        fontSize: 11.5,
      }}>
        <span style={{ color: "rgba(255,255,255,0.5)" }}>Not this one?</span>
        <button onClick={onShowAll} style={{
          background: "transparent", border: "none",
          color: accent, fontSize: 11.5,
          cursor: "pointer", padding: 0,
        }}>Show 2 other matches</button>
      </div>
    </div>
  );
}

/* ────────────────────────── No-match panel (manual search) ────────────────────────── */
function NoMatchPanel({ accent, query, onQuery, picked, onPick }) {
  const results = useMemoF(() => {
    if (!query.trim()) return [];
    const q = query.toLowerCase();
    return NO_MATCH_MANUAL.filter(g => g.name.toLowerCase().includes(q));
  }, [query]);

  return (
    <div>
      <div style={{
        padding: "10px 12px",
        background: "rgba(255,180,90,0.06)",
        border: "1px solid rgba(255,180,90,0.22)",
        borderRadius: 8,
        display: "flex", alignItems: "center", gap: 10,
        marginBottom: 12,
      }}>
        <span style={{
          width: 22, height: 22, borderRadius: 11,
          background: "rgba(255,180,90,0.14)", color: "#ffc278",
          display: "flex", alignItems: "center", justifyContent: "center",
          flexShrink: 0,
        }}>
          <IconInfo size={12} />
        </span>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.85)" }}>
            Spool couldn't identify this from the filename
          </div>
          <div style={{ fontSize: 11.5, color: "rgba(255,255,255,0.55)", marginTop: 1 }}>
            Search ludusavi's database below, or add the game without save management.
          </div>
        </div>
      </div>

      <div style={{ marginBottom: 8 }}>
        <FieldLabel>Search ludusavi</FieldLabel>
        <div style={{
          display: "flex", alignItems: "center", gap: 8,
          height: 38,
          background: "rgba(255,255,255,0.04)",
          border: `1px solid ${query ? accent : "rgba(255,255,255,0.12)"}`,
          borderRadius: 6,
          padding: "0 12px",
          transition: "border-color 120ms ease",
        }}>
          <IconSearch size={14} />
          <input
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            autoFocus
            placeholder="Type the game's name…"
            style={{
              flex: 1, background: "transparent", border: "none", outline: "none",
              color: "#fff", fontSize: 14, fontFamily: "inherit",
            }}
          />
          <span style={{ fontSize: 11, color: "rgba(255,255,255,0.5)" }}>
            {results.length} match{results.length === 1 ? "" : "es"}
          </span>
        </div>
      </div>

      <div style={{
        maxHeight: 180, overflow: "auto",
        border: "1px solid rgba(255,255,255,0.06)",
        borderRadius: 6,
        background: "rgba(0,0,0,0.18)",
      }}>
        {results.length === 0 && (
          <div style={{
            padding: 16, textAlign: "center",
            fontSize: 12, color: "rgba(255,255,255,0.45)",
          }}>No matches — try a shorter name.</div>
        )}
        {results.map((g) => {
          const isPicked = picked === g.name;
          return (
            <button
              key={g.name}
              onClick={() => onPick(g.name)}
              style={{
                display: "flex", alignItems: "center", justifyContent: "space-between",
                width: "100%", padding: "9px 12px",
                background: isPicked ? `${accent}14` : "transparent",
                border: "none",
                borderLeft: isPicked ? `2px solid ${accent}` : "2px solid transparent",
                borderBottom: "1px solid rgba(255,255,255,0.04)",
                color: "#fff", fontFamily: "inherit", fontSize: 12.5,
                cursor: "pointer", textAlign: "left",
              }}
              onMouseEnter={(e) => { if (!isPicked) e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
              onMouseLeave={(e) => { if (!isPicked) e.currentTarget.style.background = "transparent"; }}
            >
              <span style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <IconDatabase size={12} />
                <span><HiName name={g.name} query={query} /></span>
                {g.best && (
                  <span style={{
                    fontSize: 9, color: accent, fontWeight: 600,
                    letterSpacing: "0.08em", textTransform: "uppercase",
                  }}>Best match</span>
                )}
              </span>
              <span style={{ fontSize: 10.5, color: "rgba(255,255,255,0.5)" }}>
                {g.coverage} files
              </span>
            </button>
          );
        })}
      </div>

      <button style={{
        marginTop: 10,
        background: "transparent", border: "none",
        color: "rgba(255,255,255,0.55)", fontSize: 11.5,
        cursor: "pointer", padding: 0,
        display: "inline-flex", alignItems: "center", gap: 4,
      }}>
        <IconShield size={11} /> Add anyway without save management
      </button>
    </div>
  );
}

/* ────────────────────────── Footer ────────────────────────── */
function Footer({ accent, canSubmit, state }) {
  return (
    <div style={{
      padding: "14px 20px 18px",
      display: "flex",
      gap: 8,
      alignItems: "center",
      borderTop: "1px solid rgba(255,255,255,0.04)",
      background: "rgba(0,0,0,0.18)",
    }}>
      <Button variant="ghost" style={{ fontSize: 12.5, color: "rgba(255,255,255,0.6)" }}>
        Cancel
      </Button>
      <div style={{ flex: 1 }} />
      <Button variant="secondary" disabled={!canSubmit} style={{ height: 36, fontSize: 13 }}>
        <IconArmoury size={13} /> Armoury Crate
      </Button>
      <Button variant="secondary" disabled={!canSubmit} style={{ height: 36, fontSize: 13 }}>
        <IconSteam size={13} /> Add to Steam
      </Button>
      <Button
        variant="primary" accent={accent} disabled={!canSubmit}
        style={{ minWidth: 130, height: 36, fontSize: 13, fontWeight: 500 }}
      >
        Add to Library
      </Button>
    </div>
  );
}

window.AddGameFinal = AddGameFinal;
