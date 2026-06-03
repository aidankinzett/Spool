// Tracks in-flight Spool save backups by Steam appid, fed by the decky backup
// events wired in index.tsx (spool_backup_started / spool_backup_finished).
//
// A game-stop fires `started` unconditionally, but most stops are no-ops (Spool's
// runner already backed up before the forced-close fallback) and resolve almost
// instantly. To avoid a one-frame spinner flash on every stop, a `started` only
// flips the appid to "backing up" after it has stayed unresolved for DEBOUNCE_MS;
// a real forced-close backup takes seconds and crosses that threshold, a no-op
// resolves first and cancels the pending flip.
const DEBOUNCE_MS = 200;

const active = new Set<number>();
const pending = new Map<number, ReturnType<typeof setTimeout>>();
const listeners = new Set<() => void>();

function notify() {
  for (const l of listeners) l();
}

export function backupStarted(appid: number): void {
  if (active.has(appid) || pending.has(appid)) return;
  const timer = setTimeout(() => {
    pending.delete(appid);
    active.add(appid);
    notify();
  }, DEBOUNCE_MS);
  pending.set(appid, timer);
}

export function backupFinished(appid: number): void {
  const timer = pending.get(appid);
  if (timer != null) {
    clearTimeout(timer);
    pending.delete(appid);
  }
  if (active.delete(appid)) notify();
}

export function isBackingUp(appid: number): boolean {
  return active.has(appid);
}

export function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}
