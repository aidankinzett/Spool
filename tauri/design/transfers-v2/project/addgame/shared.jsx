/* Add Game dialog — shared chrome and primitives.
   All variations share the same window frame, title bar, footer button
   layout, and `Run as Administrator` toggle so we're comparing the
   *form design* not the chrome. */

const { useState: useStateS, useRef: useRefS, useEffect: useEffectS, useMemo: useMemoS } = React;

const ACCENT = "#4cc2ff";

/* ------------- Window frame: matches Settings/Library scale ------------- */
function DialogFrame({ children, width = 660, height }) {
  return (
    <div style={{
      width,
      height,
      background: "linear-gradient(180deg, rgba(28,28,28,0.96) 0%, rgba(22,22,22,0.98) 100%)",
      borderRadius: 8,
      border: "1px solid rgba(255,255,255,0.06)",
      boxShadow: "0 20px 60px rgba(0,0,0,0.55), 0 4px 16px rgba(0,0,0,0.4)",
      display: "flex",
      flexDirection: "column",
      overflow: "hidden",
      color: "#fff",
      fontFamily: `"Segoe UI Variable Text","Segoe UI Variable","Segoe UI","Inter",-apple-system,sans-serif`,
      position: "relative",
    }}>
      <div style={{
        position: "absolute",
        top: -240, left: -160,
        width: 600, height: 600,
        background: `radial-gradient(circle, ${ACCENT}0e, transparent 60%)`,
        pointerEvents: "none",
      }}/>
      {children}
    </div>
  );
}

function DialogTitleBar({ title = "Add Game", step }) {
  return (
    <div style={{
      height: 40,
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "0 0 0 16px",
      flexShrink: 0,
      borderBottom: "1px solid rgba(255,255,255,0.04)",
      WebkitUserSelect: "none",
      position: "relative",
      zIndex: 2,
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <SpoolMark size={14} fg="rgba(255,255,255,0.85)" />
        <span style={{ fontSize: 12, color: "rgba(255,255,255,0.88)", fontWeight: 500 }}>{title}</span>
        {step && (
          <span style={{
            fontSize: 11,
            color: "rgba(255,255,255,0.5)",
            paddingLeft: 8,
            marginLeft: 4,
            borderLeft: "1px solid rgba(255,255,255,0.08)",
          }}>{step}</span>
        )}
      </div>
      <div style={{ display: "flex" }}>
        <TbBtn><IconMinimize size={11} /></TbBtn>
        <TbBtn danger><IconClose size={11} /></TbBtn>
      </div>
    </div>
  );
}
function TbBtn({ children, danger }) {
  const [hover, setHover] = useStateS(false);
  return (
    <button
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        width: 46, height: 40,
        background: hover ? (danger ? "#c42b1c" : "rgba(255,255,255,0.06)") : "transparent",
        color: hover && danger ? "#fff" : "rgba(255,255,255,0.78)",
        border: "none", cursor: "pointer",
        display: "flex", alignItems: "center", justifyContent: "center",
      }}
    >{children}</button>
  );
}

/* ------------- Footer: Add to Library + shortcut buttons ------------- */
function DialogFooter({ canSubmit = false, primaryLabel = "Add to Library" }) {
  return (
    <div style={{
      padding: "14px 20px 20px",
      display: "flex",
      gap: 8,
      alignItems: "center",
      borderTop: "1px solid rgba(255,255,255,0.04)",
      background: "rgba(0,0,0,0.18)",
    }}>
      <Button variant="primary" accent={ACCENT} disabled={!canSubmit}
              style={{ flex: 1.4, height: 36, fontSize: 13, fontWeight: 500 }}>
        {primaryLabel}
      </Button>
      <Button variant="secondary" disabled={!canSubmit}
              style={{ flex: 1, height: 36, fontSize: 13 }}>
        <IconArmoury size={13} />
        Armoury Crate
      </Button>
      <Button variant="secondary" disabled={!canSubmit}
              style={{ flex: 1, height: 36, fontSize: 13 }}>
        <IconSteam size={13} />
        Add to Steam
      </Button>
    </div>
  );
}

/* ------------- A nice labelled section header ------------- */
function FieldLabel({ children, hint, required, badge }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 8,
      marginBottom: 6,
    }}>
      <span style={{
        fontSize: 12,
        fontWeight: 500,
        color: "rgba(255,255,255,0.85)",
        letterSpacing: "0.01em",
      }}>{children}</span>
      {required && (
        <span style={{
          fontSize: 9.5,
          fontWeight: 600,
          color: ACCENT,
          letterSpacing: "0.08em",
          textTransform: "uppercase",
        }}>Required</span>
      )}
      {badge}
      {hint && (
        <span style={{
          fontSize: 11,
          color: "rgba(255,255,255,0.5)",
          marginLeft: "auto",
          textAlign: "right",
        }}>{hint}</span>
      )}
    </div>
  );
}

/* ------------- Icons (extra) ------------- */
const IconArmoury = (p) => (
  <Icon {...p}>
    <path d="M12 2 4 6v6c0 5 3.5 9 8 10 4.5-1 8-5 8-10V6l-8-4z" />
  </Icon>
);
const IconSteam = (p) => (
  <Icon {...p}>
    <circle cx="12" cy="12" r="9" />
    <circle cx="15.5" cy="9.5" r="2.5" />
    <path d="M3 14l4.5 2" />
    <circle cx="8" cy="16.5" r="2" />
  </Icon>
);
const IconExe = (p) => (
  <Icon {...p}>
    <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/>
    <path d="M14 3v6h6"/>
    <path d="M8 14l1.5 3 1.5-3M13 14v3M14 14h-1v3h1"/>
  </Icon>
);
const IconLanShare = (p) => (
  <Icon {...p}>
    <path d="M9 7a3 3 0 1 1 6 0v3H9V7z"/>
    <rect x="4" y="14" width="16" height="6" rx="1.5"/>
    <line x1="9" y1="10" x2="9" y2="14"/>
    <line x1="15" y1="10" x2="15" y2="14"/>
    <line x1="12" y1="10" x2="12" y2="14"/>
  </Icon>
);
const IconDatabase = (p) => (
  <Icon {...p}>
    <ellipse cx="12" cy="5" rx="8" ry="3"/>
    <path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5"/>
    <path d="M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6"/>
  </Icon>
);
const IconSparkle = (p) => (
  <Icon {...p}>
    <path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1"/>
  </Icon>
);
const IconShield = (p) => (
  <Icon {...p}>
    <path d="M12 2 4 6v6c0 5 3.5 9 8 10 4.5-1 8-5 8-10V6l-8-4z"/>
  </Icon>
);

/* ------------- Suggestion list (used by several variants) ------------- */
const LUDUSAVI_DB_SAMPLE = [
  { name: "Legion", coverage: 12 },
  { name: "Legion TD 2", coverage: 4 },
  { name: "Lego Batman 3: Beyond Gotham", coverage: 28 },
  { name: "Lego Batman: Legacy of the Dark Knight", coverage: 41, best: true },
  { name: "Lego Batman: The Videogame", coverage: 18 },
  { name: "Lego Builder's Journey", coverage: 6 },
  { name: "Lego City Undercover", coverage: 22 },
  { name: "Lego Indiana Jones", coverage: 14 },
  { name: "Lego Island", coverage: 9 },
];

/* ------------- Highlight matched substring in a name ------------- */
function HiName({ name, query }) {
  if (!query) return <>{name}</>;
  const q = query.toLowerCase();
  const idx = name.toLowerCase().indexOf(q);
  if (idx < 0) return <>{name}</>;
  return (
    <>
      {name.slice(0, idx)}
      <span style={{ color: "#fff", fontWeight: 600, background: "rgba(76,194,255,0.18)" }}>
        {name.slice(idx, idx + q.length)}
      </span>
      {name.slice(idx + q.length)}
    </>
  );
}

Object.assign(window, {
  DialogFrame, DialogTitleBar, DialogFooter, FieldLabel,
  IconArmoury, IconSteam, IconExe, IconLanShare, IconDatabase, IconSparkle, IconShield,
  LUDUSAVI_DB_SAMPLE, HiName,
  ACCENT,
});
