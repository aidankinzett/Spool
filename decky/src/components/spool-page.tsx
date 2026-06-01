import { Navigation, Focusable } from "@decky/ui";
import { useServerBase } from "../hooks/use-server-base";
import { LibraryGrid } from "./library-grid";

// ── Full-screen library page ───────────────────────────────────────────────
// The LAN tab navigates to /spool/lan (its own route) so the full back-stack
// works via the hardware B button.
export function SpoolPage() {
  const { base, error } = useServerBase();

  return (
    <div
      style={{
        height: "100%",
        overflowY: "scroll",
        padding: "2rem",
        boxSizing: "border-box",
      }}
    >
      <Focusable
        style={{
          display: "flex",
          gap: "0.5rem",
          alignItems: "center",
          marginBottom: "1.5rem",
        }}
      >
        <h1 style={{ margin: "0 1rem 0 0" }}>Spool</h1>
        <Focusable
          onActivate={() => Navigation.Navigate("/spool/lan")}
          style={{
            padding: "0.5rem 1.25rem",
            borderRadius: "6px",
            fontWeight: 600,
            background: "transparent",
            opacity: 0.7,
          }}
        >
          LAN
        </Focusable>
      </Focusable>

      {error && <div style={{ opacity: 0.8 }}>{error}</div>}
      {base && <LibraryGrid base={base} />}
    </div>
  );
}
