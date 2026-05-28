/* Variant 3 — "Two-step wizard"
   Splits the flow into "Identify game" and "Add files", so step 1
   is dedicated entirely to picking the right name from ludusavi.
   Strongest signal that the match is the important thing. */

function AddGameV3() {
  const [step, setStep] = useStateS(1);
  const [query, setQuery] = useStateS("Lego batman");
  const [picked, setPicked] = useStateS(null);
  const [exe, setExe] = useStateS("");
  const [folder, setFolder] = useStateS("");
  const [admin, setAdmin] = useStateS(false);

  const results = useMemoS(() => {
    if (!query.trim()) return [];
    const q = query.toLowerCase();
    return LUDUSAVI_DB_SAMPLE.filter(g => g.name.toLowerCase().includes(q));
  }, [query]);

  return (
    <DialogFrame width={680} height={560}>
      <DialogTitleBar title="Add Game" step={`Step ${step} of 2`} />

      {/* Stepper */}
      <div style={{ padding: "16px 24px 0", display: "flex", gap: 0, alignItems: "center" }}>
        <Step n={1} label="Identify game" active={step === 1} done={step > 1} />
        <div style={{ flex: 1, height: 1, background: "rgba(255,255,255,0.08)", margin: "0 12px" }} />
        <Step n={2} label="Add files" active={step === 2} done={false} disabled={!picked} />
      </div>

      {step === 1 && (
        <div style={{ padding: "20px 24px 4px", display: "flex", flexDirection: "column", gap: 12, minHeight: 0, flex: 1 }}>
          <div>
            <div style={{ fontSize: 16, fontWeight: 600 }}>Which game is this?</div>
            <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.55)", marginTop: 3 }}>
              We need ludusavi's name for your game so it knows where the save files live. Pick the closest match.
            </div>
          </div>

          <div style={{
            display: "flex", alignItems: "center", gap: 8,
            height: 40,
            background: "rgba(255,255,255,0.04)",
            border: "1px solid rgba(255,255,255,0.12)",
            borderRadius: 6,
            padding: "0 12px",
          }}>
            <IconSearch size={14} />
            <input
              value={query}
              onChange={(e) => { setQuery(e.target.value); setPicked(null); }}
              autoFocus
              placeholder="Type a game name…"
              style={{
                flex: 1, background: "transparent", border: "none", outline: "none",
                color: "#fff", fontSize: 14, fontFamily: "inherit",
              }}
            />
            <span style={{ fontSize: 11, color: "rgba(255,255,255,0.5)" }}>
              {results.length} match{results.length === 1 ? "" : "es"}
            </span>
          </div>

          <div style={{
            flex: 1,
            minHeight: 0,
            overflow: "auto",
            border: "1px solid rgba(255,255,255,0.06)",
            borderRadius: 6,
            background: "rgba(0,0,0,0.18)",
          }}>
            {results.length === 0 && (
              <div style={{ padding: 24, textAlign: "center", color: "rgba(255,255,255,0.45)", fontSize: 12.5 }}>
                Start typing to search ludusavi's database.
              </div>
            )}
            {results.map((g) => {
              const isPicked = picked?.name === g.name;
              return (
                <button
                  key={g.name}
                  onClick={() => setPicked(g)}
                  style={{
                    display: "flex", alignItems: "center", justifyContent: "space-between",
                    width: "100%", padding: "10px 14px",
                    background: isPicked ? `${ACCENT}1a` : "transparent",
                    border: "none",
                    borderLeft: isPicked ? `2px solid ${ACCENT}` : "2px solid transparent",
                    borderBottom: "1px solid rgba(255,255,255,0.04)",
                    color: "#fff", fontFamily: "inherit", fontSize: 13,
                    cursor: "pointer", textAlign: "left",
                  }}
                  onMouseEnter={(e) => { if (!isPicked) e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
                  onMouseLeave={(e) => { if (!isPicked) e.currentTarget.style.background = "transparent"; }}
                >
                  <span style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <span style={{
                      width: 22, height: 22, borderRadius: 4,
                      background: g.best ? `${ACCENT}26` : "rgba(255,255,255,0.05)",
                      display: "flex", alignItems: "center", justifyContent: "center",
                      color: g.best ? ACCENT : "rgba(255,255,255,0.65)",
                    }}>
                      <IconDatabase size={12} />
                    </span>
                    <span>
                      <div><HiName name={g.name} query={query} /></div>
                      <div style={{ fontSize: 10.5, color: "rgba(255,255,255,0.45)", marginTop: 1 }}>
                        {g.coverage} save files tracked
                        {g.best && <span style={{ color: ACCENT, marginLeft: 8 }}>· best match</span>}
                      </div>
                    </span>
                  </span>
                  {isPicked && <IconCheck size={14} />}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {step === 2 && (
        <div style={{ padding: "20px 24px 4px", display: "flex", flexDirection: "column", gap: 16 }}>
          <div>
            <div style={{ fontSize: 16, fontWeight: 600 }}>Where is {picked?.name}?</div>
            <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.55)", marginTop: 3 }}>
              Point Spool at the game's executable so Play actually launches something.
            </div>
          </div>

          <div style={{
            display: "flex", alignItems: "center", gap: 10,
            padding: "10px 12px",
            background: `${ACCENT}10`,
            border: `1px solid ${ACCENT}33`,
            borderRadius: 6,
          }}>
            <span style={{
              width: 28, height: 28, borderRadius: 4,
              background: `${ACCENT}26`,
              display: "flex", alignItems: "center", justifyContent: "center",
              color: ACCENT,
            }}><IconDatabase size={14} /></span>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12.5, fontWeight: 500 }}>{picked?.name}</div>
              <div style={{ fontSize: 11, color: "rgba(255,255,255,0.55)" }}>
                {picked?.coverage} save files will be backed up
              </div>
            </div>
            <button onClick={() => setStep(1)} style={{
              background: "transparent", border: "none",
              color: "rgba(255,255,255,0.7)", fontSize: 11.5,
              cursor: "pointer", padding: "4px 8px",
            }}>Change</button>
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: "14px 12px", alignItems: "center" }}>
            <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>Executable</div>
            <div style={{ display: "flex", gap: 8 }}>
              <TextField value={exe} onChange={setExe} placeholder="game.exe…" style={{ flex: 1 }} prefix={<IconExe size={13} />} />
              <Button variant="secondary" style={{ minWidth: 86 }}>Browse</Button>
            </div>

            <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>
              Install folder
              <div style={{ fontSize: 10.5, color: "rgba(255,255,255,0.4)", marginTop: 1 }}>for LAN share</div>
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <TextField value={folder} onChange={setFolder} placeholder="Optional" style={{ flex: 1 }} prefix={<IconFolder size={13} />} />
              <Button variant="secondary" style={{ minWidth: 86 }}>Browse</Button>
            </div>

            <div />
            <label style={{
              display: "inline-flex", alignItems: "center", gap: 10, cursor: "pointer",
              fontSize: 12.5, color: "rgba(255,255,255,0.78)",
            }}>
              <ToggleSwitch checked={admin} onChange={setAdmin} accent={ACCENT} />
              Run as Administrator
            </label>
          </div>
        </div>
      )}

      <div style={{ flex: 1 }} />

      {/* Step-aware footer */}
      <div style={{
        padding: "14px 20px 18px",
        display: "flex",
        gap: 8,
        alignItems: "center",
        borderTop: "1px solid rgba(255,255,255,0.04)",
        background: "rgba(0,0,0,0.18)",
      }}>
        {step === 2 && (
          <Button variant="ghost" icon={<IconBack size={13} />} onClick={() => setStep(1)}>
            Back
          </Button>
        )}
        <div style={{ flex: 1 }} />
        {step === 1 ? (
          <Button
            variant="primary" accent={ACCENT}
            disabled={!picked}
            onClick={() => setStep(2)}
            style={{ minWidth: 120, height: 36, fontSize: 13, fontWeight: 500 }}
          >
            Continue →
          </Button>
        ) : (
          <>
            <Button variant="secondary" disabled={!exe} style={{ height: 36, fontSize: 13 }}>
              <IconArmoury size={13} /> Armoury Crate
            </Button>
            <Button variant="secondary" disabled={!exe} style={{ height: 36, fontSize: 13 }}>
              <IconSteam size={13} /> Add to Steam
            </Button>
            <Button
              variant="primary" accent={ACCENT} disabled={!exe}
              style={{ minWidth: 130, height: 36, fontSize: 13, fontWeight: 500 }}
            >
              Add to Library
            </Button>
          </>
        )}
      </div>
    </DialogFrame>
  );
}

function Step({ n, label, active, done, disabled }) {
  const color = active ? ACCENT : done ? "#7ee2a4" : disabled ? "rgba(255,255,255,0.3)" : "rgba(255,255,255,0.55)";
  const bg = active ? `${ACCENT}26` : done ? "rgba(126,226,164,0.14)" : "rgba(255,255,255,0.04)";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <span style={{
        width: 22, height: 22, borderRadius: 11,
        background: bg, color,
        display: "flex", alignItems: "center", justifyContent: "center",
        fontSize: 11.5, fontWeight: 600,
        border: `1px solid ${active ? ACCENT + "55" : "transparent"}`,
      }}>{done ? <IconCheck size={12} /> : n}</span>
      <span style={{
        fontSize: 12.5,
        fontWeight: active ? 600 : 400,
        color: active ? "#fff" : disabled ? "rgba(255,255,255,0.4)" : "rgba(255,255,255,0.7)",
      }}>{label}</span>
    </div>
  );
}

window.AddGameV3 = AddGameV3;
