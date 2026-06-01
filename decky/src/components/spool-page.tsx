import { Focusable } from "@decky/ui";
import { useState } from "react";
import { useServerBase } from "../hooks/use-server-base";
import { LibraryGrid } from "./library-grid";
import { LanView } from "./lan-view";

// ── Full-screen page: Library | LAN toggle ─────────────────────────────────
export function SpoolPage() {
  const { base, error } = useServerBase();
  const [view, setView] = useState<"library" | "lan">("library");

  const TabButton = ({ id, label }: { id: "library" | "lan"; label: string }) => (
    <Focusable
      onActivate={() => setView(id)}
      style={{
        padding: "0.5rem 1.25rem",
        borderRadius: "6px",
        fontWeight: 600,
        background: view === id ? "#2a3a52" : "transparent",
        opacity: view === id ? 1 : 0.7,
      }}
    >
      {label}
    </Focusable>
  );

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
        <TabButton id="library" label="Library" />
        <TabButton id="lan" label="LAN" />
      </Focusable>

      {error && <div style={{ opacity: 0.8 }}>{error}</div>}
      {base && view === "library" && <LibraryGrid base={base} />}
      {base && view === "lan" && <LanView base={base} />}
    </div>
  );
}
