import { toaster } from "@decky/api";
import {
  ConfirmModal,
  DialogButton,
  Menu,
  MenuItem,
  MenuSeparator,
  showContextMenu,
  showModal,
} from "@decky/ui";
import { FaEllipsisVertical } from "react-icons/fa6";
import type { LibraryGame } from "../../types";
import { backupNow, deleteGame, pullCloudSaves } from "../../api/callables";
import { backupStarted, backupFinished } from "../../lib/backup-status";
import { forgetAppid } from "../../lib/appid-map";
import { steamApps } from "../../lib/steam";
import { InstallDepsModal } from "../install-deps-modal";
import { ProtonVersionModal } from "../proton-version-modal";
import { RevisionPickerModal } from "../revision-picker-modal";

// Three-dots button rendered on the right of the game-page Spool bar. Opens a
// Steam context menu (showContextMenu) anchored to itself with Spool actions
// for the matched game. (Action logic unchanged from the original; only the
// trigger button is resized to sit inline in the compact bar.)
export function BadgeMenuButton({ game, appid }: { game: LibraryGame; appid: number }) {
  // Winetricks only applies to Windows `.exe` games launched through Proton;
  // native Linux games don't use a prefix.
  const canInstallDeps = game.exe_path?.toLowerCase().endsWith(".exe") ?? false;
  const canDelete = !!game.game_folder_path;
  // Rolling back needs at least one retained backup to restore to.
  const canRestore = game.save_backup_count > 0;

  const runBackup = () => {
    void (async () => {
      // Drive the same backup-status store the on_app_stop events feed, so the
      // bar shows its spinning reel and the patch wrapper refetches when this
      // finishes.
      backupStarted(appid);
      try {
        const res = await backupNow();
        if (res.ok) {
          toaster.toast({ title: "Spool", body: `Backed up ${res.game ?? game.game_name} ✓` });
        } else if (!res.acted) {
          toaster.toast({ title: "Spool", body: "No active session to back up." });
        } else {
          toaster.toast({ title: "Spool", body: `Backup failed: ${res.reason || "unknown error"}` });
        }
      } finally {
        backupFinished(appid);
      }
    })();
  };

  // Pull the latest cloud saves down to this device without launching. Pull-
  // only — never uploads. A true divergence is reported as a conflict the user
  // resolves in the desktop app (the Deck has no conflict modal).
  const runPull = () => {
    void (async () => {
      backupStarted(appid);
      try {
        const res = await pullCloudSaves(game.id);
        if (!res.ok) {
          const reason = res.reason || "unknown error";
          const body = /cloud sync conflict/i.test(reason)
            ? "Cloud conflict — resolve it in the Spool desktop app."
            : `Sync failed: ${reason}`;
          toaster.toast({ title: "Spool", body });
          return;
        }
        switch (res.outcome) {
          case "pulled":
            toaster.toast({ title: "Spool", body: `Pulled latest saves for ${game.game_name} ✓` });
            break;
          case "up_to_date":
            toaster.toast({ title: "Spool", body: `${game.game_name} is already up to date` });
            break;
          case "local_newer":
            toaster.toast({ title: "Spool", body: "Local saves are newer — nothing to pull." });
            break;
          case "unconfigured":
            toaster.toast({ title: "Spool", body: "No cloud remote configured." });
            break;
        }
      } finally {
        backupFinished(appid);
      }
    })();
  };

  const confirmDelete = () => {
    showModal(
      <ConfirmModal
        strTitle={`Delete ${game.game_name} from disk?`}
        strDescription={
          "This permanently removes the install folder" +
          (game.game_folder_path ? `\n${game.game_folder_path}` : "") +
          "\nand its library entry. This can't be undone."
        }
        strOKButtonText="Delete from disk"
        strCancelButtonText="Cancel"
        bDestructiveWarning
        onOK={() => {
          void (async () => {
            const res = await deleteGame(game.id);
            if (res.ok) {
              // This badge lives on the game's own Steam page, so `appid` is its
              // non-Steam shortcut. Remove it too and forget the stored mapping.
              try {
                steamApps()?.RemoveShortcut?.(appid);
              } catch (e) {
                console.warn("[Spool] RemoveShortcut failed", e);
              }
              forgetAppid(game.id);
              toaster.toast({ title: "Spool", body: `Deleted ${game.game_name} from disk` });
            } else {
              toaster.toast({ title: "Spool", body: `Couldn't delete: ${res.reason ?? "unknown error"}` });
            }
          })();
        }}
      />,
    );
  };

  const openMenu = (e: MouseEvent) => {
    showContextMenu(
      <Menu label="Spool">
        <MenuItem onSelected={runBackup}>Back up now</MenuItem>
        <MenuItem onSelected={runPull}>Sync now (pull)</MenuItem>
        {canRestore && (
          <MenuItem
            onSelected={() =>
              showModal(
                <RevisionPickerModal
                  game={game}
                  onBusyChange={(busy) =>
                    busy ? backupStarted(appid) : backupFinished(appid)
                  }
                />,
              )
            }
          >
            Restore a save…
          </MenuItem>
        )}
        {canInstallDeps && (
          <MenuItem onSelected={() => showModal(<ProtonVersionModal game={game} />)}>
            Proton version
          </MenuItem>
        )}
        {canInstallDeps && (
          <MenuItem onSelected={() => showModal(<InstallDepsModal game={game} />)}>
            Install dependencies
          </MenuItem>
        )}
        {canDelete && <MenuSeparator />}
        {canDelete && (
          <MenuItem tone="destructive" onSelected={confirmDelete}>
            Delete from disk
          </MenuItem>
        )}
      </Menu>,
      e.currentTarget ?? undefined,
    );
  };

  return (
    <DialogButton
      // Sizing only — leave background/focus to the native DialogButton so the
      // button gets Steam's standard hover/focus fill (overriding `background`
      // here kept the dark fill on focus, leaving just a black icon).
      style={{
        minWidth: 0,
        width: "36px",
        height: "36px",
        padding: 0,
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: "4px",
      }}
      onClick={openMenu}
    >
      <FaEllipsisVertical />
    </DialogButton>
  );
}
