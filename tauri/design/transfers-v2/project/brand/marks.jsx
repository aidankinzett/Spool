/* Five candidate brand marks, each as a parametric SVG.
   Each component accepts:
     size      — outer size in px
     fg        — mark color (the geometry)
     bg        — tile background ("none" or color)
     radius    — tile corner radius (px). Auto-scaled if 0.
   The geometry sits on a 64×64 grid so each mark scales identically. */

const M = {
  /* ── HOLD ── vault-door monogram.
       A keep facade: tall rounded body, a horizontal seam two-thirds down
       suggesting a closed gate, and a small save-dot inside the lower band. */
  Hold: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
    const r = radius || size * 0.22;
    return (
      <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
        {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} ry={r} fill={bg} />}
        {/* outer keep silhouette */}
        <rect x="14" y="10" width="36" height="44" rx="10" ry="10"
              fill="none" stroke={fg} strokeWidth="3.5" />
        {/* gate seam */}
        <line x1="18" y1="38" x2="46" y2="38" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
        {/* save dot */}
        <circle cx="40" cy="46" r="2" fill={fg} />
      </svg>
    );
  },

  /* ── TROVE ── chest with a thin lid arc + a keyhole dot. */
  Trove: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
    const r = radius || size * 0.22;
    return (
      <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
        {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} ry={r} fill={bg} />}
        {/* chest body */}
        <rect x="12" y="22" width="40" height="30" rx="6" ry="6"
              fill="none" stroke={fg} strokeWidth="3.5" />
        {/* lid arc (a thin half-circle that sits on top) */}
        <path d="M 12,28 A 20,18 0 0 1 52,28"
              fill="none" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
        {/* keyhole / save indicator */}
        <circle cx="32" cy="37" r="2.2" fill={fg} />
        <line x1="32" y1="38" x2="32" y2="42" stroke={fg} strokeWidth="3" strokeLinecap="round" />
      </svg>
    );
  },

  /* ── SPOOL ── outer ring + small filled hub + a tape band passing through.
       Reads as a tape reel or two-device link depending on context. */
  Spool: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
    const r = radius || size * 0.22;
    return (
      <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
        {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} ry={r} fill={bg} />}
        {/* tape band that passes through behind */}
        <line x1="6" y1="32" x2="58" y2="32" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
        {/* outer reel */}
        <circle cx="32" cy="32" r="14" fill="none" stroke={fg} strokeWidth="3.5" />
        {/* hub */}
        <circle cx="32" cy="32" r="3.6" fill={fg} />
      </svg>
    );
  },

  /* ── BEACON ── a base block with two arcs broadcasting upward. */
  Beacon: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
    const r = radius || size * 0.22;
    return (
      <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
        {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} ry={r} fill={bg} />}
        {/* base device */}
        <rect x="26" y="42" width="12" height="12" rx="2.5" ry="2.5" fill={fg} />
        {/* inner broadcast arc */}
        <path d="M 22,40 A 12,12 0 0 1 42,40"
              fill="none" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
        {/* outer broadcast arc */}
        <path d="M 14,38 A 22,22 0 0 1 50,38"
              fill="none" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
      </svg>
    );
  },

  /* ── CAIRN ── three rounded blocks stacked, smallest on top. */
  Cairn: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
    const r = radius || size * 0.22;
    return (
      <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
        {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} ry={r} fill={bg} />}
        {/* bottom */}
        <rect x="10" y="42" width="44" height="10" rx="3" ry="3" fill={fg} />
        {/* middle */}
        <rect x="17" y="28" width="30" height="10" rx="3" ry="3" fill={fg} />
        {/* top */}
        <rect x="24" y="14" width="16" height="10" rx="3" ry="3" fill={fg} />
      </svg>
    );
  },
};

/* Wordmark — single source of truth for type lockups. */
function Wordmark({ name, fg = "#18181b", size = 56, weight = 700, tracking = "-0.03em" }) {
  return (
    <span style={{
      fontFamily: 'var(--font-display)',
      fontWeight: weight,
      fontSize: size,
      letterSpacing: tracking,
      color: fg,
      lineHeight: 1,
      display: "inline-block",
    }}>
      {name}
    </span>
  );
}

/* The brand directions themselves. Tagline + voice + a note on what makes it
   unique relative to the others. */
const DIRECTIONS = [
  {
    id: "hold",
    name: "Hold",
    Mark: M.Hold,
    tagline: "A hold for every save.",
    voice: [
      "I'll hold this for you.",
      "Restored from the hold.",
      "Add to your hold.",
    ],
    angle: "Preservation-forward. Quiet, utilitarian, vault-like. Dual meaning: \u201cstronghold\u201d + \u201chold this for me.\u201d",
    sample: "in the hold",
  },
  {
    id: "trove",
    name: "Trove",
    Mark: M.Trove,
    tagline: "A treasure trove of every game you own.",
    voice: [
      "Added to your trove.",
      "12 games in the trove.",
      "Open trove.",
    ],
    angle: "Collector-forward. Personal, warm, slightly playful. Leans into the cover-art-wall feeling.",
    sample: "in your trove",
  },
  {
    id: "spool",
    name: "Spool",
    Mark: M.Spool,
    tagline: "Spool saves across every device.",
    voice: [
      "Spooled to handheld.",
      "Save spooled in 1.2s.",
      "Three devices on the line.",
    ],
    angle: "Cross-device + save-state-forward. Slightly nerdy, evokes tape archives & sync. Strong technical identity.",
    sample: "spooled",
  },
  {
    id: "beacon",
    name: "Beacon",
    Mark: M.Beacon,
    tagline: "The beacon on your local network.",
    voice: [
      "Two beacons nearby.",
      "Broadcasting on :47632.",
      "Beacon picked up Aurora.",
    ],
    angle: "Network/LAN-forward. About discovery and presence. Strongest if peer-to-peer is the headline feature.",
    sample: "on the beacon",
  },
  {
    id: "cairn",
    name: "Cairn",
    Mark: M.Cairn,
    tagline: "Mark every save. Carry every game.",
    voice: [
      "Cairn restored.",
      "Stacked on this device.",
      "11 games in the cairn.",
    ],
    angle: "Quietly distinctive, abstract, archival. Stacked blocks naturally read as a library shelf at any size.",
    sample: "in the cairn",
  },
];

/* ─────────────────────────────────────────────────────────────
   SPOOL — alternate mark studies.
   Each variant lives on the same 64×64 grid as M.* above so
   they're directly swappable in any size context.
   ───────────────────────────────────────────────────────────── */
const SPOOL_VARIANTS = [
  {
    id: "roundel",
    name: "Roundel",
    note: "Original \u2014 outer ring, centre hub, tape passing through. Underground-flavoured.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <line x1="6" y1="32" x2="58" y2="32" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
          <circle cx="32" cy="32" r="14" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="32" cy="32" r="3.6" fill={fg} />
        </svg>
      );
    },
  },
  {
    id: "reel2reel",
    name: "Reel-to-reel",
    note: "Two reels linked by a tape \u2014 reads as cross-device sync, the most literal.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <line x1="20" y1="32" x2="44" y2="32" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
          <circle cx="20" cy="32" r="10" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="44" cy="32" r="10" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="20" cy="32" r="2.6" fill={fg} />
          <circle cx="44" cy="32" r="2.6" fill={fg} />
        </svg>
      );
    },
  },
  {
    id: "cassette",
    name: "Cassette",
    note: "Cassette-window silhouette \u2014 nostalgic save-tape, plays the retro angle.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <rect x="10" y="18" width="44" height="28" rx="5" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="22" cy="32" r="4" fill={fg} />
          <circle cx="42" cy="32" r="4" fill={fg} />
        </svg>
      );
    },
  },
  {
    id: "concentric",
    name: "Concentric",
    note: "Top-down reel \u2014 wound tape, no horizontal cut. Most archive-y.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <circle cx="32" cy="32" r="20" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="32" cy="32" r="13" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="32" cy="32" r="6" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="32" cy="32" r="2.2" fill={fg} />
        </svg>
      );
    },
  },
  {
    id: "unspool",
    name: "Unspooling",
    note: "Reel with a tape tail \u2014 motion, transfer in progress.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <circle cx="26" cy="32" r="14" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="26" cy="32" r="3.6" fill={fg} />
          <path d="M 40,32 Q 50,32 54,46" fill="none" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
        </svg>
      );
    },
  },
  {
    id: "esstape",
    name: "Tape-S",
    note: "Lowercase \u2018s\u2019 drawn as a single ribbon of tape \u2014 monogram + medium.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <path
            d="M 46,20 A 10,10 0 0 0 36,12 L 28,12 A 8,8 0 0 0 28,28 L 36,28 A 8,8 0 0 1 36,44 L 28,44 A 10,10 0 0 1 18,36"
            fill="none" stroke={fg} strokeWidth="5" strokeLinecap="round" strokeLinejoin="round"
          />
        </svg>
      );
    },
  },
  {
    id: "bobbin",
    name: "Bobbin",
    note: "Side-on bobbin \u2014 thread wound between two pegs. Hand-crafted feel.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          {/* two end caps */}
          <line x1="14" y1="14" x2="14" y2="50" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
          <line x1="50" y1="14" x2="50" y2="50" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
          {/* wound stripes */}
          <line x1="18" y1="24" x2="46" y2="24" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
          <line x1="18" y1="32" x2="46" y2="32" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
          <line x1="18" y1="40" x2="46" y2="40" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
        </svg>
      );
    },
  },
  {
    id: "beads",
    name: "Beads",
    note: "Three dots on a line \u2014 minimal abstraction of \u2018saves on a wire\u2019.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64" style={{ display: "block" }}>
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <line x1="8" y1="32" x2="56" y2="32" stroke={fg} strokeWidth="3" strokeLinecap="round" />
          <circle cx="16" cy="32" r="5" fill={fg} />
          <circle cx="32" cy="32" r="7" fill={fg} />
          <circle cx="48" cy="32" r="5" fill={fg} />
        </svg>
      );
    },
  },
];

Object.assign(window, { M, Wordmark, DIRECTIONS, SPOOL_VARIANTS });

/* ─────────────────────────────────────────────────────────────
   Reel-to-reel & Cassette — refinements.
   The original R2R routes tape through the reel centres, which
   isn't how real decks work; the iterations below fix that.
   ───────────────────────────────────────────────────────────── */
const REEL_REFINEMENTS = [
  {
    id: "underslung",
    name: "Underslung",
    note: "Tape exits the bottom of each reel and runs along a lower path \u2014 anatomically right.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64">
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          {/* reels */}
          <circle cx="20" cy="26" r="10" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="44" cy="26" r="10" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="20" cy="26" r="2.6" fill={fg} />
          <circle cx="44" cy="26" r="2.6" fill={fg} />
          {/* tape path: down from each reel, across the bottom */}
          <path d="M 20,36 V 48 H 44 V 36"
                fill="none" stroke={fg} strokeWidth="3.5"
                strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      );
    },
  },
  {
    id: "underslung-head",
    name: "With head",
    note: "Underslung path + a tiny read-head pickup at centre. More machinery.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64">
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <circle cx="20" cy="24" r="9.5" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="44" cy="24" r="9.5" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="20" cy="24" r="2.6" fill={fg} />
          <circle cx="44" cy="24" r="2.6" fill={fg} />
          {/* tape down and across */}
          <path d="M 20,33.5 V 47 H 44 V 33.5"
                fill="none" stroke={fg} strokeWidth="3.5"
                strokeLinecap="round" strokeLinejoin="round" />
          {/* read head */}
          <rect x="29" y="44" width="6" height="6" rx="1" fill={fg} />
        </svg>
      );
    },
  },
  {
    id: "sagging",
    name: "Sagging",
    note: "Single curve drooping between the two reels \u2014 looser, more organic.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64">
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <circle cx="20" cy="24" r="10" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="44" cy="24" r="10" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="20" cy="24" r="2.6" fill={fg} />
          <circle cx="44" cy="24" r="2.6" fill={fg} />
          <path d="M 20,34 Q 32,52 44,34"
                fill="none" stroke={fg} strokeWidth="3.5" strokeLinecap="round" />
        </svg>
      );
    },
  },
  {
    id: "asymmetric",
    name: "Asymmetric",
    note: "Left reel full (filled), right reel empty (outline) \u2014 implies direction of transfer.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64">
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          {/* full reel */}
          <circle cx="20" cy="24" r="10" fill={fg} />
          <circle cx="20" cy="24" r="2.6" fill={bg !== "none" ? bg : "#ffffff"} />
          {/* empty reel */}
          <circle cx="44" cy="24" r="10" fill="none" stroke={fg} strokeWidth="3.5" />
          <circle cx="44" cy="24" r="2.6" fill={fg} />
          <path d="M 20,34 V 47 H 44 V 34"
                fill="none" stroke={fg} strokeWidth="3.5"
                strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      );
    },
  },
  {
    id: "cassette-tape",
    name: "Cassette · tape",
    note: "Cassette shell with two visible hubs + a thin tape strip exposed at the bottom.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64">
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          {/* shell */}
          <rect x="8" y="16" width="48" height="32" rx="4" fill="none" stroke={fg} strokeWidth="3.5" />
          {/* hubs */}
          <circle cx="22" cy="28" r="4" fill={fg} />
          <circle cx="42" cy="28" r="4" fill={fg} />
          {/* exposed tape strip across the bottom */}
          <line x1="14" y1="42" x2="50" y2="42" stroke={fg} strokeWidth="3" strokeLinecap="round" />
        </svg>
      );
    },
  },
  {
    id: "cassette-band",
    name: "Cassette · band",
    note: "Cassette with a top label band \u2014 reads as a tagged save / labelled tape.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64">
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <rect x="8" y="14" width="48" height="36" rx="4" fill="none" stroke={fg} strokeWidth="3.5" />
          {/* label band */}
          <rect x="12" y="18" width="40" height="6" rx="2" fill={fg} />
          {/* hubs */}
          <circle cx="22" cy="36" r="5" fill={fg} />
          <circle cx="42" cy="36" r="5" fill={fg} />
        </svg>
      );
    },
  },
  {
    id: "cassette-spokes",
    name: "Cassette · spokes",
    note: "Cassette with spoked reels visible \u2014 most detailed, most retro.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64">
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <rect x="8" y="16" width="48" height="32" rx="4" fill="none" stroke={fg} strokeWidth="3.5" />
          {/* left spoked reel */}
          <circle cx="22" cy="32" r="6" fill="none" stroke={fg} strokeWidth="2.5" />
          <line x1="22" y1="26" x2="22" y2="38" stroke={fg} strokeWidth="2.5" strokeLinecap="round" />
          <line x1="16" y1="32" x2="28" y2="32" stroke={fg} strokeWidth="2.5" strokeLinecap="round" />
          {/* right spoked reel */}
          <circle cx="42" cy="32" r="6" fill="none" stroke={fg} strokeWidth="2.5" />
          <line x1="42" y1="26" x2="42" y2="38" stroke={fg} strokeWidth="2.5" strokeLinecap="round" />
          <line x1="36" y1="32" x2="48" y2="32" stroke={fg} strokeWidth="2.5" strokeLinecap="round" />
        </svg>
      );
    },
  },
  {
    id: "cassette-tilt",
    name: "Cassette · tilt",
    note: "Slightly rotated cassette \u2014 dynamic, characterful. Lose some clarity at 16 px.",
    Mark: ({ size = 64, fg = "#18181b", bg = "none", radius = 0 }) => {
      const r = radius || size * 0.22;
      return (
        <svg width={size} height={size} viewBox="0 0 64 64">
          {bg !== "none" && <rect x="0" y="0" width="64" height="64" rx={r} fill={bg} />}
          <g transform="rotate(-8 32 32)">
            <rect x="8" y="18" width="48" height="28" rx="4" fill="none" stroke={fg} strokeWidth="3.5" />
            <circle cx="22" cy="32" r="4" fill={fg} />
            <circle cx="42" cy="32" r="4" fill={fg} />
            <line x1="14" y1="40" x2="50" y2="40" stroke={fg} strokeWidth="3" strokeLinecap="round" />
          </g>
        </svg>
      );
    },
  },
];

/* The two originals, kept handy so the refinement card can show them next to the new ideas. */
const REEL_ORIGINALS = SPOOL_VARIANTS.filter((v) => v.id === "reel2reel" || v.id === "cassette");

Object.assign(window, { REEL_REFINEMENTS, REEL_ORIGINALS });
