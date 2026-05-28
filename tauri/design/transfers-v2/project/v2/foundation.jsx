/* ============================================================
   Spool v2 — foundation primitives.
   - WindowFrame / WindowChrome / TapeStrip
   - Bracket / Pill / Eyebrow / MonoNum
   - Button / IconButton / TextField / Toggle / Segmented
   All export to window for cross-file use.
============================================================ */

const { useState: useS, useRef: useR, useEffect: useE } = React;

/* ──────────────────────────────────────────────────────────
   WINDOW FRAME — the outer "cassette body".
   Same chrome on every OS. Spool mark left, peer pill center,
   plain min/max/close right. No traffic-light pretense.
   ────────────────────────────────────────────────────────── */
function WindowFrame({ children, width = 1280, height = 800, ambient }) {
  return (
    <div className="spool-v2" style={{
      width, height,
      position: "relative",
      background: "var(--c-bg)",
      borderRadius: 8,
      border: "1px solid var(--c-line-strong)",
      boxShadow: "var(--shadow-window)",
      overflow: "hidden",
      display: "flex",
      flexDirection: "column",
    }}>
      {/* ambient color tint (cover-art-driven) bleeds in from top */}
      {ambient && (
        <div style={{
          position: "absolute",
          inset: 0,
          background: `radial-gradient(900px 500px at 50% -10%, ${ambient}22, transparent 60%)`,
          pointerEvents: "none",
        }}/>
      )}
      {/* faint grain — subtle, never above 4% */}
      <div style={{
        position: "absolute",
        inset: 0,
        backgroundImage:
          "radial-gradient(circle at 1px 1px, rgba(241,236,228,0.025) 1px, transparent 0)",
        backgroundSize: "3px 3px",
        pointerEvents: "none",
      }}/>
      {children}
    </div>
  );
}

/* Cassette window chrome — 36px draggable header with custom controls.
   Same on Win/Mac/Linux to avoid OS impersonation. */
function WindowChrome({
  title = "SPOOL",
  catalog,            // optional mono catalog id displayed center-left
  peers,              // optional number; shows "[ N peers · LAN ]"
  rightExtras,        // extra React nodes before window controls
  onSettings,         // if provided, a gear button appears
  closeBehavior = "minimize", // visual hint only
}) {
  return (
    <div style={{
      height: 36,
      flexShrink: 0,
      display: "flex",
      alignItems: "stretch",
      borderBottom: "1px solid var(--c-line)",
      position: "relative",
      zIndex: 5,
      WebkitUserSelect: "none",
      userSelect: "none",
    }}>
      {/* mark + wordmark — strictly typographic */}
      <div style={{
        display: "flex", alignItems: "center", gap: 10,
        padding: "0 14px 0 16px",
      }}>
        <SpoolMark size={14} fg="var(--c-fg)" />
        <span style={{
          fontFamily: "var(--f-cond)",
          fontWeight: 700,
          fontSize: 13,
          letterSpacing: "0.10em",
          color: "var(--c-fg)",
        }}>{title}</span>
        {catalog && (
          <span className="mono" style={{
            fontSize: 10.5,
            color: "var(--c-fg-3)",
            letterSpacing: "0.08em",
            paddingLeft: 8,
            borderLeft: "1px solid var(--c-line)",
            marginLeft: 2,
          }}>{catalog}</span>
        )}
      </div>

      {/* center peer pill */}
      <div style={{
        flex: 1, display: "flex", alignItems: "center", justifyContent: "center",
      }}>
        {peers !== undefined && (
          <span className="mono" style={{
            fontSize: 10.5,
            color: "var(--c-fg-2)",
            letterSpacing: "0.06em",
            display: "inline-flex",
            alignItems: "center",
            gap: 8,
          }}>
            <span style={{
              width: 5, height: 5, borderRadius: 999,
              background: peers > 0 ? "var(--c-ok)" : "var(--c-fg-3)",
              boxShadow: peers > 0 ? "0 0 6px var(--c-ok)" : "none",
            }}/>
            [ {peers} {peers === 1 ? "PEER" : "PEERS"} · LAN ]
          </span>
        )}
      </div>

      {/* right side */}
      <div style={{ display: "flex", alignItems: "stretch" }}>
        {rightExtras}
        {onSettings && (
          <ChromeBtn onClick={onSettings} title="Settings">
            <IconV2Gear size={13} />
          </ChromeBtn>
        )}
        <ChromeBtn title="Minimize"><svg width="10" height="10"><line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" strokeWidth="1"/></svg></ChromeBtn>
        <ChromeBtn title="Maximize"><svg width="10" height="10"><rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1"/></svg></ChromeBtn>
        <ChromeBtn title="Close" danger>
          <svg width="10" height="10"><line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" strokeWidth="1"/><line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" strokeWidth="1"/></svg>
        </ChromeBtn>
      </div>
    </div>
  );
}

function ChromeBtn({ children, onClick, title, danger }) {
  const [h, setH] = useS(false);
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setH(true)}
      onMouseLeave={() => setH(false)}
      title={title}
      style={{
        width: 44,
        height: 36,
        background: h ? (danger ? "var(--c-err)" : "var(--c-surface-2)") : "transparent",
        color: h ? (danger ? "#fff" : "var(--c-fg)") : "var(--c-fg-1)",
        border: "none",
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >{children}</button>
  );
}

/* ──────────────────────────────────────────────────────────
   TAPE STRIP — the signature divider.
   A 1px hairline with the dashed "tape" texture above or below
   and optional mono labels at the ends.
   ────────────────────────────────────────────────────────── */
function TapeStrip({ left, right, top, bottom, color = "var(--c-fg-3)" }) {
  return (
    <div style={{
      display: "flex",
      alignItems: "center",
      gap: 10,
      paddingTop: top,
      paddingBottom: bottom,
    }}>
      {left && (
        <span className="mono" style={{
          fontSize: 10,
          letterSpacing: "0.14em",
          textTransform: "uppercase",
          color,
          flexShrink: 0,
        }}>{left}</span>
      )}
      <div style={{
        flex: 1,
        height: 4,
        background: "var(--tape-stripe)",
        opacity: 0.5,
      }}/>
      {right && (
        <span className="mono" style={{
          fontSize: 10,
          letterSpacing: "0.14em",
          textTransform: "uppercase",
          color,
          flexShrink: 0,
        }}>{right}</span>
      )}
    </div>
  );
}

/* Bracketed emphasis — wraps a label in [ ] brackets.
   Renders with a small gap so it reads as a frame, not text. */
function Bracket({ children, color = "var(--c-fg-2)", weight = 400 }) {
  return (
    <span style={{
      display: "inline-flex",
      alignItems: "center",
      gap: 6,
      color,
      fontWeight: weight,
    }}>
      <span style={{ opacity: 0.6 }}>[</span>
      <span>{children}</span>
      <span style={{ opacity: 0.6 }}>]</span>
    </span>
  );
}

/* Pill — used for genres, status, etc. Squared corners (cassette feel) */
function Pill({ children, tone = "neutral", icon }) {
  const tones = {
    neutral: { bg: "var(--c-surface-2)", border: "var(--c-line)", fg: "var(--c-fg-1)" },
    ok:      { bg: "var(--c-ok-dim)",    border: "transparent",   fg: "var(--c-ok)"   },
    warn:    { bg: "var(--c-warn-dim)",  border: "transparent",   fg: "var(--c-warn)" },
    err:     { bg: "var(--c-err-dim)",   border: "transparent",   fg: "var(--c-err)"  },
    oxide:   { bg: "var(--c-oxide-bg)",  border: "transparent",   fg: "var(--c-oxide)"},
  }[tone];
  return (
    <span style={{
      display: "inline-flex",
      alignItems: "center",
      gap: 5,
      padding: "2px 7px",
      borderRadius: 2,
      background: tones.bg,
      border: `1px solid ${tones.border}`,
      color: tones.fg,
      fontSize: 10.5,
      fontWeight: 500,
      letterSpacing: "0.03em",
      lineHeight: 1.4,
    }}>
      {icon && <span style={{ display: "flex" }}>{icon}</span>}
      {children}
    </span>
  );
}

/* ──────────────────────────────────────────────────────────
   BUTTON
   variants: primary, secondary, ghost, danger, oxide
   primary is *tinted by accent prop* (cover-art-driven on detail pages).
   ────────────────────────────────────────────────────────── */
function Button({
  children, variant = "secondary", size = "md",
  accent = "var(--c-fg)",
  icon, iconRight, onClick, disabled, fullWidth, style,
}) {
  const [h, setH] = useS(false);
  const [a, setA] = useS(false);

  let bg, fg, border;
  if (variant === "primary") {
    bg = h ? `color-mix(in oklch, ${accent}, white 8%)` : accent;
    fg = "#1a1612";
    border = "transparent";
  } else if (variant === "oxide") {
    bg = h ? "color-mix(in oklch, var(--c-oxide), white 8%)" : "var(--c-oxide)";
    fg = "#1a1612";
    border = "transparent";
  } else if (variant === "ghost") {
    bg = h ? "var(--c-surface-2)" : "transparent";
    fg = "var(--c-fg)";
    border = "transparent";
  } else if (variant === "danger") {
    bg = h ? "var(--c-err-dim)" : "transparent";
    fg = "var(--c-err)";
    border = h ? "transparent" : "var(--c-line)";
  } else {
    bg = h ? "var(--c-surface-2)" : "var(--c-surface-1)";
    fg = "var(--c-fg)";
    border = "var(--c-line-strong)";
  }

  const heights = { sm: 26, md: 32, lg: 40, xl: 56 };
  const paddings = { sm: "0 10px", md: "0 14px", lg: "0 18px", xl: "0 28px" };
  const fonts = { sm: 11.5, md: 12.5, lg: 13.5, xl: 15 };

  return (
    <button
      onClick={disabled ? undefined : onClick}
      onMouseEnter={() => setH(true)}
      onMouseLeave={() => { setH(false); setA(false); }}
      onMouseDown={() => setA(true)}
      onMouseUp={() => setA(false)}
      disabled={disabled}
      style={{
        height: heights[size],
        padding: paddings[size],
        background: bg,
        color: fg,
        border: `1px solid ${border}`,
        borderRadius: 2,
        fontFamily: "var(--f-sans)",
        fontSize: fonts[size],
        fontWeight: variant === "primary" || variant === "oxide" ? 600 : 500,
        letterSpacing: variant === "primary" ? "0.02em" : "0",
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.45 : 1,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        width: fullWidth ? "100%" : "auto",
        transform: a ? "translateY(1px)" : "translateY(0)",
        transition: "background 100ms ease, transform 50ms ease",
        ...style,
      }}
    >
      {icon && <span style={{ display: "flex" }}>{icon}</span>}
      <span>{children}</span>
      {iconRight && <span style={{ display: "flex" }}>{iconRight}</span>}
    </button>
  );
}

/* Square icon-only button */
function IconButton({ children, onClick, title, active, accent, size = 32, danger }) {
  const [h, setH] = useS(false);
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setH(true)}
      onMouseLeave={() => setH(false)}
      title={title}
      style={{
        width: size, height: size,
        background: active ? "var(--c-surface-3)" :
                    h ? "var(--c-surface-2)" : "transparent",
        color: danger && h ? "var(--c-err)" :
               active ? (accent || "var(--c-fg)") : "var(--c-fg-1)",
        border: "1px solid",
        borderColor: active ? "var(--c-line-strong)" : "transparent",
        borderRadius: 2,
        cursor: "pointer",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        transition: "background 100ms ease",
      }}
    >{children}</button>
  );
}

/* ──────────────────────────────────────────────────────────
   TEXT FIELD
   - thin underline focus (cassette deck input feel)
   - mono variant for paths
   ────────────────────────────────────────────────────────── */
function TextField({
  value, onChange, placeholder, prefix, suffix, monospace, accent = "var(--c-fg)",
  size = "md", style,
}) {
  const [f, setF] = useS(false);
  const h = { sm: 28, md: 32, lg: 40, xl: 52 }[size];
  const fs = { sm: 12, md: 13, lg: 14, xl: 16 }[size];
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 8,
      height: h,
      padding: "0 10px",
      background: "var(--c-surface-1)",
      border: "1px solid",
      borderColor: f ? accent : "var(--c-line-strong)",
      borderRadius: 2,
      ...style,
    }}>
      {prefix && <span style={{ color: "var(--c-fg-2)", display: "flex" }}>{prefix}</span>}
      <input
        type="text"
        value={value}
        onChange={(e) => onChange?.(e.target.value)}
        placeholder={placeholder}
        onFocus={() => setF(true)}
        onBlur={() => setF(false)}
        style={{
          flex: 1, minWidth: 0,
          background: "transparent",
          border: "none", outline: "none",
          color: "var(--c-fg)",
          fontFamily: monospace ? "var(--f-mono)" : "var(--f-sans)",
          fontSize: fs,
        }}
      />
      {suffix && <span style={{ color: "var(--c-fg-2)", display: "flex" }}>{suffix}</span>}
    </div>
  );
}

/* Segmented control — for Auto/On/Off, density, etc. */
function Segmented({ value, onChange, options, accent = "var(--c-oxide)" }) {
  return (
    <div style={{
      display: "inline-flex",
      background: "var(--c-surface-1)",
      border: "1px solid var(--c-line-strong)",
      borderRadius: 2,
      padding: 2,
    }}>
      {options.map(o => {
        const active = value === o.value;
        return (
          <button key={o.value}
            onClick={() => onChange(o.value)}
            style={{
              padding: "0 14px",
              height: 26,
              background: active ? accent : "transparent",
              color: active ? "#1a1612" : "var(--c-fg-1)",
              border: "none", borderRadius: 2,
              fontFamily: "var(--f-sans)",
              fontSize: 12,
              fontWeight: active ? 600 : 500,
              letterSpacing: active ? "0.02em" : 0,
              cursor: "pointer",
            }}>{o.label}</button>
        );
      })}
    </div>
  );
}

/* Toggle — minimal hard-edge switch */
function Toggle({ checked, onChange, accent = "var(--c-oxide)" }) {
  return (
    <button
      onClick={() => onChange(!checked)}
      style={{
        width: 36, height: 20,
        background: checked ? accent : "var(--c-surface-2)",
        border: `1px solid ${checked ? "transparent" : "var(--c-line-strong)"}`,
        borderRadius: 2,
        position: "relative",
        cursor: "pointer",
        padding: 0,
        transition: "background 120ms ease",
      }}
    >
      <span style={{
        position: "absolute",
        top: 2, left: checked ? 18 : 2,
        width: 14, height: 14,
        background: checked ? "#1a1612" : "var(--c-fg-1)",
        borderRadius: 1,
        transition: "left 120ms cubic-bezier(.2,.9,.3,1)",
      }}/>
    </button>
  );
}

/* ──────────────────────────────────────────────────────────
   LEVEL BAR — VU-meter style discrete blocks. Used for
   playtime, install size, save count visualization.
   ────────────────────────────────────────────────────────── */
function LevelBar({ value, max = 10, color = "var(--c-fg-1)", height = 8, gap = 2, blockWidth = 4 }) {
  const filled = Math.round((value / max) * 10);
  return (
    <div style={{ display: "inline-flex", gap, alignItems: "center" }}>
      {Array.from({ length: 10 }).map((_, i) => (
        <span key={i} style={{
          width: blockWidth, height,
          background: i < filled ? color : "var(--c-line-strong)",
          opacity: i < filled ? (i > 7 ? 0.9 : 1) : 1,
        }}/>
      ))}
    </div>
  );
}

/* small util for color-mixing accent with bg, used inline */
function tint(hex, alpha = 0.18) {
  // hex like "#e8a444"; return rgba string. alpha 0–1.
  if (!hex || hex[0] !== "#") return hex;
  const r = parseInt(hex.slice(1,3), 16);
  const g = parseInt(hex.slice(3,5), 16);
  const b = parseInt(hex.slice(5,7), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

Object.assign(window, {
  WindowFrame, WindowChrome, ChromeBtn,
  TapeStrip, Bracket, Pill, LevelBar,
  Button, IconButton, TextField, Segmented, Toggle,
  tint,
});
