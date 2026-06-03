import { toaster } from "@decky/api";
import { DialogButton, Menu, MenuItem, Navigation, showContextMenu } from "@decky/ui";
import { FaEllipsisVertical } from "react-icons/fa6";
import type { LibraryGame } from "../../types";
import { backupNow } from "../../api/callables";
import { backupStarted, backupFinished } from "../../lib/backup-status";

// Three-dots button rendered on the right of the game-page badge row. Opens a
// Steam context menu (showContextMenu) anchored to itself with Spool actions
// for the matched game.
export function BadgeMenuButton({ game, appid }: { game: LibraryGame; appid: number }) {
  const openMenu = (e: MouseEvent) => {
    showContextMenu(
      <Menu label="Spool">
        <MenuItem onSelected={() => Navigation.Navigate(`/spool/game/${game.id}`)}>
          Open in Spool
        </MenuItem>
        <MenuItem
          onSelected={() => {
            void (async () => {
              // Drive the same backup-status store the on_app_stop events feed,
              // so the badge shows its spinner and the patch wrapper refetches
              // when this finishes.
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
          }}
        >
          Back up now
        </MenuItem>
      </Menu>,
      e.currentTarget ?? undefined,
    );
  };

  return (
    <DialogButton
      style={{
        minWidth: 0,
        width: "48px",
        height: "48px",
        padding: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
      onClick={openMenu}
    >
      <FaEllipsisVertical />
    </DialogButton>
  );
}
