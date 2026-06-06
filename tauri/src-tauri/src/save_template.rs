//! Portable save-location templates for custom (non-manifest) games.
//!
//! When the user picks a save folder for a game ludusavi's manifest doesn't
//! cover, Spool stores a *portable* template rather than the literal path, so
//! the same definition works on every device and OS. The vocabulary is
//! ludusavi's own path placeholders (see the manifest format), which ludusavi
//! resolves per-machine — and, with `--wine-prefix` (which the run workflow
//! already passes for Proton games), resolves Windows placeholders *into* the
//! prefix's `drive_c`. So `<winLocalAppData>/MyGame` means
//! `%LOCALAPPDATA%\MyGame` on Windows and
//! `<prefix>/drive_c/users/steamuser/AppData/Local/MyGame` under Proton — the
//! exact two locations [`crate::redirects`] already maps between, so a custom
//! game rides the existing cross-device restore/backup machinery unchanged.
//!
//! This module is the *inverse* of `redirects.rs`: it classifies a concrete
//! picked folder back into a placeholder template ([`classify`]), and expands
//! the one placeholder ludusavi can't resolve on its own — `<base>`, the game's
//! install folder — into a concrete per-device path ([`expand_base`]).
//!
//! ## What [`classify`] produces
//!
//! | Picked folder | Token |
//! |---|---|
//! | `…/drive_c/users/<u>/AppData/Local/X` or Windows `C:/Users/<u>/AppData/Local/X` | `<winLocalAppData>/X` |
//! | `…/AppData/LocalLow/X` | `<winLocalAppDataLow>/X` |
//! | `…/AppData/Roaming/X` | `<winAppData>/X` |
//! | `…/Documents/X` | `<winDocuments>/X` |
//! | `…/users/Public/X` or `C:/Users/Public/X` | `<winPublic>/X` |
//! | `…/ProgramData/X` or `C:/ProgramData/X` | `<winProgramData>/X` |
//! | under the game's install folder | `<base>/X` (expanded per device) |
//! | under the Linux home dir | `<home>/X` (portable across Linux users) |
//! | anything else | the literal path (works locally; may not cross OSes) |

/// Normalise to forward slashes and drop a trailing slash (but keep a bare `/`).
fn norm(p: &str) -> String {
    let s = p.replace('\\', "/");
    let t = s.trim_end_matches('/');
    if t.is_empty() { "/".to_string() } else { t.to_string() }
}

/// Case-insensitively strip `prefix/` (or exactly `prefix`) from the front of
/// `path`, returning the remainder (no leading slash). Used so a Windows-cased
/// `AppData/Local` matches regardless of how the filesystem reported it.
fn strip_prefix_ci<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let pl = path.len();
    let xl = prefix.len();
    if pl < xl || !path[..xl].eq_ignore_ascii_case(prefix) {
        return None;
    }
    match path[xl..].strip_prefix('/') {
        Some(rest) => Some(rest),
        None if pl == xl => Some(""),
        None => None,
    }
}

/// Join a placeholder root with a (possibly empty) remainder.
fn join(root: &str, rest: &str) -> String {
    if rest.is_empty() {
        root.to_string()
    } else {
        format!("{root}/{rest}")
    }
}

/// The Windows known-folder mappings, applied to the path *after* the user-home
/// prefix has been stripped (so the leading segment is e.g. `AppData/Local`).
/// Order matters: `AppData/LocalLow` is tested before `AppData/Local`.
fn windows_known_folder(after_user: &str) -> Option<String> {
    const RULES: &[(&str, &str)] = &[
        ("AppData/LocalLow", "<winLocalAppDataLow>"),
        ("AppData/Local", "<winLocalAppData>"),
        ("AppData/Roaming", "<winAppData>"),
        ("Documents", "<winDocuments>"),
    ];
    for (needle, token) in RULES {
        if let Some(rest) = strip_prefix_ci(after_user, needle) {
            return Some(join(token, rest));
        }
    }
    None
}

/// Classify a picked save folder into a portable ludusavi template.
///
/// * `picked` — the absolute folder the user chose.
/// * `prefix_root` — the game's Proton/Wine prefix ROOT (the dir containing
///   `drive_c`), when it launches through Proton; `None` otherwise.
/// * `game_folder` — the game's install folder (`game_folder_path`), if known.
/// * `home` — the OS home directory, for native-Linux save portability.
pub fn classify(
    picked: &str,
    prefix_root: Option<&str>,
    game_folder: Option<&str>,
    home: Option<&str>,
) -> String {
    let p = norm(picked);

    // 1. Inside the game's Proton/Wine prefix → Windows known folder in drive_c.
    if let Some(root) = prefix_root {
        let drive_c = format!("{}/drive_c", norm(root));
        if let Some(rest) = strip_prefix_ci(&p, &drive_c) {
            if let Some(tok) = drive_c_known_folder(rest) {
                return tok;
            }
            // Under the prefix but not a recognised known folder — keep it
            // prefix-relative is meaningless across devices, so fall through to
            // the install-folder / literal handling below.
        }
    }

    // 2. Windows drive path (C:/Users/<u>/AppData/Local/…, C:/ProgramData/…).
    if is_windows_drive_path(&p) {
        if let Some(tok) = windows_drive_known_folder(&p) {
            return tok;
        }
    }

    // 3. Under the game's install folder → `<base>/rest`.
    if let Some(folder) = game_folder.filter(|f| !f.trim().is_empty()) {
        let f = norm(folder);
        if let Some(rest) = strip_prefix_ci(&p, &f) {
            return join("<base>", rest);
        }
    }

    // 4. Under the Linux home dir → `<home>/rest` (portable across Linux users).
    if let Some(h) = home.filter(|h| !h.trim().is_empty()) {
        let h = norm(h);
        if let Some(rest) = strip_prefix_ci(&p, &h) {
            return join("<home>", rest);
        }
    }

    // 5. Fallback: the literal path. Works on this device; cross-OS portability
    //    isn't possible without a recognisable anchor.
    p
}

/// Map a path *relative to a prefix's `drive_c`* onto a Windows placeholder.
/// e.g. `users/steamuser/AppData/Local/X` → `<winLocalAppData>/X`.
fn drive_c_known_folder(rest: &str) -> Option<String> {
    // users/Public/…  → <winPublic>
    if let Some(tail) = strip_prefix_ci(rest, "users/Public") {
        return Some(join("<winPublic>", tail));
    }
    // ProgramData/…   → <winProgramData>
    if let Some(tail) = strip_prefix_ci(rest, "ProgramData") {
        return Some(join("<winProgramData>", tail));
    }
    // users/<name>/…  → strip the user segment, then match the known folder.
    if let Some(after_users) = strip_prefix_ci(rest, "users") {
        let (_, after_user) = split_first_segment(after_users);
        if let Some(tok) = windows_known_folder(after_user) {
            return Some(tok);
        }
    }
    None
}

/// Map a Windows drive path (`C:/…`) onto a Windows placeholder.
fn windows_drive_known_folder(p: &str) -> Option<String> {
    // C:/Users/Public/…
    if let Some(tail) = strip_prefix_ci(p, "C:/Users/Public") {
        return Some(join("<winPublic>", tail));
    }
    // C:/ProgramData/…
    if let Some(tail) = strip_prefix_ci(p, "C:/ProgramData") {
        return Some(join("<winProgramData>", tail));
    }
    // C:/Users/<name>/…
    if let Some(after_users) = strip_prefix_ci(p, "C:/Users") {
        let (_, after_user) = split_first_segment(after_users);
        if let Some(tok) = windows_known_folder(after_user) {
            return Some(tok);
        }
    }
    None
}

/// Split `a/b/c` into (`a`, `b/c`); ("", "") when empty.
fn split_first_segment(s: &str) -> (&str, &str) {
    match s.split_once('/') {
        Some((head, tail)) => (head, tail),
        None => (s, ""),
    }
}

/// True for a path that starts with a Windows drive letter, e.g. `C:/…`.
fn is_windows_drive_path(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Expand the one placeholder ludusavi can't resolve without a configured root:
/// `<base>` → the game's install folder. Other tokens (the Windows known
/// folders, `<home>`) and literal paths are passed through for ludusavi to
/// resolve. Returns `None` when a `<base>` token has no install folder to expand
/// against (the caller skips it rather than handing ludusavi an unresolved token).
pub fn expand_base(token: &str, game_folder: Option<&str>) -> Option<String> {
    let rest = match token.strip_prefix("<base>") {
        Some(r) => r.trim_start_matches('/'),
        None => return Some(token.to_string()),
    };
    let folder = game_folder.filter(|f| !f.trim().is_empty())?;
    Some(join(&norm(folder), rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PFX: &str = "/home/deck/.local/share/Spool/prefixes/abc";

    // ── Proton prefix → Windows placeholders ────────────────────────────────

    #[test]
    fn prefix_appdata_local() {
        let picked = format!("{PFX}/drive_c/users/steamuser/AppData/Local/MyGame/Saves");
        assert_eq!(
            classify(&picked, Some(PFX), None, None),
            "<winLocalAppData>/MyGame/Saves"
        );
    }

    #[test]
    fn prefix_appdata_locallow_before_local() {
        let picked = format!("{PFX}/drive_c/users/steamuser/AppData/LocalLow/Studio/Game");
        assert_eq!(
            classify(&picked, Some(PFX), None, None),
            "<winLocalAppDataLow>/Studio/Game"
        );
    }

    #[test]
    fn prefix_appdata_roaming() {
        let picked = format!("{PFX}/drive_c/users/steamuser/AppData/Roaming/Game");
        assert_eq!(classify(&picked, Some(PFX), None, None), "<winAppData>/Game");
    }

    #[test]
    fn prefix_documents() {
        let picked = format!("{PFX}/drive_c/users/steamuser/Documents/My Games/Game");
        assert_eq!(
            classify(&picked, Some(PFX), None, None),
            "<winDocuments>/My Games/Game"
        );
    }

    #[test]
    fn prefix_public_and_programdata() {
        let pub_pick = format!("{PFX}/drive_c/users/Public/Documents/Game");
        assert_eq!(
            classify(&pub_pick, Some(PFX), None, None),
            "<winPublic>/Documents/Game"
        );
        let pd_pick = format!("{PFX}/drive_c/ProgramData/Game/save");
        assert_eq!(
            classify(&pd_pick, Some(PFX), None, None),
            "<winProgramData>/Game/save"
        );
    }

    // ── Windows drive paths → placeholders ──────────────────────────────────

    #[test]
    fn windows_appdata_local() {
        assert_eq!(
            classify("C:/Users/Alice/AppData/Local/MyGame/Saves", None, None, None),
            "<winLocalAppData>/MyGame/Saves"
        );
    }

    #[test]
    fn windows_case_insensitive_appdata() {
        // The picker may report the path with different casing.
        assert_eq!(
            classify("C:/Users/Alice/appdata/local/MyGame", None, None, None),
            "<winLocalAppData>/MyGame"
        );
    }

    #[test]
    fn windows_backslashes_normalised() {
        assert_eq!(
            classify(r"C:\Users\Alice\Documents\My Games\X", None, None, None),
            "<winDocuments>/My Games/X"
        );
    }

    #[test]
    fn windows_public_and_programdata() {
        assert_eq!(
            classify("C:/Users/Public/Documents/Game", None, None, None),
            "<winPublic>/Documents/Game"
        );
        assert_eq!(
            classify("C:/ProgramData/Game", None, None, None),
            "<winProgramData>/Game"
        );
    }

    // ── Install-folder relative ─────────────────────────────────────────────

    #[test]
    fn install_dir_relative_base() {
        // A save next to the game on Windows → <base>-relative.
        assert_eq!(
            classify(
                "D:/Games/ULTRAKILL/Saves/Slot1",
                None,
                Some("D:/Games/ULTRAKILL"),
                None,
            ),
            "<base>/Saves/Slot1"
        );
    }

    #[test]
    fn install_dir_wins_over_literal_fallback() {
        // An unrecognised location that happens to be under the install folder
        // is still made portable as <base>/…
        assert_eq!(
            classify("/opt/games/foo/save", None, Some("/opt/games/foo"), None),
            "<base>/save"
        );
    }

    // ── Native Linux home ───────────────────────────────────────────────────

    #[test]
    fn native_linux_home_relative() {
        assert_eq!(
            classify(
                "/home/deck/.local/share/MyGame/save",
                None,
                None,
                Some("/home/deck"),
            ),
            "<home>/.local/share/MyGame/save"
        );
    }

    // ── Literal fallback ────────────────────────────────────────────────────

    #[test]
    fn unrecognised_path_is_literal() {
        assert_eq!(
            classify("/mnt/bulk/saves/game", None, None, Some("/home/deck")),
            "/mnt/bulk/saves/game"
        );
        // Trailing slash trimmed.
        assert_eq!(
            classify("D:/Weird/Place/", None, None, None),
            "D:/Weird/Place"
        );
    }

    #[test]
    fn prefix_non_known_folder_falls_through() {
        // Under the prefix but not a known folder, with no install folder/home →
        // literal (the raw prefix path), which is at least correct on this device.
        let picked = format!("{PFX}/drive_c/Program Files/Game/save");
        assert_eq!(classify(&picked, Some(PFX), None, None), picked);
    }

    // ── expand_base ─────────────────────────────────────────────────────────

    #[test]
    fn expand_base_joins_install_folder() {
        assert_eq!(
            expand_base("<base>/Saves/Slot1", Some("D:/Games/ULTRAKILL")).as_deref(),
            Some("D:/Games/ULTRAKILL/Saves/Slot1")
        );
    }

    #[test]
    fn expand_base_passes_through_other_tokens() {
        assert_eq!(
            expand_base("<winLocalAppData>/X", Some("/whatever")).as_deref(),
            Some("<winLocalAppData>/X")
        );
        assert_eq!(
            expand_base("/literal/path", None).as_deref(),
            Some("/literal/path")
        );
    }

    #[test]
    fn expand_base_without_folder_is_none() {
        assert!(expand_base("<base>/Saves", None).is_none());
        assert!(expand_base("<base>/Saves", Some("   ")).is_none());
    }
}
