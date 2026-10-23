import type { CSSProperties, ReactNode } from "react";
import { Focusable } from "@decky/ui";
import { FaTrophy } from "react-icons/fa6";

// Colours from the desktop app's design tokens (tauri/src/app.css).
export const T = {
  bg1: "#101216",
  bg2: "#15181d",
  bg3: "#1c2027",
  line1: "rgba(255,255,255,0.06)",
  line2: "rgba(255,255,255,0.10)",
  ink0: "#f4f4f5",
  ink1: "rgba(244,244,245,0.78)",
  ink2: "rgba(244,244,245,0.56)",
  ink3: "rgba(244,244,245,0.36)",
  spool: "#d7c9a0",
  mono: `"JetBrains Mono", ui-monospace, monospace`,
};

// Mono eyebrow label: wide-tracked, uppercase, monospaced.
export function MonoLabel({
  size = 10,
  color = T.ink2,
  children,
}: {
  size?: number;
  color?: string;
  children: ReactNode;
}) {
  return (
    <span
      style={{
        fontFamily: T.mono,
        fontSize: size,
        letterSpacing: "0.12em",
        textTransform: "uppercase",
        color,
      }}
    >
      {children}
    </span>
  );
}

// Card with an accent strip and mono title in its header, ported from the
// desktop `DetailCard.svelte`.
//
// The card is a Focusable so the controller can land on it, which scrolls the
// tab to it (a Focusable takes focus once it has an `onActivate`). Cards with
// their own focus stops (buttons, or rows to step through) pass `flow` so
// focus moves into those instead.
export function DetailCard({
  title,
  accent = T.spool,
  action,
  flow = false,
  style,
  children,
}: {
  title: string;
  accent?: string;
  action?: ReactNode;
  flow?: boolean;
  /** Merged into the card's own style, e.g. for outer margins. */
  style?: CSSProperties;
  children: ReactNode;
}) {
  const section: CSSProperties = {
    minWidth: 0,
    overflow: "hidden",
    borderRadius: 6,
    border: `1px solid ${T.line1}`,
    background: T.bg1,
    color: T.ink0,
  };
  return (
    <Focusable
      style={{ ...section, ...style }}
      onActivate={flow ? undefined : () => {}}
      flow-children={flow ? "column" : undefined}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
          padding: "10px 14px",
          background: T.bg2,
          borderBottom: `1px dashed ${T.line1}`,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ width: 4, height: 14, borderRadius: 1, background: accent }} />
          <MonoLabel size={10}>{title}</MonoLabel>
        </div>
        {action}
      </div>
      <div style={{ padding: 14 }}>{children}</div>
    </Focusable>
  );
}

// Achievement icon in a bordered square; a trophy glyph when there's no icon.
export function AchievementIcon({
  src,
  size,
  glow,
  title,
}: {
  src: string | null;
  size: number;
  glow?: string;
  title?: string;
}) {
  return (
    <div
      title={title}
      style={{
        width: size,
        height: size,
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        overflow: "hidden",
        borderRadius: 2,
        border: `1px solid ${T.line2}`,
        background: T.bg2,
        color: T.ink3,
        boxShadow: glow ? `0 0 10px ${glow}4d` : undefined,
      }}
    >
      {src ? (
        <img src={src} alt="" style={{ width: "100%", height: "100%", objectFit: "cover" }} />
      ) : (
        <FaTrophy size={Math.round(size * 0.4)} />
      )}
    </div>
  );
}
