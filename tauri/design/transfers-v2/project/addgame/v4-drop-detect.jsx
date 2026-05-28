/* Variant 4 — "Drop EXE, auto-detect"
   Lead with a drop zone for the executable. After drop, Spool
   parses the filename + folder name and runs them through ludusavi's
   database, presenting ranked candidates the user just confirms.
   Search becomes a confirmation, not a chore. */

function AddGameV4() {
  // Stage: "empty" | "matched" — the moment the user has dropped an exe
  const [stage, setStage] = useStateS("matched");
  const [picked, setPicked] = useStateS({ name: "Lego Batman: Legacy of the Dark Knight", coverage: 41, best: true });
  const [admin, setAdmin] = useStateS(false);
  const [folder, setFolder] = useStateS("C:\\Games\\LEGO Batman - Legacy of the Dark Knight");

  const exePath = "C:\\Games\\LEGO Batman - Legacy of the Dark Knight\\LegoBatmanLegacy.exe";
  const guesses = [
    { name: "Lego Batman: Legacy of the Dark Knight", coverage: 41, confidence: 96, best: true },
    { name: "Lego Batman 3: Beyond Gotham", coverage: 28, confidence: 42 },
    { name: "Lego Batman: The Videogame", coverage: 18, confidence: 31 },
  ];

  return (
    <DialogFrame width={680}>
      <DialogTitleBar title="Add Game" />

      <div style={{ padding: "20px 24px 4px" }}>
        <div style={{ fontSize: 18, fontWeight: 600, letterSpacing: "-0.01em" }}>
          Add a game
        </div>
        <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.55)", marginTop: 4 }}>
          Drop the game's executable below — Spool will identify it and look up its saves automatically.
        </div>
      </div>

      <div style={{ padding: "16px 24px 4px", display: "flex", flexDirection: "column", gap: 14 }}>
        {/* Drop zone — either empty CTA or post-drop summary */}
        {stage === "empty" ? (
          <div
            onClick={() => setStage("matched")}
            style={{
              padding: "28px 20px",
              border: "2px dashed rgba(255,255,255,0.18)",
              borderRadius: 8,
              background: "rgba(255,255,255,0.02)",
              display: "flex", flexDirection: "column", alignItems: "center", gap: 8,
              cursor: "pointer", textAlign: "center",
            }}
          >
            <div style={{
              width: 44, height: 44, borderRadius: 22,
              background: `${ACCENT}1a`,
              display: "flex", alignItems: "center", justifyContent: "center",
              color: ACCENT,
            }}><IconDownload size={20} /></div>
            <div style={{ fontSize: 13.5, fontWeight: 500 }}>Drop a game.exe here</div>
            <div style={{ fontSize: 12, color: "rgba(255,255,255,0.55)" }}>
              or <span style={{ color: ACCENT, textDecoration: "underline" }}>browse for one</span>
            </div>
          </div>
        ) : (
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
            }}><IconExe size={18} /></div>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{
                fontSize: 13, fontWeight: 500,
                whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
              }}>LegoBatmanLegacy.exe</div>
              <div style={{
                fontSize: 11, color: "rgba(255,255,255,0.5)",
                fontFamily: `"JetBrains Mono","Cascadia Code","SF Mono",ui-monospace,monospace`,
                whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
                marginTop: 2,
              }}>{exePath}</div>
            </div>
            <Button variant="ghost" onClick={() => setStage("empty")} style={{ fontSize: 11.5 }}>Change</Button>
          </div>
        )}

        {/* Match candidates from ludusavi */}
        {stage === "matched" && (
          <div>
            <div style={{
              display: "flex", alignItems: "center", justifyContent: "space-between",
              marginBottom: 8,
            }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{
                  display: "inline-flex", alignItems: "center", gap: 5,
                  fontSize: 10.5, color: ACCENT, fontWeight: 600,
                  letterSpacing: "0.08em", textTransform: "uppercase",
                }}>
                  <IconSparkle size={11} /> Auto-matched
                </span>
                <span style={{ fontSize: 12, color: "rgba(255,255,255,0.6)" }}>
                  3 candidates in ludusavi's database
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

            <div style={{
              display: "flex", flexDirection: "column", gap: 6,
            }}>
              {guesses.map((g) => {
                const isPicked = picked?.name === g.name;
                return (
                  <button
                    key={g.name}
                    onClick={() => setPicked(g)}
                    style={{
                      display: "flex", alignItems: "center", gap: 12,
                      padding: "10px 14px",
                      background: isPicked ? `${ACCENT}14` : "rgba(255,255,255,0.024)",
                      border: `1px solid ${isPicked ? ACCENT + "55" : "rgba(255,255,255,0.06)"}`,
                      borderRadius: 6,
                      color: "#fff", fontFamily: "inherit", fontSize: 13,
                      cursor: "pointer", textAlign: "left",
                      transition: "all 100ms ease",
                    }}
                  >
                    {/* Radio dot */}
                    <span style={{
                      width: 16, height: 16, borderRadius: 8,
                      border: `1.5px solid ${isPicked ? ACCENT : "rgba(255,255,255,0.3)"}`,
                      display: "flex", alignItems: "center", justifyContent: "center",
                      flexShrink: 0,
                    }}>
                      {isPicked && <span style={{ width: 7, height: 7, borderRadius: 4, background: ACCENT }} />}
                    </span>

                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{
                        display: "flex", alignItems: "center", gap: 8,
                        fontSize: 13, fontWeight: isPicked ? 500 : 400,
                      }}>
                        {g.name}
                        {g.best && (
                          <span style={{
                            fontSize: 9.5, color: ACCENT, fontWeight: 600,
                            letterSpacing: "0.08em", textTransform: "uppercase",
                          }}>Best match</span>
                        )}
                      </div>
                      <div style={{
                        fontSize: 10.5, color: "rgba(255,255,255,0.5)",
                        marginTop: 2, display: "flex", gap: 10,
                      }}>
                        <span>{g.coverage} save files tracked</span>
                      </div>
                    </div>

                    {/* Confidence bar */}
                    <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 4, minWidth: 64 }}>
                      <span style={{
                        fontSize: 10.5, color: "rgba(255,255,255,0.6)",
                        fontVariantNumeric: "tabular-nums",
                      }}>{g.confidence}% match</span>
                      <div style={{
                        width: 64, height: 3, borderRadius: 2,
                        background: "rgba(255,255,255,0.08)",
                        overflow: "hidden",
                      }}>
                        <div style={{
                          width: `${g.confidence}%`, height: "100%",
                          background: g.confidence > 70 ? ACCENT : "rgba(255,255,255,0.35)",
                          borderRadius: 2,
                        }}/>
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          </div>
        )}

        {/* Compacted advanced row */}
        <details style={{ marginTop: 4 }}>
          <summary style={{
            cursor: "pointer", listStyle: "none",
            fontSize: 12, color: "rgba(255,255,255,0.6)",
            display: "inline-flex", alignItems: "center", gap: 6,
            userSelect: "none",
          }}>
            <IconChevron size={11} /> Options
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
              <ToggleSwitch checked={admin} onChange={setAdmin} accent={ACCENT} />
              Run as Administrator
            </label>
          </div>
        </details>
      </div>

      <div style={{ flex: 1 }} />
      <DialogFooter canSubmit={stage === "matched" && picked} />
    </DialogFrame>
  );
}

window.AddGameV4 = AddGameV4;
