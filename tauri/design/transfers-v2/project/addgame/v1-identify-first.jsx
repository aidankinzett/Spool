/* Variant 1 — "Identify first"
   Reorders the form so Game Name is at the top with a clear,
   required label and a short explanation of *why* it must match
   ludusavi's database. Executable and folder move below.
   Smallest change from the original; least re-education needed. */

function AddGameV1() {
  const [name, setName] = useStateS("");
  const [exe, setExe] = useStateS("");
  const [folder, setFolder] = useStateS("");
  const [matched, setMatched] = useStateS(false);
  const [admin, setAdmin] = useStateS(false);

  return (
    <DialogFrame width={680}>
      <DialogTitleBar title="Add Game" />

      <div style={{ padding: "20px 24px 4px" }}>
        <div style={{ fontSize: 18, fontWeight: 600, letterSpacing: "-0.01em" }}>
          Add a game to your library
        </div>
        <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.55)", marginTop: 4 }}>
          Spool needs to know which game this is so it can back up the right saves through ludusavi.
        </div>
      </div>

      <div style={{ padding: "16px 24px 4px", display: "flex", flexDirection: "column", gap: 16 }}>
        {/* Game Name — pulled to top, marked required, ludusavi badge */}
        <div>
          <FieldLabel
            required
            badge={
              <span style={{
                display: "inline-flex", alignItems: "center", gap: 5,
                fontSize: 10.5, color: "rgba(255,255,255,0.55)",
                padding: "2px 7px", borderRadius: 10,
                background: "rgba(255,255,255,0.04)",
                border: "1px solid rgba(255,255,255,0.06)",
              }}>
                <IconDatabase size={10} /> matched to ludusavi
              </span>
            }
          >
            Game name
          </FieldLabel>
          <div style={{ display: "flex", gap: 8 }}>
            <TextField
              value={name}
              onChange={setName}
              placeholder="Start typing — e.g. Lego Batman, Hades, Elden Ring…"
              accent={ACCENT}
              style={{ flex: 1, height: 36 }}
              suffix={
                matched ? (
                  <span style={{
                    display: "inline-flex", alignItems: "center", gap: 4,
                    fontSize: 10.5, color: "#7ee2a4",
                    padding: "1px 6px", borderRadius: 8,
                    background: "rgba(126,226,164,0.10)",
                  }}>
                    <IconCheck size={10} /> matched
                  </span>
                ) : null
              }
            />
            <Button
              variant={name && !matched ? "primary" : "secondary"}
              accent={ACCENT}
              icon={<IconSearch size={13} />}
              onClick={() => setMatched(true)}
              style={{ height: 36, minWidth: 96 }}
            >
              Search
            </Button>
          </div>
          <div style={{
            fontSize: 11, color: "rgba(255,255,255,0.5)", marginTop: 6,
            display: "flex", alignItems: "center", gap: 6,
          }}>
            <IconInfo size={11} />
            Save backup needs an exact match — type the name, then click Search to pick from ludusavi's database.
          </div>
        </div>

        <div style={{ height: 1, background: "rgba(255,255,255,0.06)", margin: "4px 0" }} />

        <div style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: "14px 12px", alignItems: "center" }}>
          <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>Executable</div>
          <div style={{ display: "flex", gap: 8 }}>
            <TextField value={exe} onChange={setExe} placeholder="Browse to a game executable…" style={{ flex: 1 }} prefix={<IconExe size={13} />} />
            <Button variant="secondary" style={{ minWidth: 86 }}>Browse</Button>
          </div>

          <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>
            Game folder
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
      <DialogFooter canSubmit={matched && exe} />
    </DialogFrame>
  );
}

window.AddGameV1 = AddGameV1;
