/* Spool · Conflict resolution + First-time setup */

/* ─────────────────────────── Conflict resolution dialog ─────────────────────────── */
/* Two cassettes side-by-side. Each shows: device, last save, playtime
   delta since the last common point, screenshot. User picks one to win,
   or merges by overwriting the loser. */

function ConflictDialog({ width = 720, height = 580 }) {
  const game = {
    name: "Hades II",
    short: "Hades II",
    catalog: "SPL-0031",
    art: { from: "#2a0d3d", to: "#0a020f", accent: "#bf6cf5", mood: "Arcane" },
  };

  const sides = [
    {
      side: "A",
      device: "Workshop · Desktop",
      role: "this device",
      os: "Linux",
      modified: "2026-05-27T08:14:22",
      slot: "Slot 3 · Melinoë · Vow of Adamant III",
      session: "+42 min",
      runs: "+3 runs",
      checksum: "sha-1f4c…d811",
      thisOne: true,
    },
    {
      side: "B",
      device: "Living room · Deck",
      role: "Steam Deck",
      os: "Linux · Deck",
      modified: "2026-05-27T11:02:08",
      slot: "Slot 3 · Melinoë · Vow of Adamant IV",
      session: "+1 h 18 min",
      runs: "+7 runs",
      checksum: "sha-9af0…2c33",
      recommended: true,
    },
  ];

  const [picked, setPicked] = React.useState("B");
  const acc = game.art.accent;

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: TOK.r.lg,
      overflow: "hidden",
      display: "flex", flexDirection: "column",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
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
        <MonoLabel size={10.5} color={TOK.c.ink1}>SYNC · CONFLICT</MonoLabel>
        <div style={{ flex: 1 }} />
        <ChromeBtn glyph="close" />
      </div>

      <div style={{ padding: "22px 28px 18px", borderBottom: `1px solid ${TOK.c.line}` }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
          <CatalogId id={game.catalog} accent={acc} />
          <MonoLabel size={10} color={TOK.c.warn}>{ICN.shield} TWO DEVICES, ONE SAVE</MonoLabel>
        </div>
        <h1 style={{
          margin: 0, fontFamily: TOK.font.display, fontSize: 24, fontWeight: 700,
          letterSpacing: "-0.02em",
        }}>Pick which {game.short} save to keep.</h1>
        <p style={{ margin: "6px 0 0", fontSize: 12.5, color: TOK.c.ink2, maxWidth: 580, lineHeight: 1.5 }}>
          Both devices wrote saves since the last sync. The losing side will be archived (never deleted) — restore it from the saves card if you change your mind.
        </p>
      </div>

      <div style={{ flex: 1, padding: "16px 22px", display: "grid", gridTemplateColumns: "1fr auto 1fr", gap: 14, alignItems: "stretch" }}>
        <ConflictCard side={sides[0]} picked={picked === "A"} onPick={() => setPicked("A")} acc={acc} />
        <div style={{
          display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
          gap: 8, padding: "0 4px",
        }}>
          <MonoLabel size={9}>VS</MonoLabel>
          <div style={{ width: 1, flex: 1, background: TOK.c.line, minHeight: 80 }} />
        </div>
        <ConflictCard side={sides[1]} picked={picked === "B"} onPick={() => setPicked("B")} acc={acc} />
      </div>

      <div style={{
        padding: "12px 20px",
        borderTop: `1px solid ${TOK.c.line}`,
        background: "rgba(0,0,0,0.18)",
        display: "flex", alignItems: "center", gap: 10,
      }}>
        <label style={{ display: "inline-flex", alignItems: "center", gap: 8, fontSize: 11.5, color: TOK.c.ink2, cursor: "pointer" }}>
          <span style={{
            width: 13, height: 13, borderRadius: 3, border: `1.4px solid ${TOK.c.line3}`,
            display: "inline-flex", alignItems: "center", justifyContent: "center",
          }} />
          Always trust the newest save (skip this dialog)
        </label>
        <div style={{ flex: 1 }} />
        <Btn style={{ color: TOK.c.ink2 }}>Pause sync for this game</Btn>
        <Btn variant="primary" accent={acc} style={{ minWidth: 200, height: 32, fontSize: 13 }}>
          Keep side {picked} · launch
        </Btn>
      </div>
    </div>
  );
}

function ConflictCard({ side, picked, onPick, acc }) {
  return (
    <button
      onClick={onPick}
      style={{
        textAlign: "left",
        background: picked ? `${acc}10` : TOK.c.bg1,
        border: `1px solid ${picked ? acc + "88" : TOK.c.line}`,
        borderRadius: TOK.r.md,
        padding: 0, overflow: "hidden",
        cursor: "pointer",
        display: "flex", flexDirection: "column",
        color: "inherit", fontFamily: TOK.font.ui,
        position: "relative",
      }}
    >
      {/* tape label band */}
      <div style={{
        background: picked ? acc : TOK.c.bg2,
        padding: "8px 12px",
        display: "flex", alignItems: "center", justifyContent: "space-between",
        borderBottom: `1px solid ${picked ? acc : TOK.c.line}`,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
          <span style={{
            width: 22, height: 22, borderRadius: 2,
            background: picked ? "rgba(0,0,0,0.25)" : TOK.c.bg3,
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            fontFamily: TOK.font.display, fontWeight: 700, fontSize: 12,
            color: picked ? "rgba(255,255,255,0.95)" : TOK.c.ink1,
          }}>{side.side}</span>
          <span style={{
            fontFamily: TOK.font.mono, fontSize: 10,
            color: picked ? "rgba(0,0,0,0.7)" : TOK.c.ink2,
            letterSpacing: "0.12em", textTransform: "uppercase",
          }}>SIDE {side.side}</span>
        </div>
        {side.recommended && !picked && <MonoLabel size={9} color={TOK.c.spool}>NEWER</MonoLabel>}
        {side.thisOne && !picked && <MonoLabel size={9} color={TOK.c.ink2}>THIS DEVICE</MonoLabel>}
        {picked && (
          <span style={{
            display: "inline-flex", alignItems: "center", gap: 5,
            fontFamily: TOK.font.mono, fontSize: 9.5, color: "rgba(0,0,0,0.75)", letterSpacing: "0.1em",
            fontWeight: 600,
          }}>
            <span style={{ color: "rgba(0,0,0,0.85)", display: "flex" }}>{ICN.check}</span>
            KEEPING
          </span>
        )}
      </div>

      <div style={{ padding: "14px 14px 12px", display: "flex", flexDirection: "column", gap: 12, flex: 1 }}>
        {/* device row */}
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{
            width: 28, height: 28, borderRadius: TOK.r.sm,
            background: TOK.c.bg2, border: `1px solid ${TOK.c.line2}`,
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            color: TOK.c.ink1, flexShrink: 0,
          }}>{side.os.includes("Deck") ? ICN.controller : ICN.device}</span>
          <div style={{ minWidth: 0, flex: 1 }}>
            <div style={{ fontSize: 13, fontWeight: 500, color: TOK.c.ink0 }}>{side.device}</div>
            <div style={{ fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em" }}>
              {side.os}
            </div>
          </div>
        </div>

        {/* save slot */}
        <div style={{
          background: TOK.c.bg0, padding: "10px 12px",
          border: `1px dashed ${TOK.c.line}`, borderRadius: TOK.r.sm,
        }}>
          <MonoLabel size={9}>SAVE SLOT</MonoLabel>
          <div style={{
            marginTop: 4,
            fontSize: 12.5, color: TOK.c.ink0, fontWeight: 500, lineHeight: 1.35,
          }}>{side.slot}</div>
        </div>

        {/* deltas */}
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
          <DeltaTile label="Modified" value={absDateTimeShort(side.modified)} sub={relDate(side.modified)} />
          <DeltaTile label="Since sync" value={side.session} sub={side.runs} accent={acc} />
        </div>

        <div style={{ flex: 1 }} />

        <div style={{
          fontFamily: TOK.font.mono, fontSize: 9.5, color: TOK.c.ink3, letterSpacing: "0.06em",
          paddingTop: 6,
          borderTop: `1px dashed ${TOK.c.line}`,
        }}>{side.checksum}</div>
      </div>
    </button>
  );
}

function DeltaTile({ label, value, sub, accent }) {
  return (
    <div>
      <MonoLabel size={9}>{label}</MonoLabel>
      <div style={{
        marginTop: 3,
        fontFamily: TOK.font.display, fontSize: 16, fontWeight: 600,
        letterSpacing: "-0.005em",
        color: accent || TOK.c.ink0,
      }}>{value}</div>
      <div style={{ fontSize: 10.5, color: TOK.c.ink3, marginTop: 1 }}>{sub}</div>
    </div>
  );
}

function absDateTimeShort(s) {
  const d = new Date(s);
  return d.toLocaleString("en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

/* ─────────────────────────── First-time setup ─────────────────────────── */

const ONBOARD_STEPS = [
  { id: "welcome",  label: "Welcome" },
  { id: "ludusavi", label: "Ludusavi" },
  { id: "device",   label: "This device" },
  { id: "sync",     label: "Sync" },
  { id: "library",  label: "Library" },
  { id: "done",     label: "Done" },
];

function Onboarding({ width = 860, height = 600, step: stepProp }) {
  const [step, setStep] = React.useState(stepProp || 0);
  React.useEffect(() => { if (stepProp != null) setStep(stepProp); }, [stepProp]);

  return (
    <div style={{
      width, height,
      background: TOK.c.bg0,
      color: TOK.c.ink0,
      fontFamily: TOK.font.ui,
      borderRadius: TOK.r.lg,
      overflow: "hidden",
      display: "flex", flexDirection: "column",
      boxShadow: "0 24px 60px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.06)",
    }}>
      {/* chrome */}
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
        <MonoLabel size={10.5} color={TOK.c.ink1}>FIRST RUN</MonoLabel>
        <div style={{ flex: 1 }} />
        <ChromeBtn glyph="close" />
      </div>

      {/* progress steps */}
      <div style={{
        padding: "14px 24px",
        borderBottom: `1px solid ${TOK.c.line}`,
        background: TOK.c.bg1,
        display: "flex", alignItems: "center", gap: 0,
      }}>
        {ONBOARD_STEPS.map((s, i) => (
          <React.Fragment key={s.id}>
            <div style={{
              display: "inline-flex", alignItems: "center", gap: 7,
              opacity: i > step ? 0.5 : 1,
            }}>
              <span style={{
                width: 22, height: 22, borderRadius: 3,
                background: i < step ? TOK.c.spool : i === step ? `${TOK.c.spool}26` : TOK.c.bg3,
                border: `1px solid ${i === step ? TOK.c.spool : TOK.c.line2}`,
                color: i < step ? TOK.c.bg0 : i === step ? TOK.c.spool : TOK.c.ink2,
                fontFamily: TOK.font.mono, fontSize: 10, fontWeight: 600,
                display: "inline-flex", alignItems: "center", justifyContent: "center",
              }}>{i < step ? <span style={{ display: "flex" }}>{ICN.check}</span> : String(i + 1).padStart(2, "0")}</span>
              <MonoLabel size={9.5} color={i <= step ? TOK.c.ink1 : TOK.c.ink3}>
                {s.label}
              </MonoLabel>
            </div>
            {i < ONBOARD_STEPS.length - 1 && (
              <div style={{
                flex: 1, height: 1, margin: "0 12px",
                background: i < step ? TOK.c.spool : TOK.c.line,
              }} />
            )}
          </React.Fragment>
        ))}
      </div>

      {/* body */}
      <div style={{ flex: 1, overflowY: "auto", display: "flex" }}>
        <OnboardingStep step={step} onAdvance={() => setStep(s => Math.min(ONBOARD_STEPS.length - 1, s + 1))} />
      </div>

      {/* footer */}
      <div style={{
        padding: "12px 24px",
        borderTop: `1px solid ${TOK.c.line}`,
        background: "rgba(0,0,0,0.18)",
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <Btn style={{ color: TOK.c.ink2 }} onClick={() => setStep(s => Math.max(0, s - 1))}>
          ← Back
        </Btn>
        <div style={{ flex: 1, textAlign: "center", fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3, letterSpacing: "0.06em" }}>
          STEP {step + 1} / {ONBOARD_STEPS.length} · {ONBOARD_STEPS[step].label.toUpperCase()}
        </div>
        {step === ONBOARD_STEPS.length - 1 ? (
          <Btn variant="primary" accent={TOK.c.spool} style={{ minWidth: 160, height: 32, fontSize: 13 }}>
            Open library
          </Btn>
        ) : step === 0 ? (
          <Btn variant="primary" accent={TOK.c.spool} onClick={() => setStep(1)} style={{ minWidth: 160, height: 32, fontSize: 13 }}>
            Start setup
          </Btn>
        ) : (
          <>
            <Btn style={{ color: TOK.c.ink2 }} onClick={() => setStep(s => s + 1)}>Skip for now</Btn>
            <Btn variant="primary" accent={TOK.c.spool} onClick={() => setStep(s => s + 1)} style={{ minWidth: 140, height: 32, fontSize: 13 }}>
              Continue →
            </Btn>
          </>
        )}
      </div>
    </div>
  );
}

function OnboardingStep({ step }) {
  switch (step) {
    case 0: return <OnboardWelcome />;
    case 1: return <OnboardLudusavi />;
    case 2: return <OnboardDevice />;
    case 3: return <OnboardSync />;
    case 4: return <OnboardLibrary />;
    case 5: return <OnboardDone />;
    default: return null;
  }
}

function OnboardBody({ children, illustration }) {
  return (
    <div style={{ flex: 1, display: "grid", gridTemplateColumns: illustration ? "1fr 280px" : "1fr", minHeight: 0 }}>
      <div style={{ padding: "32px 40px", overflowY: "auto", display: "flex", flexDirection: "column", gap: 18 }}>
        {children}
      </div>
      {illustration && (
        <div style={{
          background: `linear-gradient(160deg, ${TOK.c.bg1} 0%, ${TOK.c.bg0} 100%)`,
          borderLeft: `1px solid ${TOK.c.line}`,
          display: "flex", alignItems: "center", justifyContent: "center",
          padding: 28, position: "relative", overflow: "hidden",
        }}>
          <div style={{
            position: "absolute", left: -60, top: -60,
            width: 260, height: 260, borderRadius: "50%",
            background: `radial-gradient(circle at 30% 30%, ${TOK.c.spool}22, transparent 60%)`,
          }} />
          <div style={{ position: "relative", zIndex: 1 }}>{illustration}</div>
        </div>
      )}
    </div>
  );
}

function StepHeader({ kicker, title, blurb }) {
  return (
    <div>
      <MonoLabel size={10}>{kicker}</MonoLabel>
      <h2 style={{
        margin: "8px 0 8px",
        fontFamily: TOK.font.display, fontSize: 28, fontWeight: 700,
        letterSpacing: "-0.02em", textWrap: "balance", maxWidth: 460,
      }}>{title}</h2>
      <p style={{ margin: 0, fontSize: 13.5, color: TOK.c.ink2, lineHeight: 1.55, maxWidth: 460 }}>{blurb}</p>
    </div>
  );
}

/* Step 0 — Welcome */
function OnboardWelcome() {
  return (
    <OnboardBody illustration={<SpinningReels />}>
      <StepHeader
        kicker="Welcome to Spool"
        title="A quiet shelf for the games you actually play."
        blurb="Spool installs games, backs up saves through ludusavi, and shares both across the devices on your home network. Five short steps to set it up — you can change any of it later."
      />
      <div style={{
        display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10,
        marginTop: 4,
      }}>
        {[
          { icn: ICN.shield,    title: "Saves are sacred",       sub: "Ludusavi backs up before play, restores on launch." },
          { icn: ICN.wifi,      title: "Devices, not accounts",  sub: "Discovers your other Spool installs on the LAN." },
          { icn: ICN.cloud,     title: "Sync without lock-in",   sub: "Optional. Self-host the tiny sync server or skip it." },
          { icn: ICN.controller, title: "Made for handheld too", sub: "Steam Deck gets a controller-aware shelf." },
        ].map(p => (
          <div key={p.title} style={{
            display: "flex", gap: 10, padding: 12,
            background: TOK.c.bg1,
            border: `1px solid ${TOK.c.line}`,
            borderRadius: TOK.r.sm,
          }}>
            <span style={{ color: TOK.c.spool, display: "flex", paddingTop: 1 }}>{p.icn}</span>
            <div>
              <div style={{ fontSize: 12.5, fontWeight: 500 }}>{p.title}</div>
              <div style={{ fontSize: 11, color: TOK.c.ink2, marginTop: 2, lineHeight: 1.45 }}>{p.sub}</div>
            </div>
          </div>
        ))}
      </div>
    </OnboardBody>
  );
}

/* Step 1 — Ludusavi */
function OnboardLudusavi() {
  return (
    <OnboardBody>
      <StepHeader
        kicker="01 · Ludusavi"
        title="Spool delegates save backups to ludusavi."
        blurb="It's a tiny, open-source save manager. Spool won't touch any game without it, and you keep raw access to the files. We'll detect it if it's installed; otherwise, point us at the executable."
      />
      <div style={{
        padding: 14, background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`,
        borderRadius: TOK.r.md, display: "flex", flexDirection: "column", gap: 14,
      }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          padding: 12, background: "rgba(126,226,164,0.06)",
          border: `1px solid ${TOK.c.ok}44`, borderRadius: TOK.r.sm,
        }}>
          <span style={{ color: TOK.c.ok, display: "flex" }}>{ICN.check}</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12.5, fontWeight: 500 }}>Ludusavi v0.27.0 detected</div>
            <div style={{ fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink2, letterSpacing: "0.04em", marginTop: 2 }}>
              /usr/bin/ludusavi
            </div>
          </div>
          <Pill kind="ok">Ready</Pill>
        </div>
        <div>
          <MonoLabel size={9.5}>Or browse manually</MonoLabel>
          <div style={{ display: "flex", gap: 8, marginTop: 6 }}>
            <Input value="/usr/bin/ludusavi" mono />
            <Btn icon={ICN.folder}>Browse</Btn>
          </div>
        </div>
        <div style={{ fontSize: 11.5, color: TOK.c.ink3, lineHeight: 1.5 }}>
          Don't have it? <a style={{ color: TOK.c.spool }}>Install ludusavi →</a> · Spool will pick up the change.
        </div>
      </div>
    </OnboardBody>
  );
}

/* Step 2 — Device */
function OnboardDevice() {
  return (
    <OnboardBody>
      <StepHeader
        kicker="02 · This device"
        title="Name this Spool install."
        blurb="The label other devices on your network see when they discover this one. We've guessed from your hostname — change it if you want something friendlier."
      />
      <div style={{
        padding: 14, background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`,
        borderRadius: TOK.r.md, display: "flex", flexDirection: "column", gap: 14,
      }}>
        <div>
          <MonoLabel size={9.5}>Device name</MonoLabel>
          <div style={{ marginTop: 6 }}>
            <Input value="Workshop · Desktop" />
          </div>
        </div>
        <div>
          <MonoLabel size={9.5}>This kind of device</MonoLabel>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 6, marginTop: 6 }}>
            {[
              { id: "desktop",   l: "Desktop",       sub: "Always on" },
              { id: "laptop",    l: "Laptop",        sub: "Comes and goes" },
              { id: "deck",      l: "Steam Deck",    sub: "Handheld" },
              { id: "couch",     l: "Couch PC",      sub: "TV / controller" },
            ].map(t => (
              <button key={t.id} style={{
                padding: "9px 12px",
                background: t.id === "desktop" ? `${TOK.c.spool}14` : TOK.c.bg2,
                border: `1px solid ${t.id === "desktop" ? TOK.c.spool + "66" : TOK.c.line2}`,
                borderRadius: TOK.r.sm, cursor: "pointer",
                textAlign: "left", color: TOK.c.ink0, fontFamily: TOK.font.ui,
              }}>
                <div style={{ fontSize: 12.5, fontWeight: 500 }}>{t.l}</div>
                <div style={{ fontSize: 10.5, color: TOK.c.ink3, marginTop: 1 }}>{t.sub}</div>
              </button>
            ))}
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 12, paddingTop: 4 }}>
          <Toggle value={true} />
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12.5 }}>Share installs over LAN</div>
            <div style={{ fontSize: 11, color: TOK.c.ink3, marginTop: 2 }}>
              Other Spool devices can browse and copy games from this one.
            </div>
          </div>
        </div>
      </div>
    </OnboardBody>
  );
}

/* Step 3 — Sync */
function OnboardSync() {
  const [mode, setMode] = React.useState("server");
  return (
    <OnboardBody>
      <StepHeader
        kicker="03 · Sync · optional"
        title="Where saves live when this device is off."
        blurb="A small HTTP service holds the lock that prevents two devices fighting over saves. Self-host it (recommended) or skip and stay local-only — you can add it later."
      />
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {[
          { id: "server", l: "I have a sync server", sub: "Point Spool at the URL · paste an API key" },
          { id: "discover", l: "Scan my LAN", sub: "Auto-discover the server on this network" },
          { id: "none", l: "Skip — local backups only", sub: "Saves are still backed up; just not shared" },
        ].map(o => (
          <label key={o.id} style={{
            display: "flex", gap: 12, padding: "12px 14px",
            background: mode === o.id ? `${TOK.c.spool}10` : TOK.c.bg1,
            border: `1px solid ${mode === o.id ? TOK.c.spool + "66" : TOK.c.line}`,
            borderRadius: TOK.r.sm, cursor: "pointer", alignItems: "flex-start",
          }}
          onClick={() => setMode(o.id)}>
            <span style={{
              width: 16, height: 16, borderRadius: 8, marginTop: 2,
              border: `1.5px solid ${mode === o.id ? TOK.c.spool : TOK.c.line3}`,
              display: "inline-flex", alignItems: "center", justifyContent: "center", flexShrink: 0,
            }}>
              {mode === o.id && <span style={{ width: 7, height: 7, borderRadius: 4, background: TOK.c.spool }} />}
            </span>
            <div>
              <div style={{ fontSize: 12.5, fontWeight: 500 }}>{o.l}</div>
              <div style={{ fontSize: 11, color: TOK.c.ink3, marginTop: 2 }}>{o.sub}</div>
            </div>
          </label>
        ))}
      </div>
      {mode === "server" && (
        <div style={{
          padding: 14, background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`,
          borderRadius: TOK.r.sm,
          display: "flex", flexDirection: "column", gap: 10,
        }}>
          <div>
            <MonoLabel size={9.5}>Server URL</MonoLabel>
            <div style={{ marginTop: 6 }}>
              <Input value="http://nas.local:47633" mono prefix={ICN.cloud} />
            </div>
          </div>
          <div>
            <MonoLabel size={9.5}>API key</MonoLabel>
            <div style={{ marginTop: 6, display: "flex", gap: 8 }}>
              <Input value="" placeholder="Paste your key…" mono password />
              <Btn>Register…</Btn>
            </div>
          </div>
        </div>
      )}
    </OnboardBody>
  );
}

/* Step 4 — Library directory */
function OnboardLibrary() {
  return (
    <OnboardBody>
      <StepHeader
        kicker="04 · Library"
        title="Where new games go."
        blurb="Spool installs games from LAN peers or external sources into this folder. You can put existing games here later, or add them individually from anywhere on disk."
      />
      <div style={{
        padding: 14, background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`,
        borderRadius: TOK.r.md, display: "flex", flexDirection: "column", gap: 14,
      }}>
        <div>
          <MonoLabel size={9.5}>Install directory</MonoLabel>
          <div style={{ display: "flex", gap: 8, marginTop: 6 }}>
            <Input value="D:\\Games\\Spool" mono prefix={ICN.folder} />
            <Btn icon={ICN.folder}>Browse</Btn>
          </div>
          <div style={{
            display: "flex", alignItems: "center", gap: 8,
            marginTop: 8,
            fontFamily: TOK.font.mono, fontSize: 10.5, color: TOK.c.ink3, letterSpacing: "0.04em",
          }}>
            <span style={{ width: 5, height: 5, borderRadius: 3, background: TOK.c.ok }} />
            D:\ · NVMe · 248 GB free of 931 GB
          </div>
        </div>
        <div style={{
          padding: 12, background: TOK.c.bg0, border: `1px dashed ${TOK.c.line2}`,
          borderRadius: TOK.r.sm,
          display: "flex", alignItems: "flex-start", gap: 10,
        }}>
          <span style={{ color: TOK.c.spool, display: "flex", marginTop: 1 }}>{ICN.sparkle}</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, color: TOK.c.ink1 }}>
              Want to scan existing folders for games already on disk? You can do that later from Library → Scan.
            </div>
          </div>
        </div>
      </div>
    </OnboardBody>
  );
}

/* Step 5 — Done */
function OnboardDone() {
  return (
    <OnboardBody illustration={<SpinningReels />}>
      <StepHeader
        kicker="05 · You're set"
        title="The reels are loaded."
        blurb="Add a game or browse the LAN for what your other devices are sharing. Settings are always one click away in the title bar."
      />
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {[
          { icn: ICN.check, ok: true,  l: "Ludusavi · v0.27.0",  sub: "/usr/bin/ludusavi" },
          { icn: ICN.check, ok: true,  l: "Device · Workshop · Desktop",  sub: "Sharing 0 games on LAN" },
          { icn: ICN.check, ok: true,  l: "Sync · http://nas.local:47633",  sub: "Registered as Workshop · Desktop" },
          { icn: ICN.check, ok: true,  l: "Library · D:\\Games\\Spool",  sub: "248 GB free" },
        ].map(s => (
          <div key={s.l} style={{
            display: "flex", alignItems: "center", gap: 10,
            padding: "10px 12px",
            background: TOK.c.bg1, border: `1px solid ${TOK.c.line}`, borderRadius: TOK.r.sm,
          }}>
            <span style={{ color: s.ok ? TOK.c.ok : TOK.c.warn, display: "flex" }}>{s.icn}</span>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12.5 }}>{s.l}</div>
              <div style={{ fontFamily: TOK.font.mono, fontSize: 10, color: TOK.c.ink3, letterSpacing: "0.04em", marginTop: 2 }}>{s.sub}</div>
            </div>
          </div>
        ))}
      </div>
    </OnboardBody>
  );
}

Object.assign(window, {
  ConflictDialog, Onboarding,
});
