import { ModalRoot, ConfirmModal, DialogButton, Focusable, showModal } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { FaClockRotateLeft } from "react-icons/fa6";
import type { LibraryGame, SaveRevision } from "../types";
import { listSaveRevisions, restoreSaveRevision } from "../api/callables";
import { formatRelativeTime } from "../lib/format";

// "Restore an earlier save" picker for Game Mode — mirrors the desktop detail
// card's revision list. Lists the ludusavi backups retained locally (newest
// first) and rolls the game back to the chosen one. Restoring is a *pin*: the
// backend restores the revision then immediately backs up so it becomes the new
// tip (cloud-synced), which is why the current tip is shown but not selectable.
//
// Destructive — it replaces the live saves — so picking a revision routes
// through a ConfirmModal before anything is written.

const SPIN_KEYFRAMES = "@keyframes spool-revision-spin { to { transform: rotate(360deg); } }";

export function RevisionPickerModal({
  game,
  onRestored,
  onBusyChange,
  closeModal,
}: {
  game: LibraryGame;
  // Called after a successful restore so the caller can refresh its view.
  onRestored?: () => void;
  // Called with true when a restore starts and false when it ends (success or
  // failure). The game-page badge uses it to spin the bar's reel and refetch
  // the save state once the rolled-back tip is written.
  onBusyChange?: (busy: boolean) => void;
  // Injected by `showModal`.
  closeModal?: () => void;
}) {
  const [revisions, setRevisions] = useState<SaveRevision[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [restoring, setRestoring] = useState(false);

  useEffect(() => {
    void (async () => {
      const res = await listSaveRevisions(game.id);
      if (res.ok) setRevisions(res.revisions ?? []);
      else setLoadError(res.reason ?? "Couldn't load save history.");
    })();
  }, [game.id]);

  const doRestore = (rev: SaveRevision) => {
    setRestoring(true);
    onBusyChange?.(true);
    void (async () => {
      const res = await restoreSaveRevision(game.id, rev.name);
      setRestoring(false);
      onBusyChange?.(false);
      if (res.ok) {
        toaster.toast({
          title: "Spool",
          body: `Restored ${game.game_name} from ${formatRelativeTime(rev.when)} ✓`,
        });
        onRestored?.();
        closeModal?.();
      } else {
        toaster.toast({
          title: "Spool",
          body: `Restore failed: ${res.reason ?? "unknown error"}`,
        });
      }
    })();
  };

  // Confirm before overwriting the live saves.
  const confirmRestore = () => {
    const rev = revisions?.find((r) => r.name === selected);
    if (!rev) return;
    showModal(
      <ConfirmModal
        strTitle={`Restore ${game.game_name}?`}
        strDescription={
          `This replaces the current saves with the backup from ${absolute(rev.when)} ` +
          `(${formatRelativeTime(rev.when)}) and makes it the new latest backup. ` +
          `Your current saves will roll off after the retention limit.`
        }
        strOKButtonText="Restore this save"
        strCancelButtonText="Cancel"
        bDestructiveWarning
        onOK={() => doRestore(rev)}
      />,
    );
  };

  // Revisions that can actually be rolled back to (everything but the tip).
  const olderCount = (revisions ?? []).filter((r) => !r.is_current).length;

  return (
    <ModalRoot closeModal={closeModal}>
      <style>{SPIN_KEYFRAMES}</style>
      <h2 style={{ margin: "0 0 0.25rem", fontSize: "1.3rem", fontWeight: 700 }}>
        Restore an earlier save
      </h2>
      <div style={{ opacity: 0.7, fontSize: "0.85rem", marginBottom: "0.75rem" }}>
        Roll {game.game_name} back to one of its retained backups. The restored
        save becomes the new latest backup and syncs to your other devices.
      </div>

      {loadError ? (
        <div style={{ opacity: 0.8, fontSize: "0.9rem", padding: "1rem 0" }}>{loadError}</div>
      ) : !revisions ? (
        <div style={{ opacity: 0.7, fontSize: "0.9rem", padding: "1rem 0" }}>
          Loading save history…
        </div>
      ) : olderCount === 0 ? (
        <div style={{ opacity: 0.7, fontSize: "0.9rem", padding: "1rem 0" }}>
          {revisions.length === 0
            ? "No backups yet — play the game once to capture a save."
            : "Only one backup exists, so there's nothing earlier to restore to."}
        </div>
      ) : (
        <Focusable
          style={{
            maxHeight: "45vh",
            overflowY: "scroll",
            display: "flex",
            flexDirection: "column",
            gap: "0.4rem",
            opacity: restoring ? 0.5 : 1,
            pointerEvents: restoring ? "none" : "auto",
          }}
        >
          {revisions.map((rev) => {
            const isSelected = selected === rev.name;
            return (
              <DialogButton
                key={rev.name}
                disabled={restoring || rev.is_current}
                onClick={() => setSelected(rev.name)}
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
                <div style={{ width: "18px", flexShrink: 0, opacity: 0.85 }}>
                  {!rev.is_current && <FaClockRotateLeft size={14} />}
                </div>
                <div style={{ display: "flex", flexDirection: "column" }}>
                  <span>{absolute(rev.when)}</span>
                  <span style={{ opacity: 0.55, fontSize: "0.78rem" }}>
                    {rev.is_current
                      ? `Current save · ${formatRelativeTime(rev.when)}`
                      : formatRelativeTime(rev.when)}
                  </span>
                </div>
              </DialogButton>
            );
          })}
        </Focusable>
      )}

      <Focusable
        style={{ display: "flex", alignItems: "center", gap: "0.75rem", marginTop: "1rem" }}
      >
        {restoring ? (
          <div style={{ display: "flex", alignItems: "center", gap: "0.6rem", flex: 1 }}>
            <div
              style={{
                width: "18px",
                height: "18px",
                border: "2px solid rgba(255,255,255,0.25)",
                borderTopColor: "#fff",
                borderRadius: "50%",
                animation: "spool-revision-spin 0.8s linear infinite",
              }}
            />
            <span style={{ opacity: 0.85, fontSize: "0.9rem" }}>Restoring…</span>
          </div>
        ) : (
          <>
            <DialogButton
              disabled={!selected || olderCount === 0}
              onClick={confirmRestore}
              style={{ flex: 1 }}
            >
              Restore
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

// A compact absolute timestamp ("6 May, 21:14") to pair with the relative one —
// relative time alone gets vague for older revisions ("3w ago").
function absolute(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
