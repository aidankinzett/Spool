/**
 * Path-matching helpers for relating a game's install folder to the configured
 * library folders.
 *
 * An install lives at `<library folder>/<game folder>`, so a library folder is
 * a game's current location when it is the parent of the game's folder. Windows
 * paths are case-insensitive and mix `\` / `/` separators, and the configured
 * root (canonicalised by the backend) can differ in casing/slashes from the
 * recorded game folder — so both sides are folded before comparison. Backend
 * canonicalisation stays the authoritative no-op guard; this is only to keep the
 * UI's grouping/"current location" detection from missing a match.
 *
 * Pure functions, no IO — shared by MoveInstallModal, BatchMoveModal, and the
 * Settings → Library grouped games view.
 */

/** Strip trailing slashes/backslashes. */
export function normPath(p: string): string {
  return p.replace(/[\\/]+$/, '');
}

/** The parent directory of `p` (drops the last path segment). */
export function parentOf(p: string): string {
  return normPath(p).replace(/[\\/][^\\/]+$/, '');
}

/**
 * Fold a path for comparison. Windows-style paths (drive letter or any
 * backslash) are lowercased and slash-normalised; POSIX paths are left
 * case-sensitive (only trailing separators trimmed).
 */
export function canonPath(p: string): string {
  const t = normPath(p);
  return /^[a-zA-Z]:[\\/]/.test(p) || p.includes('\\') ? t.replace(/\\/g, '/').toLowerCase() : t;
}

/**
 * Whether `root` is the library folder that `gameFolderPath` lives directly
 * inside. False when the game has no install folder.
 */
export function isCurrentRoot(root: string, gameFolderPath: string | null | undefined): boolean {
  if (!gameFolderPath) return false;
  return canonPath(parentOf(gameFolderPath)) === canonPath(root);
}

/** Whether `gameFolderPath` is equal to or nested anywhere under `root`. */
export function isInsideRoot(root: string, gameFolderPath: string | null | undefined): boolean {
  if (!root || !gameFolderPath) return false;
  const foldedRoot = canonPath(root);
  const foldedGameFolder = canonPath(gameFolderPath);
  return (
    foldedGameFolder === foldedRoot ||
    foldedGameFolder.startsWith(`${foldedRoot}/`) ||
    foldedGameFolder.startsWith(`${foldedRoot}\\`)
  );
}

/**
 * Mirror the backend's headroom rule: the on-disk footprint exceeds the file
 * byte total (cluster rounding, directory metadata), so an exact fit would fail
 * mid-copy. Reserve the larger of 1% or 256 MiB on top of the payload.
 */
export function neededBytes(size: number): number {
  return size + Math.max(size / 100, 256 * 1048576);
}
