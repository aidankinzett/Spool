/* "Current" — faithful reproduction of the existing dialog
   (so all four redesigns can be compared against the baseline). */

function AddGameCurrent() {
  const [exe, setExe] = useStateS("");
  const [folder, setFolder] = useStateS("");
  const [name, setName] = useStateS("");
  const [admin, setAdmin] = useStateS(false);

  return (
    <DialogFrame width={660}>
      <DialogTitleBar title="Add Game" />

      <div style={{ padding: "16px 20px 8px" }}>
        <div style={{ fontSize: 13, color: "rgba(255,255,255,0.8)" }}>
          Add a game to your library with save management
        </div>
        <div style={{ height: 1, background: "rgba(255,255,255,0.06)", marginTop: 14 }} />
      </div>

      <div style={{
        padding: "8px 20px 4px",
        display: "grid",
        gridTemplateColumns: "110px 1fr",
        gap: "12px 12px",
        alignItems: "center",
      }}>
        <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.8)" }}>Executable</div>
        <div style={{ display: "flex", gap: 8 }}>
          <TextField value={exe} onChange={setExe} placeholder="Browse to a game executable…" style={{ flex: 1 }} />
          <Button variant="secondary" style={{ minWidth: 86 }}>Browse</Button>
        </div>

        <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.8)" }}>
          Game Folder
          <div style={{ fontSize: 10.5, color: "rgba(255,255,255,0.45)", marginTop: 1 }}>(for LAN share)</div>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <TextField value={folder} onChange={setFolder} placeholder="Optional — root folder of game installation" style={{ flex: 1 }} />
          <Button variant="secondary" style={{ minWidth: 86 }}>Browse</Button>
        </div>

        <div style={{ fontSize: 12.5, color: "rgba(255,255,255,0.8)" }}>Game Name</div>
        <div style={{ display: "flex", gap: 8 }}>
          <TextField value={name} onChange={setName} placeholder="Enter or search for game name…" style={{ flex: 1 }} />
          <Button variant="secondary" style={{ minWidth: 86 }}>Search</Button>
        </div>

        <div />
        <label style={{
          display: "inline-flex", alignItems: "center", gap: 8, cursor: "pointer",
          fontSize: 12.5, color: "rgba(255,255,255,0.78)", paddingTop: 4,
        }}>
          <span style={{
            width: 14, height: 14, borderRadius: 2,
            border: "1px solid rgba(255,255,255,0.4)",
            background: admin ? ACCENT : "transparent",
            display: "inline-block",
          }} onClick={() => setAdmin(!admin)} />
          Run as Administrator
        </label>
      </div>

      <div style={{ flex: 1, minHeight: 16 }} />
      <DialogFooter canSubmit />
    </DialogFrame>
  );
}

window.AddGameCurrent = AddGameCurrent;
