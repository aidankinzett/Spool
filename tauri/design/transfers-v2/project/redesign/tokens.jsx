/* Spool · cassette-forward design tokens (v2)
   Cross-platform dark-only. Built on three type stacks:
     display — Space Grotesk     (titles, the brand voice)
     ui      — Geist              (everything readable)
     mono    — JetBrains Mono     (catalog numbers, paths, timestamps)
   Cover-art accent is per-game — the chrome stays neutral graphite. */

window.TOK = {
  font: {
    display: `"Space Grotesk", system-ui, sans-serif`,
    ui: `"Geist", system-ui, sans-serif`,
    mono: `"JetBrains Mono", ui-monospace, monospace`,
  },

  /* Graphite palette — quiet, slightly warm at the deep end. */
  c: {
    bg0: "#0b0c0e",       // window outside / behind chrome
    bg1: "#101216",       // pane background
    bg2: "#15181d",       // raised surface (cards)
    bg3: "#1c2027",       // hover / selection ground
    line: "rgba(255,255,255,0.06)",
    line2: "rgba(255,255,255,0.10)",
    line3: "rgba(255,255,255,0.16)",

    ink0: "#f4f4f5",      // primary text
    ink1: "rgba(244,244,245,0.78)",
    ink2: "rgba(244,244,245,0.56)",
    ink3: "rgba(244,244,245,0.36)",

    /* Status — meter colors */
    ok:   "#7ee2a4",
    warn: "#f4b66c",
    info: "#7ec6ff",
    bad:  "#ff7a7a",

    /* Brand defaults (when no cover art) */
    spool: "#d7c9a0",     // tape oxide
    spoolDeep: "#1a1612", // shell
  },

  /* Radii kept small — utility/cassette feel, not consumer-app squish */
  r: { sm: 3, md: 5, lg: 8, pill: 999 },

  /* Density — desktop is the default; touch swaps for Deck */
  d: {
    desktop: {
      titleBar: 32,
      sidebar: 320,
      rowH: 56,
      btnH: 30,
      btnHsm: 24,
      iconBtn: 36,
      cardPad: 16,
      sectionGap: 14,
      gutter: 28,
      base: 13, sm: 11, xs: 10,
      h1: 28, h2: 18, h3: 14,
    },
    touch: {
      titleBar: 48,
      sidebar: 380,
      rowH: 80,
      btnH: 44,
      btnHsm: 36,
      iconBtn: 52,
      cardPad: 22,
      sectionGap: 20,
      gutter: 28,
      base: 15, sm: 13, xs: 12,
      h1: 36, h2: 22, h3: 17,
    },
  },
};

/* ─────────────────────────── Spool brand mark ─────────────────────────── */
/* Cassette glyph — two reels + tape window, geometric.
   Renders identical on Win/Linux/macOS — no OS-specific accent. */
function SpoolMark({ size = 22, color = "currentColor", tape, dim }) {
  const tapeColor = tape || color;
  return (
    <svg
      width={size}
      height={size * (16 / 22)}
      viewBox="0 0 22 16"
      fill="none"
      style={{ display: "block", flexShrink: 0 }}
    >
      {/* shell */}
      <rect
        x="0.75"
        y="0.75"
        width="20.5"
        height="14.5"
        rx="1.4"
        stroke={color}
        strokeWidth="1.5"
        fill="none"
        opacity={dim ? 0.45 : 1}
      />
      {/* reels */}
      <circle cx="6.5" cy="8" r="2.4" stroke={color} strokeWidth="1.4" fill="none" />
      <circle cx="6.5" cy="8" r="0.7" fill={color} />
      <circle cx="15.5" cy="8" r="2.4" stroke={color} strokeWidth="1.4" fill="none" />
      <circle cx="15.5" cy="8" r="0.7" fill={color} />
      {/* tape strip across bottom */}
      <rect x="3" y="12.5" width="16" height="1.4" rx="0.4" fill={tapeColor} opacity="0.85" />
    </svg>
  );
}

/* ─────────────────────────── Window chrome ─────────────────────────── */
/* Custom title bar — same on Win/Linux/macOS. Drag area + Spool mark left,
   centered title, three controls right (minimize / max / close).
   Uses neutral glyphs, no Windows-specific symbols. */
function WindowChrome({ title, sub, accent, children, height, frameless }) {
  const h = height || TOK.d.desktop.titleBar;
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 12,
      height: h, padding: "0 10px 0 14px",
      background: frameless ? "transparent" : "rgba(0,0,0,0.25)",
      borderBottom: frameless ? "none" : `1px solid ${TOK.c.line}`,
      flexShrink: 0,
      userSelect: "none",
    }}>
      <SpoolMark size={18} color={TOK.c.ink1} tape={accent || TOK.c.spool} />
      <div style={{
        fontFamily: TOK.font.mono,
        fontSize: 10.5,
        letterSpacing: "0.16em",
        textTransform: "uppercase",
        color: TOK.c.ink2,
      }}>SPOOL</div>
      {sub && (
        <>
          <span style={{ color: TOK.c.ink3, fontSize: 10 }}>/</span>
          <div style={{
            fontFamily: TOK.font.mono,
            fontSize: 10.5,
            letterSpacing: "0.12em",
            textTransform: "uppercase",
            color: TOK.c.ink2,
          }}>{sub}</div>
        </>
      )}
      <div style={{ flex: 1 }}>{children}</div>
      <ChromeBtn glyph="min" />
      <ChromeBtn glyph="max" />
      <ChromeBtn glyph="close" />
    </div>
  );
}

function ChromeBtn({ glyph }) {
  const danger = glyph === "close";
  return (
    <button style={{
      width: 28, height: 22,
      background: "transparent",
      border: "none",
      borderRadius: TOK.r.sm,
      color: TOK.c.ink2,
      cursor: "pointer",
      display: "inline-flex", alignItems: "center", justifyContent: "center",
    }}
    onMouseEnter={(e) => {
      e.currentTarget.style.background = danger ? "rgba(255,90,90,0.18)" : "rgba(255,255,255,0.06)";
      e.currentTarget.style.color = danger ? "#ff9b9b" : TOK.c.ink0;
    }}
    onMouseLeave={(e) => {
      e.currentTarget.style.background = "transparent";
      e.currentTarget.style.color = TOK.c.ink2;
    }}>
      <svg width="10" height="10" viewBox="0 0 10 10">
        {glyph === "min" && <rect x="1" y="5" width="8" height="1" fill="currentColor" />}
        {glyph === "max" && <rect x="1.5" y="1.5" width="7" height="7" stroke="currentColor" fill="none" />}
        {glyph === "close" && (
          <>
            <line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" strokeWidth="1.1" />
            <line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" strokeWidth="1.1" />
          </>
        )}
      </svg>
    </button>
  );
}

/* ─────────────────────────── Voice / labels ─────────────────────────── */
function MonoLabel({ children, color, size = 10, ...rest }) {
  return (
    <span style={{
      fontFamily: TOK.font.mono,
      fontSize: size,
      letterSpacing: "0.12em",
      textTransform: "uppercase",
      color: color || TOK.c.ink2,
      ...rest.style,
    }} {...rest}>{children}</span>
  );
}

function CatalogId({ id, accent }) {
  return (
    <span style={{
      fontFamily: TOK.font.mono,
      fontSize: 10,
      letterSpacing: "0.1em",
      color: accent || TOK.c.ink2,
      padding: "2px 6px",
      border: `1px solid ${accent ? accent + "55" : TOK.c.line2}`,
      borderRadius: TOK.r.sm,
      whiteSpace: "nowrap",
    }}>{id}</span>
  );
}

function Pill({ kind = "info", children, soft }) {
  const palette = {
    ok:   { bg: "rgba(126,226,164,0.10)", fg: "#a5edc1", dot: TOK.c.ok },
    warn: { bg: "rgba(244,182,108,0.10)", fg: "#f6cf94", dot: TOK.c.warn },
    info: { bg: "rgba(126,198,255,0.10)", fg: "#a5d5ff", dot: TOK.c.info },
    bad:  { bg: "rgba(255,122,122,0.10)", fg: "#ffa6a6", dot: TOK.c.bad },
    off:  { bg: "rgba(255,255,255,0.04)", fg: TOK.c.ink2, dot: TOK.c.ink3 },
  }[kind] || {};
  return (
    <span style={{
      display: "inline-flex", alignItems: "center", gap: 6,
      padding: "2px 7px",
      height: 18,
      background: soft ? "transparent" : palette.bg,
      border: soft ? `1px solid ${palette.dot}44` : "none",
      color: palette.fg,
      borderRadius: TOK.r.sm,
      fontFamily: TOK.font.mono,
      fontSize: 9.5,
      letterSpacing: "0.1em",
      textTransform: "uppercase",
      lineHeight: 1,
      whiteSpace: "nowrap",
    }}>
      <span style={{ width: 5, height: 5, borderRadius: 99, background: palette.dot }} />
      {children}
    </span>
  );
}

/* ─────────────────────────── Buttons ─────────────────────────── */
function Btn({ children, variant = "ghost", accent, icon, onClick, danger, full, style }) {
  const [hover, setHover] = React.useState(false);
  const acc = accent || TOK.c.spool;

  let bg, fg, border;
  if (variant === "primary") {
    bg = hover ? shadeHex(acc, -10) : acc;
    fg = "#0b0c0e"; border = "transparent";
  } else if (variant === "secondary") {
    bg = hover ? TOK.c.bg3 : TOK.c.bg2;
    fg = TOK.c.ink0; border = TOK.c.line2;
  } else if (danger) {
    bg = hover ? "rgba(255,122,122,0.18)" : "transparent";
    fg = hover ? "#ffa6a6" : TOK.c.ink1; border = TOK.c.line;
  } else {
    bg = hover ? "rgba(255,255,255,0.06)" : "transparent";
    fg = TOK.c.ink0; border = TOK.c.line;
  }

  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: "inline-flex", alignItems: "center", gap: 7,
        padding: "0 12px",
        height: 28,
        width: full ? "100%" : undefined,
        background: bg,
        color: fg,
        border: `1px solid ${border}`,
        borderRadius: TOK.r.sm,
        fontFamily: TOK.font.ui,
        fontSize: 12.5,
        fontWeight: 500,
        cursor: "pointer",
        whiteSpace: "nowrap",
        transition: "background 100ms ease",
        ...style,
      }}
    >
      {icon}
      {children}
    </button>
  );
}

/* ─────────────────────────── Icons (line, cross-platform) ─────────────────────────── */
function I({ d, size = 14, stroke = 1.5, fill }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth={stroke}
      strokeLinecap="round" strokeLinejoin="round" style={{ display: "block", flexShrink: 0 }}>
      {fill ? <path d={d} fill="currentColor" stroke="none" /> : <path d={d} />}
    </svg>
  );
}
const ICN = {
  play:    <I fill d="M4 3.2v9.6L13 8z" />,
  search:  <I d="M7 12.5a5.5 5.5 0 1 0 0-11 5.5 5.5 0 0 0 0 11Zm4-1.5 3 3" />,
  plus:    <I d="M8 3.5v9M3.5 8h9" />,
  folder:  <I d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.6l1.4 1.4h5A1.5 1.5 0 0 1 14 5.9V12a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12V4.5Z" />,
  cog:     <I d="M8 5.6a2.4 2.4 0 1 1 0 4.8 2.4 2.4 0 0 1 0-4.8Zm0-3.6.7 1.4 1.5-.3.6 1.4 1.5.3-.3 1.5 1.4.7-1.4.7.3 1.5-1.5.3-.6 1.4-1.5-.3-.7 1.4-.7-1.4-1.5.3-.6-1.4-1.5-.3.3-1.5L2 8l1.4-.7-.3-1.5 1.5-.3.6-1.4 1.5.3z" />,
  wifi:    <I d="M2 6.5a8 8 0 0 1 12 0M4.2 9a5 5 0 0 1 7.6 0M6.5 11.5a2 2 0 0 1 3 0M8 13.5h.01" />,
  download:<I d="M8 2.5v8.5m0 0L4.5 7.6M8 11l3.5-3.4M3 13.5h10" />,
  upload:  <I d="M8 13.5V5m0 0L4.5 8.4M8 5l3.5 3.4M3 2.5h10" />,
  cloud:   <I d="M4.5 11.5a3 3 0 0 1-.3-6 3.5 3.5 0 0 1 6.8-.6 2.8 2.8 0 0 1 .5 5.6Z" />,
  trash:   <I d="M2.5 4.5h11M6 4.5V3a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.5M4 4.5l.6 8a1 1 0 0 0 1 .9h4.8a1 1 0 0 0 1-.9l.6-8" />,
  pencil:  <I d="m3 13 1-3 7-7 2 2-7 7Z" />,
  copy:    <I d="M5 4.5h7.5v9H5zM4 12V3.5h7" />,
  external:<I d="M9.5 3h3.5v3.5M13 3 8 8M11 9v3.5H3.5V5H7" />,
  chev:    <I d="m4 6 4 4 4-4" />,
  chevR:   <I d="m6 4 4 4-4 4" />,
  check:   <I d="m3.5 8.5 3 3 6-7" />,
  close:   <I d="M3.5 3.5 12.5 12.5M12.5 3.5l-9 9" />,
  steam:   <I d="M8 2.5a5.5 5.5 0 0 0-5.5 5.5l3 1.2A2 2 0 0 1 8 8.2L10.4 6a2.4 2.4 0 1 1 2.4 2.4M5.5 11.5a1.5 1.5 0 1 0 1.5-1.5" />,
  signal:  <I d="M2 13.5v-2M5 13.5v-5M8 13.5v-8M11 13.5v-11" />,
  clock:   <I d="M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12ZM8 5v3l2 1.5" />,
  hdd:     <I d="M2.5 3.5h11v9h-11zM5 7h6M5 10h3" />,
  gamepad: <I d="M3 6.5h2.5M4.25 5.5v2M11 6.5h.01M9.5 8h.01M2.5 11l1-4a2 2 0 0 1 2-1.5h5a2 2 0 0 1 2 1.5l1 4a1.5 1.5 0 0 1-2.4 1.4l-1.6-1.4h-3l-1.6 1.4A1.5 1.5 0 0 1 2.5 11Z" />,
  shield:  <I d="M8 2 3 4v4c0 3 2.5 5 5 6 2.5-1 5-3 5-6V4Z" />,
  share:   <I d="M11 5a1.8 1.8 0 1 0 0-3.6A1.8 1.8 0 0 0 11 5Zm0 9.6A1.8 1.8 0 1 0 11 11a1.8 1.8 0 0 0 0 3.6ZM5 9.8a1.8 1.8 0 1 0 0-3.6 1.8 1.8 0 0 0 0 3.6Zm1.5-2.4 3 1.6m0-3.6-3 1.6" />,
  sparkle: <I d="M8 2v3m0 6v3M2 8h3m6 0h3M4 4l2 2m4 4 2 2M4 12l2-2m4-4 2-2" />,
  exe:     <I d="M3 2.5h7.5L13 5v8.5H3zM10.5 2.5V5H13M5 9l1.5 1.5L5 12M8 12h3" />,
  reel:    <I d="M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12ZM8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3ZM4 8h1.5M10.5 8H12M8 4v1.5M8 10.5V12" />,
  source:  <I d="M8 2 2 4.5 8 7l6-2.5L8 2ZM2 8l6 2.5L14 8M2 11.5 8 14l6-2.5" />,
  device:  <I d="M2.5 3.5h11v7h-11zM6 13h4M7 10.5v2.5M9 10.5v2.5" />,
  key:     <I d="M10.5 9a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Zm-1.7 1.3L5 14l-1.5-1.5L4.5 11.5 3 10l1.8-1.7" />,
  eye:     <I d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8s-2.5 4.5-6.5 4.5S1.5 8 1.5 8ZM8 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z" />,
  filter:  <I d="M2 3h12L9.5 8.5V13L6.5 11.5V8.5Z" />,
  grid:    <I d="M3 3h4v4H3zM9 3h4v4H9zM3 9h4v4H3zM9 9h4v4H9z" />,
  list:    <I d="M5.5 4h8M5.5 8h8M5.5 12h8M3 4h.01M3 8h.01M3 12h.01" />,
  controller: <I d="M2 10c0-2 1-4 3-4h6c2 0 3 2 3 4s-1 2-2 2-1.5-1-2-1H6c-.5 0-1 1-2 1s-2 0-2-2ZM5 8h1.5M6 7.5v1M10 8h.01M11 9h.01M11.5 7h.01" />,
};

function shadeHex(hex, percent) {
  let c = hex.replace("#", "");
  if (c.length === 3) c = c.split("").map(x => x + x).join("");
  const num = parseInt(c, 16);
  const shift = Math.round(255 * percent / 100);
  let r = Math.max(0, Math.min(255, (num >> 16) + shift));
  let g = Math.max(0, Math.min(255, ((num >> 8) & 0xff) + shift));
  let b = Math.max(0, Math.min(255, (num & 0xff) + shift));
  return "#" + ((r << 16) | (g << 8) | b).toString(16).padStart(6, "0");
}

/* ─────────────────────────── Cover-art placeholder ─────────────────────────── */
/* No real PNGs — we synthesize a poster from the game's color trio.
   Tape-reel halo over a duotone gradient = recognisable "Spool" cover. */
function Cover({ game, w = 120, h = 170, label, sleeve }) {
  const a = game.art;
  return (
    <div style={{
      width: w, height: h,
      position: "relative",
      borderRadius: TOK.r.sm,
      overflow: "hidden",
      background: `linear-gradient(160deg, ${a.from} 0%, ${a.to} 100%)`,
      flexShrink: 0,
      boxShadow: "0 1px 0 rgba(255,255,255,0.04) inset, 0 6px 20px rgba(0,0,0,0.45)",
    }}>
      {/* tape-reel halo */}
      <div style={{
        position: "absolute", left: "-22%", top: "8%",
        width: w * 1.1, height: w * 1.1, borderRadius: "50%",
        background: `radial-gradient(circle at 30% 30%, ${a.accent}55, transparent 60%)`,
        mixBlendMode: "screen",
      }} />
      <div style={{
        position: "absolute", right: "-30%", bottom: "-10%",
        width: w * 0.9, height: w * 0.9, borderRadius: "50%",
        border: `1px solid ${a.accent}33`,
        background: `radial-gradient(circle, ${a.accent}22, transparent 60%)`,
      }} />
      {/* grain */}
      <div style={{
        position: "absolute", inset: 0,
        backgroundImage: "radial-gradient(rgba(255,255,255,0.05) 1px, transparent 1px)",
        backgroundSize: "3px 3px",
        opacity: 0.5,
        mixBlendMode: "overlay",
      }} />
      {/* title */}
      <div style={{
        position: "absolute", left: 10, right: 10, bottom: 10,
      }}>
        <div style={{
          fontFamily: TOK.font.display,
          fontSize: Math.max(11, Math.round(w * 0.105)),
          fontWeight: 600,
          letterSpacing: "-0.01em",
          color: TOK.c.ink0,
          lineHeight: 1.08,
          textWrap: "balance",
          textShadow: "0 1px 8px rgba(0,0,0,0.5)",
        }}>{game.short || game.name}</div>
      </div>
      {/* sleeve label across top */}
      {sleeve !== false && (
        <div style={{
          position: "absolute", top: 0, left: 0, right: 0,
          height: 14,
          background: `linear-gradient(to bottom, ${a.accent}, ${a.accent}cc)`,
        }} />
      )}
      {label !== false && (
        <div style={{
          position: "absolute", top: 14, left: 10,
          fontFamily: TOK.font.mono,
          fontSize: 8.5, letterSpacing: "0.16em",
          color: "rgba(255,255,255,0.7)",
          textTransform: "uppercase",
          marginTop: 4,
        }}>{label || game.mood || "Side A"}</div>
      )}
    </div>
  );
}

Object.assign(window, {
  TOK, SpoolMark, WindowChrome, ChromeBtn,
  MonoLabel, CatalogId, Pill, Btn,
  ICN, I,
  Cover,
  shadeHex,
});
