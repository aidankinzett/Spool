/* Spool · Save History card — timeline of backups for one game.
   Drops into the library detail page below the SavesCard, or on its own. */

function buildSaveHistory(game) {
  /* Generate a believable history from a game's backup count */
  const out = [];
  const now = new Date("2026-05-26T20:00:00");
  const n = game.backup.count || 0;
  const baseSize = (game.backup.size || 4) / Math.max(1, n);
  let curr = new Date(now);
  for (let i = 0; i < n; i++) {
    const ago = i === 0 ? 0 : Math.pow(1.35, i) * 3.6e6;
    curr = new Date(now.getTime() - ago);
    const trigger = i === 0 ? "session-end"
                  : i === 1 ? "session-start"
                  : i % 5 === 0 ? "manual"
                  : i % 7 === 0 ? "sync-incoming"
                  : "session-end";
    out.push({
      id: "rev-" + i,
      label: i === 0 ? "Latest" : `Revision ${n - i}`,
      at: curr.toISOString(),
      sizeMB: baseSize * (0.85 + Math.random() * 0.3),
      trigger,
      device: i % 4 === 1 ? "Living room · Deck"
            : i % 4 === 2 ? "Office · ThinkPad"
            : "Workshop · Desktop",
      slot: `Slot ${1 + (i % 3)} · ${["Vow of Adamant III","Eclipse Run","Tarnished VIII","Side B"][i % 4]}`,
    });
    if (out.length >= 12) break;
  }
  return out;
}

function triggerColor(t) {
  if (t === "manual") return TOK.c.warn;
  if (t === "sync-incoming") return TOK.c.info;
  return TOK.c.ok;
}
function triggerLabel(t) {
  if (t === "manual") return "MANUAL";
  if (t === "sync-incoming") return "FROM SYNC";
  if (t === "session-start") return "ON LAUNCH";
  return "ON EXIT";
}

function SaveHistoryCard({ game, acc, height }) {
  const history = buildSaveHistory(game);
  const [picked, setPicked] = React.useState(history[0]?.id);
  if (history.length === 0) return null;
  return (
    <section style={{
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line}`,
      borderRadius: TOK.r.md,
      overflow: "hidden",
      display: "flex", flexDirection: "column",
      minWidth: 0,
    }}>
      <header style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        gap: 10, padding: "10px 14px",
        borderBottom: `1px dashed ${TOK.c.line}`,
        background: TOK.c.bg2,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ width: 4, height: 14, background: acc, borderRadius: 1 }} />
          <MonoLabel size={10}>SAVE HISTORY · {history.length} REVISIONS</MonoLabel>
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          <Btn icon={ICN.filter} style={{ height: 24, fontSize: 11.5 }}>All sources</Btn>
          <Btn icon={ICN.upload} style={{ height: 24, fontSize: 11.5 }}>Back up now</Btn>
        </div>
      </header>

      <div style={{ display: "grid", gridTemplateColumns: "260px 1fr", minHeight: 0, flex: 1 }}>
        <div style={{
          borderRight: `1px solid ${TOK.c.line}`,
          maxHeight: height || 320,
          overflowY: "auto",
          position: "relative",
        }}>
          {/* Timeline rail */}
          <div style={{
            position: "absolute", left: 28, top: 12, bottom: 12, width: 1,
            background: `repeating-linear-gradient(to bottom, ${TOK.c.line2} 0 4px, transparent 4px 7px)`,
          }} />
          {history.map((h, i) => (
            <RevisionRow
              key={h.id}
              entry={h}
              picked={picked === h.id}
              first={i === 0}
              acc={acc}
              onClick={() => setPicked(h.id)}
            />
          ))}
        </div>

        <RevisionDetail entry={history.find(h => h.id === picked) || history[0]} acc={acc} game={game} />
      </div>
    </section>
  );
}

function RevisionRow({ entry, picked, first, acc, onClick }) {
  return (
    <button onClick={onClick} style={{
      display: "grid", gridTemplateColumns: "44px 1fr auto",
      gap: 8, alignItems: "center",
      width: "100%", padding: "10px 12px",
      background: picked ? `${acc}10` : "transparent",
      borderLeft: `2px solid ${picked ? acc : "transparent"}`,
      border: "none",
      cursor: "pointer", textAlign: "left",
      color: "inherit", fontFamily: TOK.font.ui,
      position: "relative",
    }}>
      {/* Timeline dot */}
      <span style={{
        width: 12, height: 12, borderRadius: 6,
        background: picked ? acc : TOK.c.bg0,
        border: `2px solid ${picked ? acc : triggerColor(entry.trigger)}`,
        zIndex: 1,
        marginLeft: 11,
        boxShadow: first ? `0 0 0 3px ${acc}33` : "none",
      }}/>
      <div style={{ minWidth: 0 }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 6,
          fontSize: 12.5, fontWeight: picked ? 500 : 400,
          color: picked ? TOK.c.ink0 : TOK.c.ink1,
        }}>
          {entry.label}
          {first && <MonoLabel size={9} color={acc}>HEAD</MonoLabel>}
        </div>
        <div style={{
          fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.05em",
          marginTop: 2,
        }}>
          <span style={{ color: triggerColor(entry.trigger) }}>{triggerLabel(entry.trigger)}</span>
          {" · "}
          {relDate(entry.at)}
        </div>
      </div>
      <span style={{
        fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink2, letterSpacing: "0.05em",
      }}>{fmtSize(entry.sizeMB)}</span>
    </button>
  );
}

function RevisionDetail({ entry, acc, game }) {
  return (
    <div style={{
      padding: "14px 18px",
      display: "flex", flexDirection: "column", gap: 12,
      minWidth: 0,
    }}>
      <div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{
            fontFamily: TOK.font.display, fontSize: 18, fontWeight: 600, letterSpacing: "-0.01em",
          }}>{entry.label}</span>
          <Pill kind={entry.trigger === "manual" ? "warn" : entry.trigger === "sync-incoming" ? "info" : "ok"}>
            {triggerLabel(entry.trigger)}
          </Pill>
        </div>
        <div style={{
          fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.05em",
          marginTop: 4,
        }}>{absDateTime(entry.at)}</div>
      </div>

      <div style={{
        display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 14,
      }}>
        <Stat label="SIZE"    value={fmtSize(entry.sizeMB)} sub="compressed" />
        <Stat label="DEVICE"  value={entry.device.split("·")[0].trim()} sub={entry.device.split("·")[1]?.trim() || ""} />
        <Stat label="SLOT"    value={entry.slot.split("·")[0].trim()} sub={entry.slot.split("·").slice(1).join("·").trim()} />
      </div>

      <div style={{
        padding: "10px 12px",
        background: TOK.c.bg0,
        border: `1px dashed ${TOK.c.line}`,
        borderRadius: TOK.r.sm,
      }}>
        <MonoLabel size={9}>CONTENTS</MonoLabel>
        <div style={{
          marginTop: 6, display: "flex", flexDirection: "column", gap: 4,
          fontFamily: TOK.font.mono, fontSize: 11, color: TOK.c.ink1,
        }}>
          <span>{game.installPath ? game.installPath.replace(/\\/g, "/") : "—"}/save/ENG0000.sl2</span>
          <span style={{ color: TOK.c.ink3 }}>{game.installPath ? game.installPath.replace(/\\/g, "/") : "—"}/save/ENG0000.sl2.bak</span>
          <span style={{ color: TOK.c.ink3 }}>%APPDATA%/EldenRingNightreign/settings.cfg</span>
        </div>
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: "auto" }}>
        <Btn icon={ICN.upload} variant="primary" accent={acc} style={{ height: 28, fontSize: 12 }}>
          Restore this revision
        </Btn>
        <Btn icon={ICN.folder} style={{ height: 28, fontSize: 12 }}>Open in ludusavi</Btn>
        <Btn icon={ICN.copy} style={{ height: 28, fontSize: 12 }}>Compare to HEAD</Btn>
        <div style={{ flex: 1 }} />
        <Btn danger icon={ICN.trash} style={{ height: 28, fontSize: 12 }}>Discard</Btn>
      </div>
    </div>
  );
}

Object.assign(window, { SaveHistoryCard });
