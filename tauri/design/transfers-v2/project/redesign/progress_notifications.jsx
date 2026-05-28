/* Spool · Transfers + Notifications surfaces.
   - Title-bar transfer pill (always-visible: ↓ downloads + ↑ uploads)
   - Transfers panel (slide-out from the pill, both queues in one place)
   - Toast notifications (top-right slide-in)
   - In-game-detail inline progress (when the selected game is the one downloading) */

const DOWNLOADS = [
  {
    id: "dl-1", dir: "down",
    game: { name: "Pizza Tower",  catalog: "SPL-0021", short: "Pizza Tower",
      art: { from: "#c41e1e", to: "#3d0606", accent: "#fff2a8", mood: "Neon" } },
    peer: "Workshop · Desktop",         peerKind: "lan",
    kind: "install",
    sizeMB: 720,    doneMB: 432,        speedMBs: 84.2,
    state: "downloading", eta: 4,
  },
  {
    id: "dl-2", dir: "down",
    game: { name: "Outer Wilds",  catalog: "SPL-0046", short: "Outer Wilds",
      art: { from: "#10243d", to: "#02060b", accent: "#76c8ff", mood: "Cosmic" } },
    peer: "TorBox · debrid",            peerKind: "torbox",
    kind: "install",
    sizeMB: 6144,   doneMB: 1843,       speedMBs: 11.4,
    state: "downloading", eta: 372,
  },
  {
    id: "dl-3", dir: "down",
    game: { name: "Hades II",  catalog: "SPL-0031", short: "Hades II",
      art: { from: "#2a0d3d", to: "#0a020f", accent: "#bf6cf5", mood: "Arcane" } },
    peer: "Living room · Deck",         peerKind: "lan",
    kind: "install",
    sizeMB: 14950,  doneMB: 14950,      speedMBs: 0,
    state: "extracting", extractProgress: 0.62,
  },
];

const UPLOADS = [
  {
    id: "up-1", dir: "up",
    game: { name: "Elden Ring: Nightreign", catalog: "SPL-0044", short: "Nightreign",
      art: { from: "#3c1a0d", to: "#080304", accent: "#e8a444", mood: "Ember" } },
    peer: "Sync · nas.local",           peerKind: "cloud",
    kind: "saves",
    sizeMB: 12.8,   doneMB: 8.4,        speedMBs: 4.6,
    state: "uploading", eta: 1,
  },
  {
    id: "up-2", dir: "up",
    game: { name: "Stardew Valley", catalog: "SPL-0014", short: "Stardew Valley",
      art: { from: "#2b3d18", to: "#070d04", accent: "#f3c850", mood: "Pastoral" } },
    peer: "Living room · Deck",         peerKind: "lan",
    kind: "install",
    sizeMB: 512,    doneMB: 412,        speedMBs: 76.4,
    state: "uploading", eta: 2,
  },
  {
    id: "up-3", dir: "up",
    game: { name: "Hades II",  catalog: "SPL-0031", short: "Hades II",
      art: { from: "#2a0d3d", to: "#0a020f", accent: "#bf6cf5", mood: "Arcane" } },
    peer: "Sync · nas.local",           peerKind: "cloud",
    kind: "saves",
    sizeMB: 0.8,    doneMB: 0,          speedMBs: 0,
    state: "queued",
  },
];

function pct(d) { return Math.round((d.doneMB / d.sizeMB) * 100); }
function fmtETA(s) {
  if (!s) return "—";
  if (s < 60) return s + "s";
  if (s < 3600) return Math.round(s / 60) + " min";
  return Math.round(s / 3600) + " h";
}
function fmtMB(mb) {
  if (mb < 1024) return mb.toFixed(0) + " MB";
  return (mb / 1024).toFixed(1) + " GB";
}

/* ─────────────────────────── Title-bar pill ─────────────────────────── */
/* Drops into the LibraryChrome where the LAN/Cloud/Settings icons live.
   Two arms — ↓ downloads and ↑ uploads — sharing one cassette progress
   strip. Click to open the unified Transfers panel. */
function TransferPill({ downloads, uploads, open, onToggle }) {
  const dPct = downloads.length
    ? Math.round(downloads.reduce((a, b) => a + b.doneMB, 0) /
                 downloads.reduce((a, b) => a + b.sizeMB, 0) * 100)
    : 0;
  const uPct = uploads.length
    ? Math.round(uploads.reduce((a, b) => a + b.doneMB, 0) /
                 Math.max(1, uploads.reduce((a, b) => a + b.sizeMB, 0)) * 100)
    : 0;
  return (
    <button
      onClick={onToggle}
      title="Transfers"
      style={{
        display: "inline-flex", alignItems: "stretch", gap: 0,
        height: 22, padding: 0,
        background: open ? TOK.c.bg3 : TOK.c.bg2,
        border: `1px solid ${open ? TOK.c.line3 : TOK.c.line2}`,
        borderRadius: TOK.r.sm,
        color: TOK.c.ink1, cursor: "pointer",
        overflow: "hidden",
      }}
    >
      <PillArm
        icon={ICN.download} accent={TOK.c.spool}
        count={downloads.length} percent={dPct}
      />
      <span style={{ width: 1, background: open ? TOK.c.line3 : TOK.c.line2 }} />
      <PillArm
        icon={ICN.upload} accent={TOK.c.ok}
        count={uploads.length} percent={uPct}
      />
    </button>
  );
}

function PillArm({ icon, accent, count, percent }) {
  const idle = count === 0;
  return (
    <span style={{
      display: "inline-flex", alignItems: "center", gap: 6,
      padding: "0 9px",
      opacity: idle ? 0.45 : 1,
    }}>
      <span style={{ display: "flex", color: idle ? TOK.c.ink3 : accent }}>{icon}</span>
      <span style={{
        fontFamily: TOK.font.mono, fontSize: 10, letterSpacing: "0.08em",
        color: TOK.c.ink1, minWidth: 8, textAlign: "center",
      }}>{count}</span>
      <span style={{
        display: "inline-block", width: 22, height: 3,
        background: TOK.c.bg0, borderRadius: 2, overflow: "hidden",
      }}>
        <span style={{
          display: "block", height: "100%", width: `${idle ? 0 : percent}%`,
          background: accent,
        }} />
      </span>
    </span>
  );
}

/* Back-compat alias for older artboards. */
function DownloadPill({ count, percent, open, onToggle }) {
  return (
    <TransferPill downloads={DOWNLOADS} uploads={UPLOADS} open={open} onToggle={onToggle} />
  );
}

/* ─────────────────────────── Transfers panel ─────────────────────────── */
function TransfersPanel({ downloads, uploads }) {
  const totalDone = [...downloads, ...uploads].reduce((a, b) => a + b.doneMB, 0);
  const totalSize = [...downloads, ...uploads].reduce((a, b) => a + b.sizeMB, 0);
  const totalSpeed = [...downloads, ...uploads].reduce((a, b) => a + (b.speedMBs || 0), 0);
  return (
    <div style={{
      width: 460,
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line2}`,
      borderRadius: TOK.r.md,
      boxShadow: "0 18px 48px rgba(0,0,0,0.5)",
      overflow: "hidden",
      fontFamily: TOK.font.ui,
      color: TOK.c.ink0,
    }}>
      {/* header */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "12px 14px",
        background: TOK.c.bg2,
        borderBottom: `1px dashed ${TOK.c.line}`,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ width: 4, height: 14, background: TOK.c.spool, borderRadius: 1 }} />
          <MonoLabel size={10}>Transfers</MonoLabel>
        </div>
        <Btn icon={ICN.cog} style={{ height: 22, fontSize: 11 }}>Manage</Btn>
      </div>

      {/* scrollable body */}
      <div style={{ maxHeight: 520, overflowY: "auto" }}>
        <SectionHeader
          icon={ICN.download} accent={TOK.c.spool}
          title="Downloading" count={downloads.length}
        />
        {downloads.map((d, i) => (
          <TransferRow key={d.id} d={d} last={i === downloads.length - 1} />
        ))}

        <SectionHeader
          icon={ICN.upload} accent={TOK.c.ok}
          title="Uploading" count={uploads.length}
        />
        {uploads.length === 0 ? (
          <EmptySection text="No outgoing transfers" />
        ) : uploads.map((u, i) => (
          <TransferRow key={u.id} d={u} last={i === uploads.length - 1} />
        ))}
      </div>

      {/* footer summary */}
      <div style={{
        padding: "10px 14px",
        background: TOK.c.bg0,
        borderTop: `1px solid ${TOK.c.line}`,
        display: "flex", alignItems: "center", justifyContent: "space-between",
        fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.04em",
      }}>
        <span>{fmtMB(totalDone)} / {fmtMB(totalSize)}</span>
        <span style={{ color: TOK.c.ink2 }}>{totalSpeed.toFixed(1)} MB/s</span>
        <span style={{ color: TOK.c.ink3 }}>D:\ · 248 GB free</span>
      </div>
    </div>
  );
}

function SectionHeader({ icon, accent, title, count }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 8,
      padding: "10px 14px 4px",
      background: TOK.c.bg1,
      position: "sticky", top: 0, zIndex: 1,
      borderBottom: `1px dashed ${TOK.c.line}`,
    }}>
      <span style={{ display: "flex", color: accent }}>{icon}</span>
      <MonoLabel size={9.5} color={TOK.c.ink2}>{title.toUpperCase()}</MonoLabel>
      <span style={{
        fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.08em",
      }}>· {count}</span>
    </div>
  );
}

function EmptySection({ text }) {
  return (
    <div style={{
      padding: "16px 14px",
      fontSize: 11.5, color: TOK.c.ink3,
      fontFamily: TOK.font.mono, letterSpacing: "0.04em",
      borderBottom: `1px dashed ${TOK.c.line}`,
    }}>{text}</div>
  );
}

/* Back-compat alias — older artboards still pass `items`. */
function DownloadPanel({ items }) {
  const downloads = items ? items.filter(i => (i.dir || "down") === "down") : DOWNLOADS;
  const uploads   = items ? items.filter(i => i.dir === "up") : UPLOADS;
  return <TransfersPanel downloads={downloads} uploads={uploads.length ? uploads : UPLOADS} />;
}

function TransferRow({ d, last }) {
  const acc = d.game.art.accent;
  const isQueued = d.state === "queued";
  const p = d.state === "extracting" ? Math.round(d.extractProgress * 100)
          : isQueued ? 0
          : pct(d);

  const verb =
    d.state === "extracting" ? `Unpacking · ${p}%`
    : isQueued ? "Queued · waiting"
    : d.kind === "saves"
      ? `${fmtMB(d.doneMB)} / ${fmtMB(d.sizeMB)} · saves · ${d.speedMBs.toFixed(1)} MB/s · ${fmtETA(d.eta)} left`
      : `${fmtMB(d.doneMB)} / ${fmtMB(d.sizeMB)} · ${d.speedMBs.toFixed(1)} MB/s · ${fmtETA(d.eta)} left`;

  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: "40px 1fr auto",
      gap: 12, alignItems: "center",
      padding: "12px 14px",
      borderBottom: last ? "none" : `1px dashed ${TOK.c.line}`,
      opacity: isQueued ? 0.7 : 1,
    }}>
      <Cover game={d.game} w={40} h={56} sleeve={false} label={false} />

      <div style={{ minWidth: 0 }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 8, marginBottom: 4,
        }}>
          <span style={{
            fontSize: 13, fontWeight: 500,
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
          }}>{d.game.short}</span>
          {d.kind === "saves" && <Pill kind="info" soft>Saves</Pill>}
          {d.state === "extracting" && <Pill kind="info">Extracting</Pill>}
          {isQueued && <Pill kind="warn" soft>Queued</Pill>}
        </div>

        <ProgressBar
          accent={isQueued ? TOK.c.line3 : acc}
          percent={p}
          label={verb}
          source={d.peer || d.source}
          sourceKind={d.peerKind || d.sourceKind}
          dir={d.dir || "down"}
        />
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
        <IconBtn icon={ICN.close} title="Cancel" />
      </div>
    </div>
  );
}

/* Back-compat alias */
function DownloadRow(props) { return <TransferRow {...props} />; }

function IconBtn({ icon, title }) {
  const [hover, setHover] = React.useState(false);
  return (
    <button
      title={title}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        width: 24, height: 24, borderRadius: TOK.r.sm,
        background: hover ? "rgba(255,255,255,0.06)" : "transparent",
        border: "none", color: hover ? TOK.c.ink0 : TOK.c.ink2, cursor: "pointer",
        display: "inline-flex", alignItems: "center", justifyContent: "center",
      }}
    >{icon}</button>
  );
}

/* ─────────────────────────── Progress bar ─────────────────────────── */
/* Cassette-tape progress: small tape strip on top, oxide fill underneath,
   reel-tick marks beneath. Subtle but unmistakably "Spool". */
function ProgressBar({ percent, accent, label, source, sourceKind, height = 6, dir }) {
  return (
    <div>
      <div style={{ position: "relative", height, marginBottom: 6 }}>
        {/* track */}
        <div style={{
          position: "absolute", inset: 0,
          background: TOK.c.bg0,
          borderRadius: 1,
        }} />
        {/* fill */}
        <div style={{
          position: "absolute", top: 0, left: 0, bottom: 0,
          width: `${percent}%`,
          background: accent,
          borderRadius: 1,
          boxShadow: `0 0 8px ${accent}66`,
        }} />
        {/* tick marks */}
        <div style={{
          position: "absolute", left: 0, right: 0, bottom: -3,
          height: 2,
          backgroundImage: `repeating-linear-gradient(to right, ${TOK.c.line2} 0 1px, transparent 1px 12.5%)`,
        }} />
      </div>

      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "center",
        fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink2, letterSpacing: "0.04em",
        gap: 8,
      }}>
        <span style={{
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", minWidth: 0,
        }}>{label}</span>
        {source && (
          <span style={{ display: "inline-flex", alignItems: "center", gap: 5, color: TOK.c.ink3, flexShrink: 0 }}>
            {dir === "up"
              ? <span style={{ fontFamily: TOK.font.mono, color: TOK.c.ok }}>↑</span>
              : <span style={{ fontFamily: TOK.font.mono, color: TOK.c.spool }}>↓</span>}
            <span style={{ display: "flex", color: sourceKind === "lan" ? TOK.c.ok : sourceKind === "torbox" ? TOK.c.info : sourceKind === "cloud" ? TOK.c.info : TOK.c.ink3 }}>
              {sourceKind === "lan" ? ICN.wifi : sourceKind === "torbox" ? ICN.cloud : sourceKind === "cloud" ? ICN.cloud : ICN.download}
            </span>
            {source}
          </span>
        )}
      </div>
    </div>
  );
}

/* ─────────────────────────── Inline detail progress ─────────────────────────── */
/* Goes under the hero on the library detail page when the SELECTED game
   is currently downloading — replaces the Play button. */
function DownloadingInlineBlock({ d }) {
  const acc = d.game.art.accent;
  const p = pct(d);
  return (
    <div style={{
      background: `linear-gradient(180deg, ${TOK.c.bg1} 0%, ${TOK.c.bg0} 100%)`,
      border: `1px solid ${acc}33`,
      borderRadius: TOK.r.md,
      padding: 16,
      position: "relative",
      overflow: "hidden",
    }}>
      <div style={{
        position: "absolute", top: 0, left: 0, right: 0, height: 3,
        background: `linear-gradient(90deg, ${acc}, ${acc}66, ${acc})`,
      }} />

      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        marginBottom: 10,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 9 }}>
          <span style={{
            display: "inline-flex", width: 26, height: 26, borderRadius: 14,
            background: `${acc}22`, alignItems: "center", justifyContent: "center", color: acc,
          }}>
            {ICN.download}
          </span>
          <div>
            <MonoLabel size={9.5} color={acc}>DOWNLOADING · {(d.peer || d.source).toUpperCase()}</MonoLabel>
            <div style={{
              fontFamily: TOK.font.display, fontSize: 17, fontWeight: 600, letterSpacing: "-0.01em",
              marginTop: 2,
            }}>{p}% · {fmtMB(d.doneMB)} of {fmtMB(d.sizeMB)}</div>
          </div>
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          <Btn icon={ICN.close}>Pause</Btn>
          <Btn danger icon={ICN.trash}>Cancel</Btn>
        </div>
      </div>

      <ProgressBar
        accent={acc}
        percent={p}
        label={`${d.speedMBs.toFixed(1)} MB/s · ${fmtETA(d.eta)} left`}
        height={8}
      />
    </div>
  );
}

/* ─────────────────────────── Toast notifications ─────────────────────────── */
const TOASTS = [
  {
    id: "t1", kind: "ok", title: "Saves backed up",
    sub: "Elden Ring: Nightreign · 41 revisions",
    label: "LUDUSAVI", time: "now",
    catalog: "SPL-0044", accent: TOK.c.ok,
  },
  {
    id: "t2", kind: "info", title: "Peer joined LAN",
    sub: "Living room · Deck — 8 games shared",
    label: "DISCOVERY", time: "12s",
    accent: TOK.c.info,
    cta: "Browse",
  },
  {
    id: "t3", kind: "ok", title: "Download complete",
    sub: "Pizza Tower · 720 MB · from Workshop · Desktop",
    label: "TRANSFER · LAN", time: "1m",
    catalog: "SPL-0021", accent: TOK.c.ok,
    cta: "Add to library",
  },
  {
    id: "t4", kind: "warn", title: "Save conflict",
    sub: "Hades II · Living room · Deck has newer file",
    label: "SYNC · CONFLICT", time: "4m",
    catalog: "SPL-0031", accent: TOK.c.warn,
    cta: "Resolve",
  },
  {
    id: "t5", kind: "bad", title: "Couldn't reach sync",
    sub: "http://nas.local:47633 timed out · retrying in 30s",
    label: "SYNC · OFFLINE", time: "now",
    accent: TOK.c.bad,
    cta: "Diagnose",
  },
];

function Toast({ t }) {
  return (
    <div style={{
      width: 380,
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line2}`,
      borderRadius: TOK.r.md,
      boxShadow: "0 12px 32px rgba(0,0,0,0.5)",
      overflow: "hidden",
      fontFamily: TOK.font.ui,
      position: "relative",
      display: "flex",
    }}>
      {/* left tape strip */}
      <div style={{
        width: 4, alignSelf: "stretch",
        background: t.accent,
      }} />

      <div style={{ flex: 1, padding: "12px 14px 12px 14px", minWidth: 0 }}>
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          marginBottom: 6,
        }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
            <MonoLabel size={9.5} color={t.accent}>{t.label}</MonoLabel>
            {t.catalog && (
              <span style={{
                fontFamily: TOK.font.mono, fontSize: 9, color: TOK.c.ink3, letterSpacing: "0.06em",
              }}>{t.catalog}</span>
            )}
          </div>
          <span style={{ fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em" }}>
            {t.time.toUpperCase()}
          </span>
        </div>

        <div style={{
          fontSize: 13.5, fontWeight: 600, color: TOK.c.ink0,
          letterSpacing: "-0.005em",
          marginBottom: 2,
        }}>{t.title}</div>
        <div style={{ fontSize: 11.5, color: TOK.c.ink2, lineHeight: 1.45, marginBottom: t.cta ? 10 : 0 }}>
          {t.sub}
        </div>

        {t.cta && (
          <div style={{ display: "flex", gap: 6 }}>
            <Btn style={{ height: 24, fontSize: 11.5 }}>{t.cta}</Btn>
            <Btn style={{ height: 24, fontSize: 11.5, color: TOK.c.ink2, border: "none" }}>Dismiss</Btn>
          </div>
        )}
      </div>

      <button title="Dismiss" style={{
        position: "absolute", top: 8, right: 8,
        width: 18, height: 18, borderRadius: TOK.r.sm,
        background: "transparent", border: "none",
        color: TOK.c.ink3, cursor: "pointer",
        display: "inline-flex", alignItems: "center", justifyContent: "center",
      }}>
        {ICN.close}
      </button>
    </div>
  );
}

function ToastStack() {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {TOASTS.map(t => <Toast key={t.id} t={t} />)}
    </div>
  );
}

/* ─────────────────────────── Composite scene — library with active downloads ─────────────────────────── */
function LibraryWithProgressScene() {
  const [panelOpen, setPanelOpen] = React.useState(true);
  const total = DOWNLOADS.reduce((a, b) => a + b.sizeMB, 0);
  const done = DOWNLOADS.reduce((a, b) => a + b.doneMB, 0);
  const overallPct = Math.round((done / total) * 100);

  return (
    <div style={{
      width: 1280, height: 760,
      background: TOK.c.bg0,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      display: "flex", flexDirection: "column",
      borderRadius: TOK.r.lg,
      overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
      position: "relative",
    }}>
      {/* chrome with download pill */}
      <div style={{
        display: "flex", alignItems: "center", gap: 14,
        height: TOK.d.desktop.titleBar,
        padding: "0 8px 0 14px",
        background: "rgba(0,0,0,0.32)",
        borderBottom: `1px solid ${TOK.c.line}`,
        userSelect: "none",
      }}>
        <SpoolMark size={18} color={TOK.c.ink1} tape={TOK.c.spool} />
        <MonoLabel size={10.5}>SPOOL</MonoLabel>
        <span style={{ color: TOK.c.ink3, fontSize: 10 }}>/</span>
        <MonoLabel size={10.5} color={TOK.c.ink1}>LIBRARY</MonoLabel>
        <div style={{ flex: 1 }} />

        {/* progress pill — the new bit */}
        <TransferPill
          downloads={DOWNLOADS}
          uploads={UPLOADS}
          open={panelOpen}
          onToggle={() => setPanelOpen(o => !o)}
        />
        <div style={{ width: 6 }} />

        {/* other tools */}
        <ChromeIcon icon={ICN.wifi} title="LAN peers" badge="2" />
        <ChromeIcon icon={ICN.cloud} title="Sync server" status={TOK.c.ok} />
        <ChromeIcon icon={ICN.cog} title="Settings" />
        <div style={{ width: 6 }} />
        <ChromeBtn glyph="min" />
        <ChromeBtn glyph="max" />
        <ChromeBtn glyph="close" />
      </div>

      {/* body — sidebar + detail */}
      <div style={{ flex: 1, display: "grid", gridTemplateColumns: `${TOK.d.desktop.sidebar}px 1fr`, minHeight: 0, position: "relative" }}>
        <LibrarySidebar selectedId="pizza-tower" setSelectedId={() => {}} />
        <DetailWithInlineProgress />

        {/* transfers panel anchored under the pill */}
        {panelOpen && (
          <div style={{
            position: "absolute",
            top: 6, right: 12,
            zIndex: 5,
          }}>
            <TransfersPanel downloads={DOWNLOADS} uploads={UPLOADS} />
          </div>
        )}

        {/* toasts top-right */}
        <div style={{
          position: "absolute",
          top: panelOpen ? 660 : 12,
          right: 12,
          zIndex: 4,
        }}>
          <Toast t={TOASTS[1]} />
        </div>
      </div>
    </div>
  );
}

function DetailWithInlineProgress() {
  const d = DOWNLOADS[0]; // Pizza Tower
  const fakeGame = {
    ...d.game,
    id: "pizza-tower",
    catalog: d.game.catalog,
    genres: ["Platformer", "Score-attack"],
    dev: "Tour de Pizza", pub: "Tour de Pizza",
    release: "2023-01-26", added: "2026-05-27",
    lastPlayed: null, playtime: 0, sessions: 0,
    installPath: "D:\\Games\\Spool\\PizzaTower (downloading)",
    installSize: 720, exe: "PizzaTower.exe",
    backup: { last: null, size: 0, count: 0, status: "off" },
    lan: false, sync: "info",
    description: "A 2D platformer inspired by Wario Land, with an emphasis on movement, exploration, and score-attack.",
  };

  return (
    <div style={{
      background: TOK.c.bg0,
      display: "flex", flexDirection: "column",
      overflowY: "auto",
    }}>
      <DetailHero game={fakeGame} />
      <div style={{ padding: "20px 28px 0" }}>
        <DownloadingInlineBlock d={d} />
      </div>
      <div style={{
        padding: "20px 28px 28px",
        display: "grid",
        gridTemplateColumns: "minmax(0,1.4fr) minmax(0,1fr)",
        gap: 14,
      }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, minWidth: 0 }}>
          <AboutCard game={fakeGame} acc={fakeGame.art.accent} />
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, minWidth: 0 }}>
          <DetailsCard game={fakeGame} acc={fakeGame.art.accent} />
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  DOWNLOADS, UPLOADS, TOASTS,
  TransferPill, TransfersPanel, TransferRow,
  DownloadPill, DownloadPanel, DownloadRow,
  ProgressBar, DownloadingInlineBlock,
  Toast, ToastStack,
  LibraryWithProgressScene,
});
