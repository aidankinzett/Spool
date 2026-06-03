import { useSyncExternalStore } from "react";
import { isBackingUp, subscribe } from "../lib/backup-status";

// True while a Spool save backup is actively running for the given Steam appid.
// Backed by the module-level store in backup-status.ts, so a badge that mounts
// mid-backup picks up the in-progress state immediately.
export function useBackingUp(appid: number): boolean {
  return useSyncExternalStore(
    subscribe,
    () => isBackingUp(appid),
  );
}
