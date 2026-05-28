/* Spool — official brand mark (cassette · tape variant).
   Cassette shell with two visible hubs and an exposed tape strip
   along the bottom edge. Scales cleanly from 16 px to a Steam Deck tile.

   Usage: <SpoolMark size={18} fg="#fff" />
   Tile:  <SpoolMark size={64} fg="#fff" bg="#161618" />  */

function SpoolMark({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) {
  const r = radius || size * 0.22;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      style={{ display: "block", flexShrink: 0 }}
      aria-hidden="true"
    >
      {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
      {/* shell */}
      <rect x="8" y="16" width="48" height="32" rx="4" fill="none" stroke={fg} strokeWidth="3.5" />
      {/* reel hubs */}
      <circle cx="22" cy="28" r="4" fill={fg} />
      <circle cx="42" cy="28" r="4" fill={fg} />
      {/* exposed tape strip */}
      <line x1="14" y1="42" x2="50" y2="42" stroke={fg} strokeWidth="3" strokeLinecap="round" />
    </svg>
  );
}

/* Wordmark — Spool, set in the display font family.
   Use alongside SpoolMark in titlebars / headers / about pages. */
function SpoolWordmark({ size = 14, fg = "rgba(255,255,255,0.92)", weight = 600, tracking = "-0.01em" }) {
  return (
    <span style={{
      fontFamily: 'var(--font-display, "Segoe UI Variable Display", "Segoe UI", "Inter", sans-serif)',
      fontWeight: weight,
      fontSize: size,
      letterSpacing: tracking,
      color: fg,
      lineHeight: 1,
    }}>Spool</span>
  );
}

Object.assign(window, { SpoolMark, SpoolWordmark });
