import { ModalRoot, DialogButton, Focusable } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { FaCheck } from "react-icons/fa6";
import type { LibraryGame, ProtonVersion } from "../types";
import { listProtonVersions, setProtonVersion } from "../api/callables";

// Per-game Proton version picker. Mirrors the desktop edit page's Proton
// dropdown, but as a controller-friendly list for Game Mode. The empty-path
// "Auto" entry clears the override so umu-run picks its own default
// (its bundled UMU-Proton).
//
// `path` is the absolute Proton dir stored on the game's `proton_version_path`
// — the same value the desktop dropdown writes — so the two stay interchangeable.

const SPIN_KEYFRAMES = "@keyframes spool-proton-spin { to { transform: rotate(360deg); } }";

export function ProtonVersionModal({
  game,
  onSaved,
  closeModal,
}: {
  game: LibraryGame;
  // Called after a successful save so the caller can refresh its view.
  onSaved?: () => void;
  // Injected by `showModal`.
  closeModal?: () => void;
}) {
  const [versions, setVersions] = useState<ProtonVersion[]>([]);
  const [loading, setLoading] = useState(true);
  // "" = Auto (no override). Seeded from the game's current pin.
  const [selected, setSelected] = useState<string>(game.proton_version_path ?? "");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void (async () => {
      const list = await listProtonVersions();
      setVersions(list);
      setLoading(false);
    })();
  }, []);

  const save = () => {
    if (saving) return;
    setSaving(true);
    void (async () => {
      const res = await setProtonVersion(game.id, selected);
      setSaving(false);
      if (res.ok) {
        const label =
          versions.find((v) => v.path === selected)?.name ?? "Auto";
        toaster.toast({ title: "Spool", body: `Proton: ${label}` });
        onSaved?.();
        closeModal?.();
      } else {
        toaster.toast({
          title: "Spool",
          body: `Couldn't set Proton: ${res.reason ?? "unknown error"}`,
        });
      }
    })();
  };

  // Rows: "Auto" first (empty path), then each discovered build.
  const rows: { path: string; name: string; sub?: string }[] = [
    { path: "", name: "Auto (newest installed)", sub: "Let umu-run choose" },
    ...versions.map((v) => ({ path: v.path, name: v.name, sub: v.source })),
  ];

  return (
    <ModalRoot closeModal={closeModal}>
      <style>{SPIN_KEYFRAMES}</style>
      <h2 style={{ margin: "0 0 0.25rem", fontSize: "1.3rem", fontWeight: 700 }}>
        Proton version
      </h2>
      <div style={{ opacity: 0.7, fontSize: "0.85rem", marginBottom: "0.75rem" }}>
        Choose the Proton build {game.game_name} launches with. Auto lets
        umu-run pick its bundled default — change it only if a specific build
        runs the game better.
      </div>

      {loading ? (
        <div style={{ opacity: 0.7, fontSize: "0.9rem", padding: "1rem 0" }}>
          Looking for installed Proton builds…
        </div>
      ) : (
        <Focusable
          style={{
            maxHeight: "45vh",
            overflowY: "scroll",
            display: "flex",
            flexDirection: "column",
            gap: "0.4rem",
            opacity: saving ? 0.5 : 1,
            pointerEvents: saving ? "none" : "auto",
          }}
        >
          {rows.map((row) => {
            const isSelected = selected === row.path;
            return (
              <DialogButton
                key={row.path || "__auto__"}
                disabled={saving}
                onClick={() => setSelected(row.path)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.6rem",
                  textAlign: "left",
                  justifyContent: "flex-start",
                  border: isSelected
                    ? "1px solid rgba(255,255,255,0.6)"
                    : "1px solid transparent",
                }}
              >
                <div style={{ width: "16px", flexShrink: 0 }}>
                  {isSelected && <FaCheck />}
                </div>
                <div style={{ display: "flex", flexDirection: "column" }}>
                  <span>{row.name}</span>
                  {row.sub && (
                    <span style={{ opacity: 0.55, fontSize: "0.78rem" }}>{row.sub}</span>
                  )}
                </div>
              </DialogButton>
            );
          })}
        </Focusable>
      )}

      <Focusable
        style={{
          display: "flex",
          alignItems: "center",
          gap: "0.75rem",
          marginTop: "1rem",
        }}
      >
        {saving ? (
          <div style={{ display: "flex", alignItems: "center", gap: "0.6rem", flex: 1 }}>
            <div
              style={{
                width: "18px",
                height: "18px",
                border: "2px solid rgba(255,255,255,0.25)",
                borderTopColor: "#fff",
                borderRadius: "50%",
                animation: "spool-proton-spin 0.8s linear infinite",
              }}
            />
            <span style={{ opacity: 0.85, fontSize: "0.9rem" }}>Saving…</span>
          </div>
        ) : (
          <>
            <DialogButton disabled={loading} onClick={save} style={{ flex: 1 }}>
              Save
            </DialogButton>
            <DialogButton onClick={() => closeModal?.()} style={{ flex: 1 }}>
              Cancel
            </DialogButton>
          </>
        )}
      </Focusable>
    </ModalRoot>
  );
}
