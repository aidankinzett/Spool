/* Variant 2 — "Live combobox"
   The Game Name field IS the search — typing reveals a live dropdown
   of matches from ludusavi's database. No separate Search button,
   no two-step interaction. Coverage chips communicate which entries
   ludusavi knows save locations for. */

function AddGameV2() {
  const [name, setName] = useStateS("Lego batman");
  const [exe, setExe] = useStateS("");
  const [folder, setFolder] = useStateS("");
  const [picked, setPicked] = useStateS(null);
  const [admin, setAdmin] = useStateS(false);
  const [open, setOpen] = useStateS(true);
  const inputRef = useRefS(null);

  const results = useMemoS(() => {
    if (!name.trim()) return [];
    const q = name.toLowerCase();
    return LUDUSAVI_DB_SAMPLE.filter(g => g.name.toLowerCase().includes(q)).slice(0, 5);
  }, [name]);

  return (
    <DialogFrame width={680}>
      <DialogTitleBar title="Add Game" />

      <div style={{ padding: "20px 24px 4px" }}>
        <div style={{ fontSize: 18, fontWeight: 600, letterSpacing: "-0.01em" }}>
          Add a game to your library
        </div>
        <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.55)", marginTop: 4 }}>
          Pick the game first — Spool uses the name to find its save files through ludusavi.
        </div>
      </div>

      <div style={{ padding: "16px 24px 4px", display: "flex", flexDirection: "column", gap: 16 }}>
        {/* Combobox — typing shows ludusavi matches inline */}
        <div style={{ position: "relative" }}>
          <FieldLabel required>Game</FieldLabel>
          <div style={{
            display: "flex", alignItems: "center", gap: 8,
            height: 40,
            background: "rgba(255,255,255,0.04)",
            border: `1px solid ${open && results.length ? ACCENT : "rgba(255,255,255,0.12)"}`,
            borderRadius: 6,
            padding: "0 12px",
            transition: "border-color 120ms ease",
          }}>
            <IconSearch size={14} />
            <input
              ref={inputRef}
              value={name}
              onChange={(e) => { setName(e.target.value); setPicked(null); setOpen(true); }}
              onFocus={() => setOpen(true)}
              placeholder="Search ludusavi's database — start typing a game name"
              style={{
                flex: 1, minWidth: 0,
                background: "transparent", border: "none", outline: "none",
                color: "#fff", fontSize: 14, fontFamily: "inherit",
              }}
            />
            {picked && (
              <span style={{
                display: "inline-flex", alignItems: "center", gap: 4,
                fontSize: 11, color: "#7ee2a4",
                padding: "2px 8px", borderRadius: 10,
                background: "rgba(126,226,164,0.10)",
              }}>
                <IconCheck size={11} /> matched
              </span>
            )}
          </div>

          {/* Dropdown */}
          {open && results.length > 0 && !picked && (
            <div style={{
              position: "absolute",
              top: "calc(100% + 4px)", left: 0, right: 0,
              background: "#1f1f1f",
              border: "1px solid rgba(255,255,255,0.10)",
              borderRadius: 6,
              padding: 4,
              zIndex: 10,
              boxShadow: "0 12px 28px rgba(0,0,0,0.55)",
              maxHeight: 240, overflow: "auto",
            }}>
              <div style={{
                padding: "6px 10px",
                fontSize: 10.5,
                color: "rgba(255,255,255,0.45)",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                display: "flex", justifyContent: "space-between",
              }}>
                <span>From ludusavi · {results.length} match{results.length === 1 ? "" : "es"}</span>
                <span>Saves found</span>
              </div>
              {results.map((g, i) => (
                <button
                  key={g.name}
                  onClick={() => { setName(g.name); setPicked(g); setOpen(false); }}
                  style={{
                    display: "flex", alignItems: "center", justifyContent: "space-between",
                    width: "100%", padding: "8px 10px",
                    background: g.best ? "rgba(76,194,255,0.08)" : "transparent",
                    border: "none", borderRadius: 4,
                    color: "#fff", fontFamily: "inherit", fontSize: 13,
                    cursor: "pointer", textAlign: "left",
                  }}
                  onMouseEnter={(e) => e.currentTarget.style.background = "rgba(255,255,255,0.06)"}
                  onMouseLeave={(e) => e.currentTarget.style.background = g.best ? "rgba(76,194,255,0.08)" : "transparent"}
                >
                  <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <IconDatabase size={12} />
                    <span><HiName name={g.name} query={name} /></span>
                    {g.best && (
                      <span style={{
                        fontSize: 9.5, color: ACCENT, fontWeight: 600,
                        letterSpacing: "0.08em", textTransform: "uppercase",
                      }}>Best match</span>
                    )}
                  </span>
                  <span style={{
                    fontSize: 11, color: "rgba(255,255,255,0.5)",
                    fontVariantNumeric: "tabular-nums",
                  }}>{g.coverage} files</span>
                </button>
              ))}
            </div>
          )}
        </div>

        <div style={{ height: 1, background: "rgba(255,255,255,0.06)" }} />

        <div style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: "14px 12px", alignItems: "center" }}>
          <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>Executable</div>
          <div style={{ display: "flex", gap: 8 }}>
            <TextField value={exe} onChange={setExe} placeholder="Browse to game.exe…" style={{ flex: 1 }} prefix={<IconExe size={13} />} />
            <Button variant="secondary" style={{ minWidth: 86 }}>Browse</Button>
          </div>

          <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>
            Install folder
            <div style={{ fontSize: 10.5, color: "rgba(255,255,255,0.4)", marginTop: 1 }}>for LAN share</div>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <TextField value={folder} onChange={setFolder} placeholder="Optional — root install folder" style={{ flex: 1 }} prefix={<IconFolder size={13} />} />
            <Button variant="secondary" style={{ minWidth: 86 }}>Browse</Button>
          </div>

          <div />
          <label style={{
            display: "inline-flex", alignItems: "center", gap: 10, cursor: "pointer",
            fontSize: 12.5, color: "rgba(255,255,255,0.78)", paddingTop: 2,
          }}>
            <ToggleSwitch checked={admin} onChange={setAdmin} accent={ACCENT} />
            Run as Administrator
          </label>
        </div>
      </div>

      <div style={{ flex: 1 }} />
      <DialogFooter canSubmit={picked && exe} />
    </DialogFrame>
  );
}

window.AddGameV2 = AddGameV2;
