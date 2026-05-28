/* Spool · Dialogs & overlays that complete the existing flows.
   - RestoreDialog        — pick a revision to restore
   - EditGameDialog       — change name / art / paths / launch / share
   - DropOverlay          — full-window state when dragging files in
   - ContextMenu          — right-click on a library row
   - ConfirmDialog        — generic destructive confirm + 4 presets */

/* ─────────────────────────── RESTORE ─────────────────────────── */
function RestoreDialog({ width = 640, height = 560 }) {
  const game = LIB.find(g => g.id === "elden-ring-nightreign");
  const history = buildSaveHistory(game);
  const [picked, setPicked] = React.useState(history[3]?.id);
  const acc = game.art.accent;
  const sel = history.find(h => h.id === picked) || history[0];

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
      display: "flex", flexDirection: "column",
    }}>
      <div style={{
        display: "flex", alignItems: "center", gap: 12,
        height: TOK.d.desktop.titleBar,
        padding: "0 8px 0 14px",
        background: "rgba(0,0,0,0.32)",
        borderBottom: `1px solid ${TOK.c.line}`,
      }}>
        <SpoolMark size={18} color={TOK.c.ink1} tape={acc} />
        <MonoLabel size={10.5}>SPOOL</MonoLabel>
        <span style={{ color: TOK.c.ink3, fontSize: 10 }}>/</span>
        <MonoLabel size={10.5} color={TOK.c.ink1}>RESTORE · SAVE</MonoLabel>
        <div style={{ flex: 1 }} />
        <ChromeBtn glyph="close" />
      </div>

      <div style={{
        padding: "18px 24px 14px",
        borderBottom: `1px solid ${TOK.c.line}`,
        display: "flex", gap: 14, alignItems: "center",
      }}>
        <Cover game={game} w={56} h={78} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
            <CatalogId id={game.catalog} accent={acc} />
            <MonoLabel size={10} color={acc}>RESTORE A REVISION</MonoLabel>
          </div>
          <h1 style={{
            margin: 0, fontFamily: TOK.font.display, fontSize: 22, fontWeight: 700,
            letterSpacing: "-0.018em",
          }}>{game.name}</h1>
        </div>
      </div>

      <div style={{ flex: 1, display: "grid", gridTemplateColumns: "220px 1fr", minHeight: 0 }}>
        {/* List */}
        <div style={{
          borderRight: `1px solid ${TOK.c.line}`,
          background: TOK.c.bg1,
          overflowY: "auto",
          position: "relative",
        }}>
          <div style={{ padding: "10px 12px 6px" }}>
            <MonoLabel size={9.5}>{history.length} REVISIONS</MonoLabel>
          </div>
          <div style={{
            position: "absolute", left: 24, top: 38, bottom: 12, width: 1,
            background: `repeating-linear-gradient(to bottom, ${TOK.c.line2} 0 4px, transparent 4px 7px)`,
          }} />
          {history.map((h, i) => (
            <button
              key={h.id}
              onClick={() => setPicked(h.id)}
              style={{
                display: "grid", gridTemplateColumns: "28px 1fr",
                gap: 8, alignItems: "center",
                width: "100%", padding: "7px 12px",
                background: picked === h.id ? `${acc}10` : "transparent",
                borderLeft: `2px solid ${picked === h.id ? acc : "transparent"}`,
                border: "none", cursor: "pointer", textAlign: "left",
                color: "inherit", fontFamily: TOK.font.ui,
                position: "relative",
              }}>
              <span style={{
                width: 10, height: 10, borderRadius: 5,
                background: picked === h.id ? acc : TOK.c.bg0,
                border: `2px solid ${picked === h.id ? acc : triggerColor(h.trigger)}`,
                marginLeft: 7, zIndex: 1,
              }} />
              <div>
                <div style={{
                  fontSize: 11.5, fontWeight: picked === h.id ? 500 : 400,
                  color: picked === h.id ? TOK.c.ink0 : TOK.c.ink1,
                }}>{i === 0 ? "Latest" : `Revision ${history.length - i}`}</div>
                <div style={{
                  fontFamily: TOK.font.mono, fontSize: 9, color: TOK.c.ink3, letterSpacing: "0.04em",
                  marginTop: 1,
                }}>{relDate(h.at)} · {fmtSize(h.sizeMB)}</div>
              </div>
            </button>
          ))}
        </div>

        {/* Preview */}
        <div style={{ padding: "16px 22px", overflowY: "auto" }}>
          <div style={{
            padding: "10px 12px",
            background: "rgba(244,182,108,0.08)",
            border: `1px solid ${TOK.c.warn}44`,
            borderRadius: TOK.r.sm,
            display: "flex", alignItems: "flex-start", gap: 10,
            marginBottom: 14,
          }}>
            <span style={{ color: TOK.c.warn, display: "flex", marginTop: 2 }}>{ICN.shield}</span>
            <div style={{ fontSize: 11.5, color: TOK.c.ink1, lineHeight: 1.5 }}>
              This will overwrite your current save with a copy from <strong>{relDate(sel.at)}</strong>.
              The current save will be archived as a new revision first — nothing is permanently lost.
            </div>
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14, marginBottom: 14 }}>
            <Stat label="WHEN" value={absDateTimeShort(sel.at)} sub={relDate(sel.at)} />
            <Stat label="SOURCE" value={sel.device.split("·")[0].trim()} sub={triggerLabel(sel.trigger)} />
            <Stat label="SIZE" value={fmtSize(sel.sizeMB)} sub="compressed" />
            <Stat label="SLOT" value={sel.slot.split("·")[0].trim()} sub={sel.slot.split("·").slice(1).join(" ").trim()} />
          </div>

          <div>
            <MonoLabel size={9.5}>FILES TO REPLACE</MonoLabel>
            <div style={{
              marginTop: 6, padding: 10,
              background: TOK.c.bg1, border: `1px dashed ${TOK.c.line}`, borderRadius: TOK.r.sm,
              display: "flex", flexDirection: "column", gap: 4,
              fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink1, letterSpacing: "0.02em",
            }}>
              <span><span style={{ color: TOK.c.warn }}>~</span> %APPDATA%/EldenRingNightreign/save/ENG0000.sl2</span>
              <span style={{ color: TOK.c.ink2 }}>
                <span style={{ color: TOK.c.warn }}>~</span> %APPDATA%/EldenRingNightreign/save/ENG0000.sl2.bak
              </span>
              <span style={{ color: TOK.c.ok }}>+ %APPDATA%/EldenRingNightreign/settings.cfg <span style={{ color: TOK.c.ink3 }}>(unchanged)</span></span>
            </div>
          </div>

          <div style={{ marginTop: 14 }}>
            <label style={{ display: "inline-flex", alignItems: "center", gap: 8, fontSize: 12, color: TOK.c.ink1, cursor: "pointer" }}>
              <span style={{
                width: 13, height: 13, borderRadius: 3,
                border: `1.4px solid ${TOK.c.spool}`, background: `${TOK.c.spool}22`,
                display: "inline-flex", alignItems: "center", justifyContent: "center",
              }}>
                <svg width="9" height="9" viewBox="0 0 9 9"><path d="M1.5 4.5 3.5 6.5 7.5 2.5" fill="none" stroke={TOK.c.spool} strokeWidth="1.4" strokeLinecap="round" /></svg>
              </span>
              Archive current save before overwriting
            </label>
          </div>
        </div>
      </div>

      <div style={{
        padding: "12px 20px", borderTop: `1px solid ${TOK.c.line}`,
        background: "rgba(0,0,0,0.18)",
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <Btn style={{ color: TOK.c.ink2 }}>Cancel</Btn>
        <div style={{ flex: 1 }} />
        <Btn icon={ICN.folder}>Open in ludusavi</Btn>
        <Btn variant="primary" accent={acc} icon={ICN.upload} style={{ minWidth: 200, height: 32, fontSize: 13 }}>
          Restore this revision
        </Btn>
      </div>
    </div>
  );
}

/* ─────────────────────────── EDIT GAME ─────────────────────────── */
function EditGameDialog({ width = 720, height = 660 }) {
  const game = LIB.find(g => g.id === "elden-ring-nightreign");
  const acc = game.art.accent;
  const [tab, setTab] = React.useState("identity");
  const tabs = [
    { id: "identity", label: "Identity" },
    { id: "install",  label: "Install" },
    { id: "launch",   label: "Launch" },
    { id: "saves",    label: "Saves" },
    { id: "sharing",  label: "Sharing" },
  ];
  return (
    <div style={{
      width, height,
      background: TOK.c.bg0, color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
      display: "flex", flexDirection: "column",
    }}>
      <div style={{
        display: "flex", alignItems: "center", gap: 12,
        height: TOK.d.desktop.titleBar,
        padding: "0 8px 0 14px",
        background: "rgba(0,0,0,0.32)",
        borderBottom: `1px solid ${TOK.c.line}`,
      }}>
        <SpoolMark size={18} color={TOK.c.ink1} tape={acc} />
        <MonoLabel size={10.5}>SPOOL</MonoLabel>
        <span style={{ color: TOK.c.ink3, fontSize: 10 }}>/</span>
        <MonoLabel size={10.5} color={TOK.c.ink1}>EDIT · ENTRY</MonoLabel>
        <div style={{ flex: 1 }} />
        <ChromeBtn glyph="close" />
      </div>

      <div style={{
        padding: "16px 22px 12px",
        display: "flex", gap: 14, alignItems: "center",
        borderBottom: `1px solid ${TOK.c.line}`,
      }}>
        <Cover game={game} w={50} h={70} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <CatalogId id={game.catalog} accent={acc} />
            <MonoLabel size={10} color={acc}>EDITING</MonoLabel>
          </div>
          <div style={{
            marginTop: 4,
            fontFamily: TOK.font.display, fontSize: 18, fontWeight: 600,
            letterSpacing: "-0.012em",
          }}>{game.name}</div>
        </div>
      </div>

      <div style={{
        display: "flex", gap: 0,
        padding: "0 22px",
        borderBottom: `1px solid ${TOK.c.line}`,
        background: TOK.c.bg1,
      }}>
        {tabs.map(t => (
          <button key={t.id} onClick={() => setTab(t.id)} style={{
            padding: "10px 14px",
            background: "transparent",
            border: "none",
            borderBottom: `2px solid ${tab === t.id ? acc : "transparent"}`,
            color: tab === t.id ? TOK.c.ink0 : TOK.c.ink2,
            fontFamily: TOK.font.ui, fontSize: 12.5, fontWeight: tab === t.id ? 500 : 400,
            cursor: "pointer",
          }}>{t.label}</button>
        ))}
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: "18px 22px" }}>
        {tab === "identity" && <EditTabIdentity game={game} acc={acc} />}
        {tab === "install"  && <EditTabInstall game={game} />}
        {tab === "launch"   && <EditTabLaunch game={game} />}
        {tab === "saves"    && <EditTabSaves game={game} />}
        {tab === "sharing"  && <EditTabSharing game={game} />}
      </div>

      <div style={{
        padding: "12px 20px", borderTop: `1px solid ${TOK.c.line}`,
        background: "rgba(0,0,0,0.18)",
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <Btn danger icon={ICN.trash}>Remove from library</Btn>
        <div style={{ flex: 1 }} />
        <Btn style={{ color: TOK.c.ink2 }}>Cancel</Btn>
        <Btn variant="primary" accent={acc} style={{ minWidth: 120, height: 32, fontSize: 13 }}>
          Save changes
        </Btn>
      </div>
    </div>
  );
}

function EditField({ label, helper, children }) {
  return (
    <div style={{
      display: "grid", gridTemplateColumns: "160px 1fr", gap: 16, alignItems: "start",
      padding: "10px 0",
      borderBottom: `1px dashed ${TOK.c.line}`,
    }}>
      <div style={{ paddingTop: 6 }}>
        <div style={{ fontSize: 12.5, color: TOK.c.ink0, fontWeight: 500 }}>{label}</div>
        {helper && <div style={{ fontSize: 11, color: TOK.c.ink2, marginTop: 2, lineHeight: 1.45 }}>{helper}</div>}
      </div>
      <div>{children}</div>
    </div>
  );
}

function EditTabIdentity({ game, acc }) {
  return (
    <div>
      <EditField label="Title" helper="What shows in the library and on the detail page.">
        <Input value={game.name} />
      </EditField>
      <EditField label="Short title" helper="Used in the sidebar and toast notifications.">
        <Input value={game.short} />
      </EditField>
      <EditField label="Catalog ID" helper="Auto-assigned. Override only if you're importing from another database.">
        <Row>
          <Input value={game.catalog} mono style={{ maxWidth: 180 }} />
          <Btn>Reset</Btn>
        </Row>
      </EditField>
      <EditField label="Mood" helper="Single word printed on the cassette label. Mostly cosmetic.">
        <Input value={game.art.mood} />
      </EditField>
      <EditField label="Cover art" helper="Source for the auto-tinted accent.">
        <Row>
          <Btn icon={ICN.folder}>Browse for image…</Btn>
          <Btn icon={ICN.sparkle}>Refetch from SteamGridDB</Btn>
          <Btn style={{ color: TOK.c.ink2 }}>Reset to generated</Btn>
        </Row>
      </EditField>
    </div>
  );
}

function EditTabInstall({ game }) {
  return (
    <div>
      <EditField label="Install path">
        <Row>
          <Input value={game.installPath} mono prefix={ICN.folder} />
          <Btn>Browse</Btn>
        </Row>
      </EditField>
      <EditField label="Executable">
        <Input value={game.exe} mono prefix={ICN.exe} />
      </EditField>
      <EditField label="Install size" helper="Auto-calculated. Click Recalculate if it looks wrong.">
        <Row>
          <span style={{ fontFamily: TOK.font.mono, fontSize: 12, color: TOK.c.ink1 }}>
            {fmtSize(game.installSize)} · D:\
          </span>
          <Btn>Recalculate</Btn>
        </Row>
      </EditField>
      <EditField label="Added on">
        <Row>
          <span style={{ fontSize: 12.5, color: TOK.c.ink1 }}>{absDateTime(game.added)}</span>
          <Pill kind="off" soft>locked</Pill>
        </Row>
      </EditField>
    </div>
  );
}

function EditTabLaunch({ game }) {
  return (
    <div>
      <EditField label="Launch options" helper="Passed to the executable. Steam-compatible: %command% expands to the .exe.">
        <Input value="--launch-skip-intro --width 2560 --height 1440" mono />
      </EditField>
      <EditField label="Runner" helper="Linux/Deck only. Choose how Spool launches Windows games.">
        <Seg value="proton" options={[
          { v: "native", l: "Native" },
          { v: "proton", l: "Proton GE" },
          { v: "wine",   l: "Wine" },
        ]}/>
      </EditField>
      <EditField label="Environment" helper="One per line. KEY=VALUE.">
        <textarea
          defaultValue="DXVK_ASYNC=1\nPROTON_USE_WINED3D=0"
          style={{
            width: "100%", minHeight: 64,
            background: TOK.c.bg2, color: TOK.c.ink0,
            border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
            padding: "8px 10px", outline: "none",
            fontFamily: TOK.font.mono, fontSize: 11.5, resize: "vertical",
          }}
        />
      </EditField>
      <EditField label="Run as administrator">
        <Toggle value={false} />
      </EditField>
      <EditField label="Window">
        <Seg value="default" options={[
          { v: "default",     l: "Default" },
          { v: "borderless",  l: "Borderless" },
          { v: "fullscreen",  l: "Fullscreen" },
        ]}/>
      </EditField>
    </div>
  );
}

function EditTabSaves({ game }) {
  return (
    <div>
      <EditField label="Backup policy" helper="When Spool tells ludusavi to back up.">
        <Seg value="auto" options={[
          { v: "auto",   l: "Auto (launch + exit)" },
          { v: "exit",   l: "Exit only" },
          { v: "manual", l: "Manual" },
        ]}/>
      </EditField>
      <EditField label="Tracked locations" helper="From ludusavi's database. Add overrides if your install puts saves elsewhere.">
        <div style={{
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`,
          borderRadius: TOK.r.sm,
        }}>
          {["%APPDATA%/EldenRingNightreign/save", "%LOCALAPPDATA%/EldenRingNightreign/settings.cfg"].map((p, i) => (
            <div key={p} style={{
              display: "flex", alignItems: "center", gap: 8,
              padding: "7px 10px",
              borderBottom: i === 0 ? `1px dashed ${TOK.c.line}` : "none",
            }}>
              <span style={{ color: TOK.c.ok, display: "flex" }}>{ICN.check}</span>
              <span style={{ flex: 1, fontFamily: TOK.font.mono, fontSize: 11, color: TOK.c.ink1 }}>{p}</span>
              <Pill kind="off" soft>ludusavi</Pill>
            </div>
          ))}
        </div>
        <div style={{ marginTop: 8 }}>
          <Btn icon={ICN.plus}>Add override</Btn>
        </div>
      </EditField>
      <EditField label="Retain" helper="How many revisions to keep before pruning.">
        <Seg value="all" options={[
          { v: "10",   l: "Last 10" },
          { v: "50",   l: "Last 50" },
          { v: "all",  l: "Keep all" },
        ]}/>
      </EditField>
    </div>
  );
}

function EditTabSharing({ game }) {
  return (
    <div>
      <EditField label="Share install on LAN" helper="Other Spool devices can pull this game from this device.">
        <Toggle value={true} />
      </EditField>
      <EditField label="Sync saves" helper="Upload save revisions to the sync server so other devices can pick them up.">
        <Toggle value={true} />
      </EditField>
      <EditField label="Conflict policy" helper="Override the global setting for just this game.">
        <Seg value="global" options={[
          { v: "global",  l: "Use global" },
          { v: "newest",  l: "Trust newest" },
          { v: "prompt",  l: "Always ask" },
        ]}/>
      </EditField>
      <EditField label="Currently shared with" helper="Peers that have pulled this install in the last 30 days.">
        <div style={{
          background: TOK.c.bg2, border: `1px solid ${TOK.c.line}`,
          borderRadius: TOK.r.sm,
        }}>
          {[
            { n: "Living room · Deck",  d: "pulled 2 days ago" },
            { n: "Office · ThinkPad",   d: "pulled 3 weeks ago" },
          ].map((p, i) => (
            <div key={p.n} style={{
              display: "flex", alignItems: "center", gap: 8,
              padding: "7px 10px",
              borderBottom: i === 0 ? `1px dashed ${TOK.c.line}` : "none",
            }}>
              <span style={{ width: 7, height: 7, borderRadius: 4, background: TOK.c.ok }} />
              <span style={{ flex: 1, fontSize: 12 }}>{p.n}</span>
              <span style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em" }}>{p.d}</span>
            </div>
          ))}
        </div>
      </EditField>
    </div>
  );
}

/* ─────────────────────────── DROP OVERLAY ─────────────────────────── */
/* Full-window state shown when the user drags files over the library.
   Demonstrated overlaid on a faded copy of the library window. */
function LibraryWithDropOverlay({ width = 1280, height = 760 }) {
  return (
    <div style={{
      width, height, position: "relative",
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
    }}>
      {/* Faded library underneath */}
      <div style={{ filter: "blur(1.5px) brightness(0.55)", pointerEvents: "none" }}>
        <LibraryWindow />
      </div>

      {/* Overlay */}
      <div style={{
        position: "absolute", inset: 0,
        background: `radial-gradient(ellipse at center, ${TOK.c.spool}26 0%, rgba(0,0,0,0.72) 65%)`,
        display: "flex", alignItems: "center", justifyContent: "center",
        flexDirection: "column", gap: 24,
      }}>
        {/* Dashed cassette frame */}
        <div style={{
          width: 560, padding: "44px 50px",
          background: "rgba(0,0,0,0.55)",
          border: `2px dashed ${TOK.c.spool}88`,
          borderRadius: TOK.r.lg,
          textAlign: "center",
          display: "flex", flexDirection: "column", alignItems: "center", gap: 14,
          backdropFilter: "blur(20px)",
        }}>
          <SpinningReels />
          <MonoLabel size={11} color={TOK.c.spool}>DROP TO CATALOG</MonoLabel>
          <h2 style={{
            margin: 0,
            fontFamily: TOK.font.display, fontSize: 30, fontWeight: 700,
            letterSpacing: "-0.022em", color: TOK.c.ink0,
          }}>Release to identify · 2 files</h2>
          <p style={{
            margin: 0, fontSize: 13, color: TOK.c.ink1, maxWidth: 380, lineHeight: 1.55,
          }}>
            Spool will run ludusavi on the executables you drop and try to match them
            against its known save-game database. You can confirm matches before anything is added.
          </p>

          <div style={{
            marginTop: 6, width: "100%",
            display: "flex", flexDirection: "column", gap: 6,
          }}>
            {[
              { name: "nightreign.exe",  path: "D:\\Games\\Elden Ring - Nightreign", state: "ok" },
              { name: "Hades2.exe",      path: "D:\\Games\\Hades2",                  state: "warn" },
            ].map(f => (
              <div key={f.name} style={{
                display: "flex", alignItems: "center", gap: 10,
                padding: "9px 12px",
                background: TOK.c.bg1,
                border: `1px solid ${TOK.c.line2}`,
                borderRadius: TOK.r.sm,
                textAlign: "left",
              }}>
                <span style={{ color: TOK.c.ink1, display: "flex" }}>{ICN.exe}</span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 12.5, fontWeight: 500 }}>{f.name}</div>
                  <div style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{f.path}</div>
                </div>
                <Pill kind={f.state === "ok" ? "ok" : "warn"}>
                  {f.state === "ok" ? "1 match" : "3 candidates"}
                </Pill>
              </div>
            ))}
          </div>
        </div>

        <MonoLabel size={10} color={TOK.c.ink2}>
          ESC TO CANCEL
        </MonoLabel>
      </div>
    </div>
  );
}

/* ─────────────────────────── CONTEXT MENU ─────────────────────────── */
function LibraryWithContextMenu({ width = 1280, height = 760 }) {
  return (
    <div style={{
      width, height, position: "relative",
      borderRadius: TOK.r.lg, overflow: "hidden",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
    }}>
      <LibraryWindow initialId="elden-ring-nightreign" />
      {/* Position the menu over a sidebar row */}
      <div style={{
        position: "absolute",
        top: 200, left: 56,
        zIndex: 10,
      }}>
        <ContextMenu />
      </div>
    </div>
  );
}

function ContextMenu() {
  const game = LIB.find(g => g.id === "elden-ring-nightreign");
  const acc = game.art.accent;

  const sections = [
    [
      { icon: ICN.play,     label: "Play",                shortcut: "Enter" },
      { icon: ICN.folder,   label: "Open install folder", shortcut: "⌘O" },
      { icon: ICN.steam,    label: "Add to Steam" },
      { icon: ICN.sparkle,  label: "Add to Armoury Crate" },
    ],
    [
      { icon: ICN.upload,   label: "Back up saves now",     shortcut: "⌘B" },
      { icon: ICN.download, label: "Restore saves…",        shortcut: "⌘R" },
      { icon: ICN.clock,    label: "Save history…",         shortcut: "⌘H" },
    ],
    [
      { icon: ICN.share,    label: "Share on LAN",       toggle: true, on: true },
      { icon: ICN.cloud,    label: "Sync saves",         toggle: true, on: true },
    ],
    [
      { icon: ICN.pencil,   label: "Edit…",              shortcut: "F2" },
      { icon: ICN.trash,    label: "Remove from library…", danger: true,  shortcut: "Del" },
    ],
  ];

  return (
    <div style={{
      width: 260,
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line2}`,
      borderRadius: TOK.r.md,
      boxShadow: "0 18px 48px rgba(0,0,0,0.6)",
      overflow: "hidden",
      fontFamily: TOK.font.ui,
      color: TOK.c.ink0,
      padding: "6px 0",
    }}>
      {/* Header */}
      <div style={{
        padding: "8px 14px 10px",
        borderBottom: `1px dashed ${TOK.c.line}`,
        display: "flex", gap: 10, alignItems: "center",
      }}>
        <Cover game={game} w={24} h={34} sleeve={false} label={false} />
        <div style={{ minWidth: 0 }}>
          <div style={{ fontSize: 12, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {game.short}
          </div>
          <div style={{ fontFamily: TOK.font.mono, fontSize: 9, color: TOK.c.ink3, letterSpacing: "0.06em", marginTop: 1 }}>
            {game.catalog}
          </div>
        </div>
      </div>

      {sections.map((sec, si) => (
        <div key={si} style={{
          padding: "4px 0",
          borderBottom: si < sections.length - 1 ? `1px dashed ${TOK.c.line}` : "none",
        }}>
          {sec.map((item, ii) => (
            <ContextRow key={ii} item={item} acc={acc} />
          ))}
        </div>
      ))}
    </div>
  );
}

function ContextRow({ item, acc }) {
  const [hover, setHover] = React.useState(false);
  const danger = item.danger;
  return (
    <button
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        display: "flex", alignItems: "center", gap: 10,
        width: "100%", padding: "6px 12px",
        height: 28,
        background: hover
          ? (danger ? "rgba(255,122,122,0.14)" : `${acc}1a`)
          : "transparent",
        border: "none", color: danger ? (hover ? "#ffa6a6" : TOK.c.ink1) : (hover ? TOK.c.ink0 : TOK.c.ink1),
        fontFamily: TOK.font.ui, fontSize: 12,
        cursor: "pointer", textAlign: "left",
      }}
    >
      <span style={{ display: "flex", color: hover && !danger ? acc : (danger ? "currentColor" : TOK.c.ink2) }}>{item.icon}</span>
      <span style={{ flex: 1 }}>{item.label}</span>
      {item.toggle != null && (
        <span style={{
          width: 7, height: 7, borderRadius: 4,
          background: item.on ? TOK.c.ok : TOK.c.ink3,
        }} />
      )}
      {item.shortcut && (
        <span style={{
          fontFamily: TOK.font.mono, fontSize: 9.5,
          color: TOK.c.ink3, letterSpacing: "0.04em",
        }}>{item.shortcut}</span>
      )}
    </button>
  );
}

/* ─────────────────────────── CONFIRM DIALOGS ─────────────────────────── */
function ConfirmCard({ kind = "warn", icon, kicker, title, blurb, primary, secondary, danger, extras }) {
  return (
    <div style={{
      width: 460,
      background: TOK.c.bg1,
      border: `1px solid ${TOK.c.line2}`,
      borderRadius: TOK.r.md,
      boxShadow: "0 18px 48px rgba(0,0,0,0.55)",
      overflow: "hidden",
      fontFamily: TOK.font.ui, color: TOK.c.ink0,
    }}>
      {/* tape strip */}
      <div style={{
        height: 3,
        background: kind === "danger" ? TOK.c.bad : kind === "warn" ? TOK.c.warn : TOK.c.info,
      }} />
      <div style={{ padding: "20px 22px 16px" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{
            color: kind === "danger" ? TOK.c.bad : kind === "warn" ? TOK.c.warn : TOK.c.info,
            display: "flex",
          }}>{icon}</span>
          <MonoLabel size={10} color={kind === "danger" ? TOK.c.bad : kind === "warn" ? TOK.c.warn : TOK.c.info}>
            {kicker}
          </MonoLabel>
        </div>
        <h2 style={{
          margin: "8px 0 6px",
          fontFamily: TOK.font.display, fontSize: 20, fontWeight: 700,
          letterSpacing: "-0.015em", textWrap: "balance",
        }}>{title}</h2>
        <p style={{ margin: 0, fontSize: 12.5, color: TOK.c.ink2, lineHeight: 1.5, textWrap: "pretty" }}>
          {blurb}
        </p>

        {extras && <div style={{ marginTop: 14 }}>{extras}</div>}
      </div>

      <div style={{
        padding: "10px 16px",
        borderTop: `1px solid ${TOK.c.line}`,
        background: TOK.c.bg0,
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <div style={{ flex: 1 }} />
        <Btn style={{ color: TOK.c.ink2 }}>{secondary || "Cancel"}</Btn>
        {danger
          ? <Btn danger icon={ICN.trash} style={{ minWidth: 140, height: 30, fontSize: 12.5, background: "rgba(255,122,122,0.18)", color: "#ffa6a6", borderColor: "rgba(255,122,122,0.55)" }}>{primary}</Btn>
          : <Btn variant="primary" accent={TOK.c.spool} style={{ minWidth: 140, height: 30, fontSize: 12.5 }}>{primary}</Btn>}
      </div>
    </div>
  );
}

function ConfirmGrid() {
  return (
    <div style={{
      width: 1000, height: 600,
      background: TOK.c.bg0,
      fontFamily: TOK.font.ui,
      padding: 28,
      display: "grid", gridTemplateColumns: "1fr 1fr", gap: 24, alignItems: "start",
    }}>
      <ConfirmCard
        kind="danger" danger
        icon={ICN.trash}
        kicker="REMOVE FROM LIBRARY"
        title="Remove Elden Ring: Nightreign?"
        blurb="The catalog entry goes away. Game files on D:\\ stay where they are, and your save backups are kept — re-add the game later to recover both."
        primary="Remove entry"
        extras={
          <label style={{ display: "inline-flex", alignItems: "center", gap: 8, fontSize: 12, color: TOK.c.ink1 }}>
            <span style={{ width: 13, height: 13, border: `1.4px solid ${TOK.c.line3}`, borderRadius: 3 }} />
            Also delete game files (64.1 GB) from disk
          </label>
        }
      />
      <ConfirmCard
        kind="danger" danger
        icon={ICN.trash}
        kicker="DISCARD REVISION"
        title="Discard revision 9?"
        blurb="The save from 17 May 2026 will be deleted permanently. The other 11 revisions stay."
        primary="Discard"
      />
      <ConfirmCard
        kind="warn"
        icon={ICN.close}
        kicker="CANCEL DOWNLOAD"
        title="Cancel Outer Wilds?"
        blurb="6.1 GB downloaded so far will be kept for 24 hours so you can resume. After that we clean it up."
        primary="Cancel download"
      />
      <ConfirmCard
        kind="warn"
        icon={ICN.wifi}
        kicker="FORGET PEER"
        title="Forget Office · ThinkPad?"
        blurb="The device disappears from your LAN list and can no longer pull games from you without re-pairing. Your saves are unaffected."
        primary="Forget peer"
        extras={
          <label style={{ display: "inline-flex", alignItems: "center", gap: 8, fontSize: 12, color: TOK.c.ink1 }}>
            <span style={{ width: 13, height: 13, border: `1.4px solid ${TOK.c.line3}`, borderRadius: 3 }} />
            Block future discovery from this device
          </label>
        }
      />
    </div>
  );
}

Object.assign(window, {
  RestoreDialog,
  EditGameDialog,
  LibraryWithDropOverlay,
  LibraryWithContextMenu, ContextMenu,
  ConfirmCard, ConfirmGrid,
});
