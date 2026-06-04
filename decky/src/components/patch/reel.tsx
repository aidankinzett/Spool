// Spool · save-state reel glyph + tape meter.
// The reel is the save-state indicator on the injected game-page bar; it spins
// while a backup is running. Geometry matches the cassette reels in SpoolMark.

const SPOOL_KF_ID = "spool-bar-keyframes";
if (typeof document !== "undefined" && !document.getElementById(SPOOL_KF_ID)) {
  const el = document.createElement("style");
  el.id = SPOOL_KF_ID;
  el.textContent =
    "@keyframes spool-reel-spin{to{transform:rotate(360deg)}}" +
    "@keyframes spool-tape-sheen{0%{transform:translateX(-130%)}55%,100%{transform:translateX(360%)}}";
  document.head.appendChild(el);
}

export function Reel({
  size = 14,
  color = "currentColor",
  spinning = false,
  dur = 2.2,
}: {
  size?: number;
  color?: string;
  spinning?: boolean;
  dur?: number;
}) {
  const sw = size * 0.09;
  const cx = size / 2;
  const r1 = size * 0.13;
  const r2 = size * 0.31;
  const spokes = [0, 1, 2, 3].map((k) => {
    const a = (k * Math.PI) / 2 + Math.PI / 4;
    return (
      <line
        key={k}
        x1={cx + Math.cos(a) * r1}
        y1={cx + Math.sin(a) * r1}
        x2={cx + Math.cos(a) * r2}
        y2={cx + Math.sin(a) * r2}
        stroke={color}
        strokeWidth={sw}
        strokeLinecap="round"
      />
    );
  });
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      style={{
        display: "block",
        flexShrink: 0,
        animation: spinning ? `spool-reel-spin ${dur}s linear infinite` : "none",
      }}
    >
      <circle cx={cx} cy={cx} r={size * 0.4} fill="none" stroke={color} strokeWidth={sw} />
      {spokes}
      <circle cx={cx} cy={cx} r={size * 0.12} fill={color} />
    </svg>
  );
}

// Cassette-tape progress meter — indeterminate sheen used while backing up.
export function TapeMeter({ accent, width = 70 }: { accent: string; width?: number }) {
  return (
    <span style={{ display: "inline-block", width, verticalAlign: "middle" }}>
      <span
        style={{
          position: "relative",
          display: "block",
          height: 3,
          borderRadius: 1,
          background: "rgba(0,0,0,0.4)",
          overflow: "hidden",
          boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.05)",
        }}
      >
        <span
          style={{
            position: "absolute",
            top: 0,
            bottom: 0,
            left: 0,
            width: "34%",
            background: `linear-gradient(90deg, transparent, ${accent}, transparent)`,
            animation: "spool-tape-sheen 1.5s ease-in-out infinite",
          }}
        />
      </span>
    </span>
  );
}
