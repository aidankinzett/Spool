/* Reusable Windows 11 Fluent-styled controls.
   All controls accept an `accent` prop (CSS color) so the Tweaks panel
   can recolor toggles, buttons, focus rings in one go. */

const { useState, useRef, useEffect } = React;

/* ---------- Toggle Switch ---------- */
const ToggleSwitch = ({ checked, onChange, accent = "#4cc2ff", disabled }) => (
  <button
    role="switch"
    aria-checked={checked}
    disabled={disabled}
    onClick={() => !disabled && onChange(!checked)}
    style={{
      position: "relative",
      width: 40,
      height: 20,
      borderRadius: 10,
      border: checked
        ? `1px solid ${accent}`
        : "1px solid rgba(255,255,255,0.55)",
      background: checked ? accent : "transparent",
      cursor: disabled ? "default" : "pointer",
      opacity: disabled ? 0.4 : 1,
      padding: 0,
      transition: "background 120ms ease, border-color 120ms ease",
      flexShrink: 0,
    }}
  >
    <span
      style={{
        position: "absolute",
        top: "50%",
        left: checked ? 22 : 4,
        width: checked ? 12 : 10,
        height: checked ? 12 : 10,
        borderRadius: "50%",
        background: checked ? "#000" : "rgba(255,255,255,0.78)",
        transform: "translateY(-50%)",
        transition: "left 140ms cubic-bezier(.2,.9,.3,1.2), width 140ms ease, height 140ms ease, background 120ms ease",
      }}
    />
  </button>
);

/* ---------- Button ---------- */
const Button = ({
  children,
  variant = "secondary", // primary, secondary, ghost, danger
  size = "md", // sm, md
  accent = "#4cc2ff",
  icon,
  onClick,
  disabled,
  style,
  fullWidth,
}) => {
  const [hover, setHover] = useState(false);
  const [active, setActive] = useState(false);

  const palette = {
    primary: {
      bg: accent,
      bgHover: shade(accent, -8),
      bgActive: shade(accent, -16),
      fg: "#000",
      border: "transparent",
    },
    secondary: {
      bg: "rgba(255,255,255,0.06)",
      bgHover: "rgba(255,255,255,0.09)",
      bgActive: "rgba(255,255,255,0.04)",
      fg: "#fff",
      border: "rgba(255,255,255,0.09)",
    },
    ghost: {
      bg: "transparent",
      bgHover: "rgba(255,255,255,0.06)",
      bgActive: "rgba(255,255,255,0.03)",
      fg: "#fff",
      border: "transparent",
    },
    danger: {
      bg: "rgba(255,95,95,0.10)",
      bgHover: "rgba(255,95,95,0.18)",
      bgActive: "rgba(255,95,95,0.08)",
      fg: "#ff8a8a",
      border: "rgba(255,95,95,0.20)",
    },
  }[variant];

  const padding = size === "sm" ? "4px 10px" : "6px 14px";
  const fontSize = size === "sm" ? 12 : 13;
  const height = size === "sm" ? 26 : 32;

  return (
    <button
      onClick={disabled ? undefined : onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => { setHover(false); setActive(false); }}
      onMouseDown={() => setActive(true)}
      onMouseUp={() => setActive(false)}
      disabled={disabled}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        padding,
        height,
        minWidth: size === "sm" ? 0 : 70,
        width: fullWidth ? "100%" : undefined,
        background: active ? palette.bgActive : hover ? palette.bgHover : palette.bg,
        color: palette.fg,
        border: `1px solid ${palette.border}`,
        borderRadius: 4,
        fontFamily: "inherit",
        fontSize,
        fontWeight: 400,
        cursor: disabled ? "default" : "pointer",
        opacity: disabled ? 0.4 : 1,
        transition: "background 100ms ease",
        whiteSpace: "nowrap",
        ...style,
      }}
    >
      {icon}
      {children}
    </button>
  );
};

/* ---------- Text input ---------- */
const TextField = ({
  value,
  onChange,
  placeholder,
  password,
  readOnly,
  accent = "#4cc2ff",
  prefix,
  suffix,
  monospace,
  style,
}) => {
  const [focus, setFocus] = useState(false);
  const [reveal, setReveal] = useState(false);
  const inputType = password && !reveal ? "password" : "text";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        height: 32,
        background: "rgba(255,255,255,0.04)",
        border: "1px solid rgba(255,255,255,0.08)",
        borderRadius: 4,
        padding: "0 10px",
        gap: 8,
        position: "relative",
        transition: "background 120ms ease",
        ...style,
      }}
    >
      {prefix && (
        <span style={{ color: "rgba(255,255,255,0.55)", display: "flex" }}>
          {prefix}
        </span>
      )}
      <input
        type={inputType}
        value={value || ""}
        onChange={(e) => onChange && onChange(e.target.value)}
        placeholder={placeholder}
        readOnly={readOnly}
        onFocus={() => setFocus(true)}
        onBlur={() => setFocus(false)}
        style={{
          flex: 1,
          minWidth: 0,
          background: "transparent",
          border: "none",
          outline: "none",
          color: "#fff",
          fontFamily: monospace
            ? `"JetBrains Mono","Cascadia Code","SF Mono",ui-monospace,monospace`
            : "inherit",
          fontSize: 13,
        }}
      />
      {password && (
        <button
          onClick={() => setReveal(!reveal)}
          tabIndex={-1}
          style={{
            background: "transparent",
            border: "none",
            color: "rgba(255,255,255,0.55)",
            cursor: "pointer",
            padding: 2,
            display: "flex",
          }}
        >
          <IconEye size={14} />
        </button>
      )}
      {suffix}
      {/* focus underline accent — Fluent style */}
      {focus && (
        <span
          style={{
            position: "absolute",
            left: -1,
            right: -1,
            bottom: -1,
            height: 2,
            background: accent,
            borderRadius: "0 0 4px 4px",
            pointerEvents: "none",
          }}
        />
      )}
    </div>
  );
};

/* ---------- Select / dropdown ---------- */
const Select = ({ value, onChange, options, accent = "#4cc2ff", style }) => {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);

  useEffect(() => {
    if (!open) return;
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    document.addEventListener("mousedown", h);
    return () => document.removeEventListener("mousedown", h);
  }, [open]);

  const current = options.find(o => o.value === value) || options[0];

  return (
    <div ref={ref} style={{ position: "relative", ...style }}>
      <button
        onClick={() => setOpen(!open)}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          width: "100%",
          height: 32,
          padding: "0 10px",
          background: "rgba(255,255,255,0.04)",
          border: "1px solid rgba(255,255,255,0.08)",
          borderRadius: 4,
          color: "#fff",
          fontFamily: "inherit",
          fontSize: 13,
          cursor: "pointer",
          textAlign: "left",
        }}
      >
        <span>{current?.label}</span>
        <IconChevronDown size={14} />
      </button>
      {open && (
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 4px)",
            left: 0,
            right: 0,
            background: "#2c2c2c",
            border: "1px solid rgba(255,255,255,0.10)",
            borderRadius: 6,
            padding: 4,
            zIndex: 100,
            boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
            backdropFilter: "blur(20px)",
          }}
        >
          {options.map((o) => (
            <button
              key={o.value}
              onClick={() => { onChange(o.value); setOpen(false); }}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                width: "100%",
                padding: "8px 10px",
                background: o.value === value ? "rgba(255,255,255,0.06)" : "transparent",
                border: "none",
                borderRadius: 4,
                color: "#fff",
                fontFamily: "inherit",
                fontSize: 13,
                cursor: "pointer",
                textAlign: "left",
              }}
              onMouseEnter={(e) => { if (o.value !== value) e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
              onMouseLeave={(e) => { if (o.value !== value) e.currentTarget.style.background = "transparent"; }}
            >
              <span>{o.label}</span>
              {o.value === value && (
                <span style={{ color: accent, display: "flex" }}>
                  <IconCheck size={14} />
                </span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
};

/* ---------- SettingsCard — the core Fluent row ---------- */
const SettingsCard = ({
  icon,
  title,
  description,
  control,
  expandable,
  expanded,
  onToggleExpand,
  children,
  density = "comfortable",
  accent = "#4cc2ff",
  showIcons = true,
  status, // { kind: 'ok'|'warn'|'info', text: '...' }
}) => {
  const [hover, setHover] = useState(false);
  const padY = density === "compact" ? 10 : 14;
  const iconBg = density === "compact" ? 28 : 32;

  return (
    <div
      style={{
        background: "rgba(255,255,255,0.024)",
        border: "1px solid rgba(255,255,255,0.06)",
        borderRadius: 6,
        overflow: "hidden",
        transition: "background 120ms ease",
      }}
    >
      <div
        onClick={expandable ? onToggleExpand : undefined}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 14,
          padding: `${padY}px 16px`,
          cursor: expandable ? "pointer" : "default",
          background: hover && expandable ? "rgba(255,255,255,0.022)" : "transparent",
        }}
      >
        {showIcons && (
          <div
            style={{
              width: iconBg,
              height: iconBg,
              borderRadius: 4,
              background: "rgba(255,255,255,0.04)",
              border: "1px solid rgba(255,255,255,0.05)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: "rgba(255,255,255,0.85)",
              flexShrink: 0,
            }}
          >
            {icon}
          </div>
        )}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: 13,
            fontWeight: 500,
            color: "#fff",
            lineHeight: 1.3,
          }}>
            <span>{title}</span>
            {status && <StatusPill {...status} />}
          </div>
          {description && (
            <div style={{
              fontSize: 12,
              color: "rgba(255,255,255,0.6)",
              marginTop: 2,
              lineHeight: 1.4,
            }}>{description}</div>
          )}
        </div>
        <div
          onClick={(e) => e.stopPropagation()}
          style={{ display: "flex", alignItems: "center", gap: 10 }}
        >
          {control}
          {expandable && (
            <span
              style={{
                color: "rgba(255,255,255,0.6)",
                display: "flex",
                transform: expanded ? "rotate(180deg)" : "rotate(0)",
                transition: "transform 180ms ease",
              }}
            >
              <IconChevronDown size={16} />
            </span>
          )}
        </div>
      </div>
      {expandable && expanded && (
        <div
          style={{
            borderTop: "1px solid rgba(255,255,255,0.05)",
            padding: "14px 16px 16px",
            background: "rgba(0,0,0,0.18)",
          }}
        >
          {children}
        </div>
      )}
    </div>
  );
};

/* ---------- Status pill ---------- */
const StatusPill = ({ kind = "info", text }) => {
  const palette = {
    ok: { bg: "rgba(80,200,120,0.14)", fg: "#7ee2a4", dot: "#7ee2a4" },
    warn: { bg: "rgba(255,180,90,0.14)", fg: "#ffc278", dot: "#ffc278" },
    info: { bg: "rgba(76,194,255,0.14)", fg: "#7fd4ff", dot: "#7fd4ff" },
    off: { bg: "rgba(255,255,255,0.06)", fg: "rgba(255,255,255,0.55)", dot: "rgba(255,255,255,0.4)" },
  }[kind];
  return (
    <span style={{
      display: "inline-flex",
      alignItems: "center",
      gap: 6,
      padding: "1px 8px",
      height: 18,
      background: palette.bg,
      color: palette.fg,
      borderRadius: 9,
      fontSize: 11,
      fontWeight: 500,
      lineHeight: 1,
    }}>
      <span style={{
        width: 6, height: 6, borderRadius: 3, background: palette.dot,
      }}/>
      {text}
    </span>
  );
};

/* ---------- Subfield row (used inside expandable card body) ---------- */
const SubField = ({ label, children, helper, hint }) => (
  <div style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: 12, alignItems: "center", marginBottom: 10 }}>
    <div style={{ fontSize: 12, color: "rgba(255,255,255,0.75)" }}>
      {label}
      {hint && (
        <div style={{ fontSize: 10.5, color: "rgba(255,255,255,0.4)", marginTop: 1 }}>
          {hint}
        </div>
      )}
    </div>
    <div>
      {children}
      {helper && (
        <div style={{ fontSize: 11, color: "rgba(255,255,255,0.5)", marginTop: 4 }}>
          {helper}
        </div>
      )}
    </div>
  </div>
);

/* ---------- Color helper ---------- */
function shade(hex, percent) {
  // expects #rrggbb or short. returns shifted hex.
  let c = hex.replace("#", "");
  if (c.length === 3) c = c.split("").map(x => x + x).join("");
  const num = parseInt(c, 16);
  let r = (num >> 16) + Math.round(255 * percent / 100);
  let g = ((num >> 8) & 0xff) + Math.round(255 * percent / 100);
  let b = (num & 0xff) + Math.round(255 * percent / 100);
  r = Math.max(0, Math.min(255, r));
  g = Math.max(0, Math.min(255, g));
  b = Math.max(0, Math.min(255, b));
  return "#" + ((r << 16) | (g << 8) | b).toString(16).padStart(6, "0");
}

Object.assign(window, {
  ToggleSwitch, Button, TextField, Select, SettingsCard, StatusPill, SubField,
  shade,
});
