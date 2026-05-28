/* Spool · Controller glyphs (Steam Deck / Xbox-style)
   Used in the Deck shelf hint bar and elsewhere we need to show controller bindings. */

function CtrlGlyph({ kind, size = 22 }) {
  const s = size;
  switch (kind) {
    /* ABXY — Xbox / Deck layout */
    case "A": return <CtrlBubble s={s} color="#7ee2a4" label="A" />;
    case "B": return <CtrlBubble s={s} color="#ff7a7a" label="B" />;
    case "X": return <CtrlBubble s={s} color="#7ec6ff" label="X" />;
    case "Y": return <CtrlBubble s={s} color="#f4d35e" label="Y" />;

    /* Shoulders & triggers */
    case "L1": case "LB":
      return <CtrlPill s={s} label="L1" />;
    case "R1": case "RB":
      return <CtrlPill s={s} label="R1" />;
    case "L2": case "LT":
      return <CtrlPill s={s} label="L2" trigger />;
    case "R2": case "RT":
      return <CtrlPill s={s} label="R2" trigger />;

    /* Menu / view */
    case "menu": case "start":
      return (
        <svg width={s * 1.3} height={s} viewBox="0 0 30 22" fill="none" style={{ display: "block" }}>
          <rect x="1" y="1" width="28" height="20" rx="10" stroke={TOK.c.ink1} strokeWidth="1.4" fill="rgba(255,255,255,0.04)" />
          <line x1="9" y1="11" x2="21" y2="11" stroke={TOK.c.ink0} strokeWidth="1.6" strokeLinecap="round" />
          <line x1="9" y1="7" x2="21" y2="7" stroke={TOK.c.ink0} strokeWidth="1.6" strokeLinecap="round" />
          <line x1="9" y1="15" x2="21" y2="15" stroke={TOK.c.ink0} strokeWidth="1.6" strokeLinecap="round" />
        </svg>
      );
    case "view":
      return (
        <svg width={s * 1.3} height={s} viewBox="0 0 30 22" fill="none" style={{ display: "block" }}>
          <rect x="1" y="1" width="28" height="20" rx="10" stroke={TOK.c.ink1} strokeWidth="1.4" fill="rgba(255,255,255,0.04)" />
          <rect x="9" y="7" width="12" height="8" rx="1" stroke={TOK.c.ink0} strokeWidth="1.4" fill="none" />
          <rect x="11.5" y="9.5" width="7" height="3" fill={TOK.c.ink0} />
        </svg>
      );

    /* Dpad */
    case "dpad-up":    return <Dpad s={s} dir="up" />;
    case "dpad-down":  return <Dpad s={s} dir="down" />;
    case "dpad-left":  return <Dpad s={s} dir="left" />;
    case "dpad-right": return <Dpad s={s} dir="right" />;
    case "dpad":       return <Dpad s={s} />;

    /* Sticks */
    case "lstick": return <Stick s={s} label="L" />;
    case "rstick": return <Stick s={s} label="R" />;

    /* Deck-specific back grips */
    case "L4": case "L5": case "R4": case "R5":
      return <CtrlPill s={s} label={kind} dim />;

    /* Steam button (cassette-style hamburger over circle) */
    case "steam":
      return (
        <svg width={s} height={s} viewBox="0 0 22 22" fill="none" style={{ display: "block" }}>
          <circle cx="11" cy="11" r="9" stroke={TOK.c.ink0} strokeWidth="1.4" fill="rgba(255,255,255,0.04)" />
          <line x1="6.5" y1="9" x2="15.5" y2="9" stroke={TOK.c.ink0} strokeWidth="1.4" strokeLinecap="round" />
          <line x1="6.5" y1="13" x2="15.5" y2="13" stroke={TOK.c.ink0} strokeWidth="1.4" strokeLinecap="round" />
        </svg>
      );
  }
  return null;
}

function CtrlBubble({ s, color, label }) {
  return (
    <span style={{
      width: s, height: s, borderRadius: s / 2,
      background: "rgba(255,255,255,0.04)",
      border: `1.5px solid ${color}`,
      display: "inline-flex", alignItems: "center", justifyContent: "center",
      fontFamily: TOK.font.display, fontWeight: 700,
      fontSize: s * 0.55, color,
      lineHeight: 1,
      flexShrink: 0,
    }}>{label}</span>
  );
}

function CtrlPill({ s, label, trigger, dim }) {
  return (
    <span style={{
      height: s, padding: `0 ${s * 0.32}px`,
      background: dim ? "transparent" : "rgba(255,255,255,0.04)",
      border: `1.4px solid ${dim ? TOK.c.line2 : TOK.c.ink1}`,
      borderRadius: trigger ? s * 0.3 : s * 0.5,
      display: "inline-flex", alignItems: "center", justifyContent: "center",
      fontFamily: TOK.font.mono, fontSize: s * 0.45, fontWeight: 600,
      color: dim ? TOK.c.ink2 : TOK.c.ink0, letterSpacing: "0.04em",
      flexShrink: 0,
    }}>{label}</span>
  );
}

function Dpad({ s, dir }) {
  return (
    <svg width={s} height={s} viewBox="0 0 22 22" fill="none" style={{ display: "block", flexShrink: 0 }}>
      <path
        d="M9 1.5h4v7h7v4h-7v7H9v-7H2v-4h7Z"
        fill="rgba(255,255,255,0.04)"
        stroke={TOK.c.ink1} strokeWidth="1.4" strokeLinejoin="round"
      />
      {dir && {
        up:    <polygon points="11,4.5 13,7 9,7" fill={TOK.c.ink0} />,
        down:  <polygon points="11,17.5 13,15 9,15" fill={TOK.c.ink0} />,
        left:  <polygon points="4.5,11 7,9 7,13" fill={TOK.c.ink0} />,
        right: <polygon points="17.5,11 15,9 15,13" fill={TOK.c.ink0} />,
      }[dir]}
    </svg>
  );
}

function Stick({ s, label }) {
  return (
    <svg width={s} height={s} viewBox="0 0 22 22" fill="none" style={{ display: "block", flexShrink: 0 }}>
      <circle cx="11" cy="11" r="9.5" stroke={TOK.c.ink1} strokeWidth="1.4" fill="rgba(255,255,255,0.04)" />
      <circle cx="11" cy="11" r="5" fill={TOK.c.bg0} stroke={TOK.c.ink0} strokeWidth="1.2" />
      <text x="11" y="13.5" textAnchor="middle" fontFamily={TOK.font.display} fontSize="6" fontWeight="700" fill={TOK.c.ink0}>{label}</text>
    </svg>
  );
}

/* Hint group — glyph + label, the unit of the Deck hint bar */
function CtrlHint({ glyph, label, size = 20 }) {
  return (
    <div style={{
      display: "inline-flex", alignItems: "center", gap: 7,
      color: TOK.c.ink1,
    }}>
      <CtrlGlyph kind={glyph} size={size} />
      <span style={{
        fontFamily: TOK.font.mono, fontSize: 10.5, letterSpacing: "0.1em",
        textTransform: "uppercase", color: TOK.c.ink1,
      }}>{label}</span>
    </div>
  );
}

Object.assign(window, { CtrlGlyph, CtrlHint });
