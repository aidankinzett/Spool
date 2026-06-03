import { ModalRoot, DialogButton, ToggleField, Focusable } from "@decky/ui";
import { toaster } from "@decky/api";
import { useState } from "react";
import type { LibraryGame } from "../types";
import { installDeps } from "../api/callables";

// Common winetricks verbs for getting Windows games running under Proton.
// Mirrors the desktop edit page's "Install dependencies" field, but as a
// controller-friendly preset list so there's no on-screen keyboard in Game
// Mode. `verb` is the exact token passed to `winetricks`.
const PRESET_VERBS: { verb: string; label: string }[] = [
  { verb: "vcrun2022", label: "Visual C++ 2015–2022 runtime" },
  { verb: "vcrun2019", label: "Visual C++ 2019 runtime" },
  { verb: "vcrun2017", label: "Visual C++ 2017 runtime" },
  { verb: "vcrun2015", label: "Visual C++ 2015 runtime" },
  { verb: "vcrun2013", label: "Visual C++ 2013 runtime" },
  { verb: "vcrun2012", label: "Visual C++ 2012 runtime" },
  { verb: "vcrun2010", label: "Visual C++ 2010 runtime" },
  { verb: "dotnet48", label: ".NET Framework 4.8" },
  { verb: "dotnet40", label: ".NET Framework 4.0" },
  { verb: "d3dx9", label: "DirectX 9 runtime (d3dx9)" },
  { verb: "d3dcompiler_47", label: "D3D shader compiler (47)" },
  { verb: "d3dcompiler_43", label: "D3D shader compiler (43)" },
  { verb: "xact", label: "XAudio2 / XACT audio" },
  { verb: "dxvk", label: "DXVK (DX9/10/11 → Vulkan)" },
  { verb: "vkd3d", label: "VKD3D (DX12 → Vulkan)" },
  { verb: "corefonts", label: "Microsoft core fonts" },
  { verb: "physx", label: "NVIDIA PhysX" },
];

// Inline keyframes for the busy spinner — injected once with the component so
// we don't depend on a particular @decky/ui spinner export.
const SPIN_KEYFRAMES = "@keyframes spool-deps-spin { to { transform: rotate(360deg); } }";

export function InstallDepsModal({
  game,
  closeModal,
}: {
  game: LibraryGame;
  // Injected by `showModal`.
  closeModal?: () => void;
}) {
  // Default to vcrun2022 — the single most common requirement and the desktop
  // edit page's default.
  const [selected, setSelected] = useState<Set<string>>(new Set(["vcrun2022"]));
  const [installing, setInstalling] = useState(false);

  const toggle = (verb: string, on: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (on) next.add(verb);
      else next.delete(verb);
      return next;
    });
  };

  const run = () => {
    // Preserve preset order so the verb string is stable/readable.
    const verbs = PRESET_VERBS.filter((v) => selected.has(v.verb))
      .map((v) => v.verb)
      .join(" ");
    if (!verbs || installing) return;
    setInstalling(true);
    void (async () => {
      const res = await installDeps(game.id, verbs);
      setInstalling(false);
      if (res.ok) {
        toaster.toast({ title: "Spool", body: `Installed: ${verbs}` });
        closeModal?.();
      } else {
        toaster.toast({
          title: "Spool",
          body: `Couldn't install: ${res.reason ?? "unknown error"}`,
        });
      }
    })();
  };

  return (
    <ModalRoot closeModal={closeModal}>
      <style>{SPIN_KEYFRAMES}</style>
      <h2 style={{ margin: "0 0 0.25rem", fontSize: "1.3rem", fontWeight: 700 }}>
        Install dependencies
      </h2>
      <div style={{ opacity: 0.7, fontSize: "0.85rem", marginBottom: "0.75rem" }}>
        Install Windows runtimes into {game.game_name}'s Proton prefix via
        winetricks. Needs a UMU or GE Proton version set for the game.
      </div>

      <Focusable
        style={{
          maxHeight: "45vh",
          overflowY: "scroll",
          opacity: installing ? 0.5 : 1,
          pointerEvents: installing ? "none" : "auto",
        }}
      >
        {PRESET_VERBS.map((v) => (
          <ToggleField
            key={v.verb}
            label={v.label}
            description={v.verb}
            checked={selected.has(v.verb)}
            disabled={installing}
            onChange={(on: boolean) => toggle(v.verb, on)}
          />
        ))}
      </Focusable>

      <Focusable
        style={{
          display: "flex",
          alignItems: "center",
          gap: "0.75rem",
          marginTop: "1rem",
        }}
      >
        {installing ? (
          <div style={{ display: "flex", alignItems: "center", gap: "0.6rem", flex: 1 }}>
            <div
              style={{
                width: "18px",
                height: "18px",
                border: "2px solid rgba(255,255,255,0.25)",
                borderTopColor: "#fff",
                borderRadius: "50%",
                animation: "spool-deps-spin 0.8s linear infinite",
              }}
            />
            <span style={{ opacity: 0.85, fontSize: "0.9rem" }}>
              Installing… this can take a few minutes.
            </span>
          </div>
        ) : (
          <>
            <DialogButton
              disabled={selected.size === 0}
              onClick={run}
              style={{ flex: 1 }}
            >
              Install {selected.size > 0 ? `(${selected.size})` : ""}
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
