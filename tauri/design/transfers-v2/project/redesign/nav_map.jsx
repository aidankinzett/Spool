/* Navigation map — explains how to reach each window */

function NavMap() {
  const nodes = [
    { id: "library", x: 380, y: 220, w: 200, h: 110, label: "LIBRARY", title: "Library", sub: "Main window · default" },
    { id: "browse",  x: 80,  y: 60,  w: 200, h: 90,  label: "BROWSE", title: "Browse sources", sub: "Hydra-style aggregator" },
    { id: "lan",     x: 680, y: 60,  w: 200, h: 90,  label: "LAN · PEERS", title: "LAN peer browse", sub: "Per-device library" },
    { id: "settings",x: 680, y: 380, w: 200, h: 90,  label: "SETTINGS", title: "Settings", sub: "Library · Sharing · Sources" },
    { id: "add",     x: 80,  y: 380, w: 200, h: 90,  label: "ADD ENTRY", title: "Add a game", sub: "Identify a .exe with ludusavi" },
    { id: "deck",    x: 380, y: 470, w: 200, h: 90,  label: "DECK", title: "Steam Deck shelf", sub: "Touch / controller mode" },
  ];

  const connections = [
    { from: "library", to: "browse",  label: "WIFI/BROWSE icon\n+ sidebar footer" },
    { from: "library", to: "lan",     label: "WIFI icon\n+ sidebar footer" },
    { from: "library", to: "settings", label: "COG icon" },
    { from: "library", to: "add",     label: "“Add a game” button" },
    { from: "library", to: "deck",    label: "auto on Steam Deck\n(touch density)" },
  ];

  return (
    <div style={{
      width: 960, height: 600,
      background: TOK.c.bg0,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      position: "relative",
      borderRadius: TOK.r.lg,
      overflow: "hidden",
      border: `1px solid ${TOK.c.line}`,
    }}>
      <div style={{
        padding: "18px 24px",
        borderBottom: `1px solid ${TOK.c.line}`,
        background: TOK.c.bg1,
        display: "flex", alignItems: "center", gap: 12,
      }}>
        <SpoolMark size={18} color={TOK.c.ink1} tape={TOK.c.spool} />
        <MonoLabel size={10.5}>SPOOL · NAVIGATION MAP</MonoLabel>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11, color: TOK.c.ink2 }}>
          Every window can be reached from the Library title bar or sidebar footer.
        </span>
      </div>

      {/* connections */}
      <svg
        width="960" height="540"
        style={{ position: "absolute", inset: "60px 0 0 0", pointerEvents: "none" }}
      >
        {connections.map(c => {
          const a = nodes.find(n => n.id === c.from);
          const b = nodes.find(n => n.id === c.to);
          const ax = a.x + a.w / 2, ay = a.y + a.h / 2;
          const bx = b.x + b.w / 2, by = b.y + b.h / 2;
          const mx = (ax + bx) / 2, my = (ay + by) / 2;
          return (
            <g key={c.from + c.to}>
              <line x1={ax} y1={ay} x2={bx} y2={by}
                stroke={TOK.c.spool + "55"} strokeWidth="1.2"
                strokeDasharray="4 4" />
              {c.label.split("\n").map((ln, i) => (
                <text key={i}
                  x={mx} y={my + i * 11 - 4}
                  textAnchor="middle"
                  fontFamily={TOK.font.mono} fontSize="9.5"
                  fill={TOK.c.ink3} letterSpacing="0.04em">
                  {ln}
                </text>
              ))}
            </g>
          );
        })}
      </svg>

      {/* nodes */}
      {nodes.map(n => (
        <div key={n.id} style={{
          position: "absolute",
          left: n.x, top: 60 + n.y, width: n.w, height: n.h,
          background: n.id === "library" ? `${TOK.c.spool}18` : TOK.c.bg1,
          border: `1px solid ${n.id === "library" ? TOK.c.spool + "88" : TOK.c.line2}`,
          borderRadius: TOK.r.md,
          padding: "12px 14px",
          display: "flex", flexDirection: "column", justifyContent: "center", gap: 4,
          boxShadow: "0 4px 12px rgba(0,0,0,0.3)",
        }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span style={{ width: 4, height: 12, background: n.id === "library" ? TOK.c.spool : TOK.c.ink2, borderRadius: 1 }} />
            <MonoLabel size={9} color={n.id === "library" ? TOK.c.spool : TOK.c.ink2}>{n.label}</MonoLabel>
            {n.id === "library" && <MonoLabel size={9} color={TOK.c.ink3}>· HOME</MonoLabel>}
          </div>
          <div style={{ fontFamily: TOK.font.display, fontSize: 17, fontWeight: 600, letterSpacing: "-0.012em" }}>
            {n.title}
          </div>
          <div style={{ fontSize: 11, color: TOK.c.ink2 }}>{n.sub}</div>
        </div>
      ))}
    </div>
  );
}

Object.assign(window, { NavMap });
