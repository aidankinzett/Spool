/* Spool · LAN Peer Browse — list of devices and one peer's library */

const LAN_PEERS = [
  { id: "this",  name: "Workshop · Desktop",  os: "Linux",        role: "this", count: 8,  online: true,  latency: 0 },
  { id: "deck",  name: "Living room · Deck",  os: "Linux · Deck", role: "peer", count: 8,  online: true,  latency: 4 },
  { id: "tp",    name: "Office · ThinkPad",   os: "Linux",        role: "peer", count: 22, online: true,  latency: 11 },
  { id: "media", name: "Media · Mini PC",     os: "Linux",        role: "peer", count: 14, online: false, latency: null },
];

function buildPeerLibrary() {
  return [
    { game: LIB[1], state: "match",     theirs: "v1.04",  size: 64_100, lastPlayed: "2026-05-26T09:14:00" },
    { game: LIB[2], state: "pulling",   theirs: "v0.94",  size: 14_950, lastPlayed: "2026-05-25T22:33:00", progress: 0.62 },
    { game: LIB[3], state: "available", theirs: "v1.0.2", size:  7_350, lastPlayed: "2026-05-22T18:01:00" },
    { game: LIB[7], state: "match",     theirs: "v1.1.0", size:    720, lastPlayed: "2026-05-21T16:42:00" },
    { game: { id: "ace-attorney", catalog: "PEER-014", short: "Investigations",
              name: "Ace Attorney Investigations Collection",
              art: { from: "#1e2640", to: "#04060d", accent: "#8aa6ff", mood: "Bureau" } },
              state: "available", theirs: "v1.0",  size: 4_200, lastPlayed: "2026-05-20T22:15:00" },
    { game: { id: "yakuza-pirate", catalog: "PEER-019", short: "Pirate Yakuza",
              name: "Like a Dragon: Pirate Yakuza in Hawaii",
              art: { from: "#1d0e26", to: "#040208", accent: "#ff7ad6", mood: "Tropic" } },
              state: "available", theirs: "v1.03", size: 38_900, lastPlayed: "2026-05-18T20:00:00" },
    { game: { id: "balatro", catalog: "PEER-008", short: "Balatro",
              name: "Balatro",
              art: { from: "#1f2b35", to: "#070a0c", accent: "#f4b66c", mood: "Joker" } },
              state: "available", theirs: "v1.0.1", size: 220, lastPlayed: "2026-05-15T23:50:00" },
    { game: { id: "thunder-helix", catalog: "PEER-022", short: "Thunder Helix",
              name: "Thunder Helix",
              art: { from: "#2a2310", to: "#070502", accent: "#f4ec5e", mood: "Volt" } },
              state: "available", theirs: "v0.4-beta", size: 3_200, lastPlayed: null },
  ];
}

function LanPeerWindow({ width = 1240, height = 760 }) {
  const [peerId, setPeerId] = React.useState("deck");
  const peer = LAN_PEERS.find(p => p.id === peerId) || LAN_PEERS[1];
  const peerLib = buildPeerLibrary();
  return (
    <div style={{
      width, height,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      display: "flex", flexDirection: "column",
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
    }}>
      <LanChrome />
      <div style={{ flex: 1, display: "grid", gridTemplateColumns: "280px 1fr", minHeight: 0 }}>
        <LanPeerList active={peerId} onPick={setPeerId} />
        <LanPeerDetail peer={peer} entries={peerLib} />
      </div>
    </div>
  );
}

function LanChrome() {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 12,
      height: TOK.d.desktop.titleBar,
      padding: "0 8px 0 14px",
      background: "rgba(0,0,0,0.32)",
      borderBottom: `1px solid ${TOK.c.line}`,
    }}>
      <SpoolMark size={18} color={TOK.c.ink1} tape={TOK.c.spool} />
      <MonoLabel size={10.5}>SPOOL</MonoLabel>
      <span style={{ color: TOK.c.ink3, fontSize: 10 }}>/</span>
      <MonoLabel size={10.5} color={TOK.c.ink1}>LAN · PEERS</MonoLabel>
      <span style={{ color: TOK.c.ink3, fontSize: 10 }}>·</span>
      <MonoLabel size={10.5} color={TOK.c.ok}>{ICN.wifi} 3 ONLINE</MonoLabel>
      <div style={{ flex: 1 }} />
      <ChromeIcon icon={ICN.cog} title="LAN settings" />
      <div style={{ width: 6 }} />
      <ChromeBtn glyph="min" />
      <ChromeBtn glyph="max" />
      <ChromeBtn glyph="close" />
    </div>
  );
}

function LanPeerList({ active, onPick }) {
  return (
    <aside style={{
      borderRight: `1px solid ${TOK.c.line}`,
      background: TOK.c.bg1,
      display: "flex", flexDirection: "column", minHeight: 0,
    }}>
      <div style={{ padding: "12px 14px 6px" }}>
        <MonoLabel size={10}>Devices · {LAN_PEERS.filter(p => p.online).length} online</MonoLabel>
      </div>
      <div style={{ flex: 1, overflowY: "auto" }}>
        {LAN_PEERS.map(p => <LanPeerRow key={p.id} peer={p} active={active === p.id} onClick={() => p.online && p.role !== "this" && onPick(p.id)} />)}
      </div>
      <div style={{
        padding: "10px 14px",
        borderTop: `1px solid ${TOK.c.line}`,
        fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em",
      }}>UDP · BROADCAST · :47632</div>
    </aside>
  );
}

function LanPeerRow({ peer, active, onClick }) {
  const isThis = peer.role === "this";
  const offline = !peer.online;
  return (
    <button
      onClick={onClick}
      style={{
        display: "flex", alignItems: "center", gap: 10,
        padding: "10px 12px",
        background: active ? TOK.c.bg3 : "transparent",
        borderLeft: `2px solid ${active ? TOK.c.spool : "transparent"}`,
        border: "none", width: "100%", textAlign: "left",
        cursor: offline || isThis ? "default" : "pointer",
        color: offline ? TOK.c.ink3 : "inherit",
        fontFamily: TOK.font.ui,
        opacity: offline ? 0.5 : 1,
      }}
    >
      <span style={{
        width: 30, height: 30, borderRadius: TOK.r.sm,
        background: TOK.c.bg2, border: `1px solid ${TOK.c.line2}`,
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        color: TOK.c.ink1, flexShrink: 0,
      }}>{peer.os.includes("Deck") ? ICN.controller : ICN.device}</span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 6,
          fontSize: 12.5, fontWeight: 500,
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
        }}>
          {peer.name}
          {isThis && <MonoLabel size={9} color={TOK.c.ink3}>YOU</MonoLabel>}
        </div>
        <div style={{
          fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em",
          marginTop: 2,
        }}>{peer.os} · {peer.count} games {offline ? "· offline" : peer.latency != null ? `· ${peer.latency} ms` : ""}</div>
      </div>
      <span style={{
        width: 7, height: 7, borderRadius: 4,
        background: offline ? TOK.c.ink3 : isThis ? TOK.c.spool : TOK.c.ok,
        boxShadow: offline ? "none" : `0 0 8px ${isThis ? TOK.c.spool : TOK.c.ok}88`,
        flexShrink: 0,
      }} />
    </button>
  );
}

function LanPeerDetail({ peer, entries }) {
  const peerCount = entries.length;
  const inLib = entries.filter(g => g.state === "match").length;
  const pulling = entries.filter(g => g.state === "pulling").length;
  const available = peerCount - inLib - pulling;
  return (
    <section style={{ display: "flex", flexDirection: "column", minHeight: 0, background: TOK.c.bg0 }}>
      <div style={{
        padding: "20px 28px",
        background: `linear-gradient(180deg, ${TOK.c.bg1} 0%, ${TOK.c.bg0} 100%)`,
        borderBottom: `1px solid ${TOK.c.line}`,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
          <span style={{
            width: 46, height: 46, borderRadius: TOK.r.md,
            background: TOK.c.bg2, border: `1px solid ${TOK.c.line2}`,
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            color: TOK.c.ink0,
          }}>{peer.os.includes("Deck") ? ICN.controller : ICN.device}</span>
          <div style={{ flex: 1 }}>
            <MonoLabel size={10}>{peer.os.toUpperCase()} · {peer.latency} MS · :47632</MonoLabel>
            <h1 style={{
              margin: "5px 0 0",
              fontFamily: TOK.font.display, fontSize: 26, fontWeight: 700,
              letterSpacing: "-0.02em",
            }}>{peer.name}</h1>
          </div>
          <Btn icon={ICN.share}>Share back</Btn>
          <Btn icon={ICN.cog} />
        </div>
        <div style={{
          marginTop: 16,
          display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 18,
        }}>
          <Stat label="LIBRARY" value={peerCount + ""} sub="games shared" />
          <Stat label="IN YOUR LIBRARY" value={inLib + ""} sub="already installed" />
          <Stat label="PULLING NOW" value={pulling + ""} sub="from this peer" />
          <Stat label="AVAILABLE TO PULL" value={available + ""} sub="not in your library" />
        </div>
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: "16px 28px 28px" }}>
        <div style={{
          display: "grid", gridTemplateColumns: "1fr 100px 110px 80px 200px",
          gap: 10, padding: "0 4px 8px",
          borderBottom: `1px solid ${TOK.c.line}`,
        }}>
          <MonoLabel size={9}>Title</MonoLabel>
          <MonoLabel size={9}>Their version</MonoLabel>
          <MonoLabel size={9}>Size</MonoLabel>
          <MonoLabel size={9}>Last played</MonoLabel>
          <div />
        </div>
        <div>
          {entries.map(p => <PeerGameRow key={p.game.id} entry={p} />)}
        </div>
      </div>
    </section>
  );
}

function PeerGameRow({ entry }) {
  const { game, state, theirs, size, lastPlayed, progress } = entry;
  const acc = game.art.accent;
  return (
    <div style={{
      display: "grid", gridTemplateColumns: "1fr 100px 110px 80px 200px",
      gap: 10, alignItems: "center",
      padding: "10px 4px",
      borderBottom: `1px dashed ${TOK.c.line}`,
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10, minWidth: 0 }}>
        <Cover game={game} w={36} h={50} sleeve={false} label={false} />
        <div style={{ minWidth: 0 }}>
          <div style={{
            fontSize: 13, fontWeight: 500,
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
            display: "flex", alignItems: "center", gap: 8,
          }}>
            {game.name}
            {state === "match" && <Pill kind="ok">In your library</Pill>}
            {state === "pulling" && <Pill kind="info">Pulling</Pill>}
          </div>
          <div style={{
            fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em",
            marginTop: 2,
          }}>{game.catalog || "—"} · {game.art.mood}</div>
        </div>
      </div>
      <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.04em" }}>{theirs}</span>
      <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.04em" }}>{fmtSize(size)}</span>
      <span style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3, letterSpacing: "0.04em" }}>
        {lastPlayed ? relDate(lastPlayed) : "—"}
      </span>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 8 }}>
        {state === "match" && (
          <Btn icon={ICN.check} style={{ height: 26, fontSize: 11.5, color: TOK.c.ink2 }}>Synced</Btn>
        )}
        {state === "pulling" && (
          <div style={{ flex: 1, minWidth: 0, maxWidth: 200 }}>
            <ProgressBar
              accent={acc}
              percent={Math.round((progress || 0) * 100)}
              label={`${Math.round((progress || 0) * 100)}% · ${(11.4).toFixed(1)} MB/s`}
              height={5}
            />
          </div>
        )}
        {state === "available" && (
          <Btn variant="primary" accent={acc} icon={ICN.download} style={{ height: 26, fontSize: 11.5 }}>Pull</Btn>
        )}
      </div>
    </div>
  );
}

Object.assign(window, { LAN_PEERS, LanPeerWindow });
