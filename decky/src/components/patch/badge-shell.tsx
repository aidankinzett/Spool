import type { ReactNode } from "react";

// Shared pill container for the badges injected on the Steam
// /library/app/:appid page (see backup-badge.tsx, playtime-badge.tsx).
export function BadgeShell({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        padding: "0.4rem 0.75rem",
        borderRadius: "4px",
        background: "rgba(255,255,255,0.08)",
        fontSize: "0.8rem",
        fontWeight: 600,
      }}
    >
      {children}
    </div>
  );
}

// Dot separator used between badge segments.
export const BadgeSep = () => <span style={{ opacity: 0.3, margin: "0 0.3rem" }}>·</span>;
