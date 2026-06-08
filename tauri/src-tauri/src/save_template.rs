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
//! | anything else under the user profile (e.g. `…/users/<u>/Saved Games/X`) | `<home>/Saved Games/X` |
//! | under the game's install folder | `<base>/X` (expanded per device) |
//! | under the Linux home dir (native, non-Proton game) | `<home>/X` |
//! | anything else | the literal path (works locally; may not cross OSes) |
//!
//! `<home>` is portable in both directions: ludusavi's `parse_paths` maps it to
//! the prefix's `drive_c/users/steamuser` under `--wine-prefix` and to the real
//! profile (`C:/Users/<u>`) on Windows — so a "Saved Games" pick round-trips.

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
    let xl = prefix.len();
    // `is_char_boundary` is false when `xl > path.len()` OR `xl` lands inside a
    // multibyte UTF-8 char, so it guards `path[..xl]` (and the later `path[xl..]`)
    // from panicking on a non-ASCII folder name — e.g. a Cyrillic/CJK save folder
    // picked under the prefix, where byte offset `xl` falls mid-character.
    if !path.is_char_boundary(xl) || !path[..xl].eq_ignore_ascii_case(prefix) {
        return None;
    }
    match path[xl..].strip_prefix('/') {
        Some(rest) => Some(rest),
        None if path.len() == xl => Some(""),
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

    // 1. Inside the game's Proton/Wine prefix → a portable token. ludusavi maps
    //    <home> / <winXxx> back into the prefix's drive_c under --wine-prefix, so
    //    any folder under the user profile round-trips. A folder under the prefix
    //    that isn't a recognised location stays literal (device-local) — never
    //    mapped via the real-home <home> in step 4.
    if let Some(root) = prefix_root {
        let drive_c = format!("{}/drive_c", norm(root));
        if let Some(rest) = strip_prefix_ci(&p, &drive_c) {
            if let Some(tok) = drive_c_known_folder(rest) {
                return tok;
            }
            // Under the prefix but not a recognised known folder — fall through
            // rather than returning the literal here, so a game *installed inside
            // the prefix* whose save sits under its install folder still becomes
            // the portable `<base>/…` (step 3). Step 4 (real home) is skipped for
            // Proton games, so a genuinely unanchored prefix path lands as a
            // literal at step 5 just as before.
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

    // 4. Under the Linux home dir → `<home>/rest`. Only for non-Proton games:
    //    under --wine-prefix ludusavi resolves <home> into the prefix, so a
    //    real-home path for a Proton game must stay literal, not become <home>.
    if prefix_root.is_none() {
        if let Some(h) = home.filter(|h| !h.trim().is_empty()) {
            let h = norm(h);
            if let Some(rest) = strip_prefix_ci(&p, &h) {
                return join("<home>", rest);
            }
        }
    }

    // 5. Fallback: the literal path. Works on this device; cross-OS portability
    //    isn't possible without a recognisable anchor.
    p
}

/// Map a path *relative to a prefix's `drive_c`* onto a portable token.
/// e.g. `users/steamuser/AppData/Local/X` → `<winLocalAppData>/X`,
/// `users/steamuser/Saved Games/X` → `<home>/Saved Games/X`. `None` for
/// locations outside the user profile / ProgramData (e.g. `Program Files`).
fn drive_c_known_folder(rest: &str) -> Option<String> {
    // ProgramData/…   → <winProgramData> (outside the user profile)
    if let Some(tail) = strip_prefix_ci(rest, "ProgramData") {
        return Some(join("<winProgramData>", tail));
    }
    profile_token(strip_prefix_ci(rest, "users"))
}

/// Map a Windows drive path (`C:/…`) onto a portable token.
fn windows_drive_known_folder(p: &str) -> Option<String> {
    // C:/ProgramData/…
    if let Some(tail) = strip_prefix_ci(p, "C:/ProgramData") {
        return Some(join("<winProgramData>", tail));
    }
    profile_token(strip_prefix_ci(p, "C:/Users"))
}

/// Shared user-profile classifier for both the prefix and Windows-drive paths.
/// `after_users` is the part after `users/` (or `C:/Users/`): `<name>/<rest>`.
///   * `Public/…`              → `<winPublic>/…`
///   * a Windows known folder  → `<winLocalAppData>` / `<winDocuments>` / …
///   * anything else (Saved Games, game-specific dirs, the profile root) →
///     `<home>/…`, which ludusavi resolves into the prefix on Linux and to the
///     real profile on Windows.
///
/// `None` when there's no user segment at all (e.g. `…/users` itself).
fn profile_token(after_users: Option<&str>) -> Option<String> {
    let after_users = after_users?;
    if after_users.is_empty() {
        return None; // `…/users` with no profile under it — no portable anchor.
    }
    let (user, after_user) = split_first_segment(after_users);
    if user.eq_ignore_ascii_case("Public") {
        return Some(join("<winPublic>", after_user));
    }
    if let Some(tok) = windows_known_folder(after_user) {
        return Some(tok);
    }
    // The profile root itself, or a folder we don't have a <winXxx> token for.
    Some(join("<home>", after_user))
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
    let rest = if token == "<base>" {
        ""
    } else if let Some(r) = token.strip_prefix("<base>/") {
        r
    } else {
        return Some(token.to_string());
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

    #[test]
    fn prefix_saved_games_uses_home() {
        // "Saved Games" has no <winXxx> token, but <home> resolves into the
        // prefix under --wine-prefix, so it's still portable.
        let picked = format!("{PFX}/drive_c/users/steamuser/Saved Games/MyGame");
        assert_eq!(
            classify(&picked, Some(PFX), None, None),
            "<home>/Saved Games/MyGame"
        );
    }

    #[test]
    fn prefix_game_specific_profile_dir_uses_home() {
        // A folder dropped straight under the user profile.
        let picked = format!("{PFX}/drive_c/users/steamuser/MyGameSaves");
        assert_eq!(classify(&picked, Some(PFX), None, None), "<home>/MyGameSaves");
    }

    #[test]
    fn prefix_profile_root_is_home() {
        let picked = format!("{PFX}/drive_c/users/steamuser");
        assert_eq!(classify(&picked, Some(PFX), None, None), "<home>");
    }

    #[test]
    fn prefix_users_root_without_profile_is_literal() {
        // `.../users` with no profile under it has no portable anchor.
        let picked = format!("{PFX}/drive_c/users");
        assert_eq!(classify(&picked, Some(PFX), None, None), picked);
    }

    #[test]
    fn windows_saved_games_uses_home() {
        assert_eq!(
            classify("C:/Users/Alice/Saved Games/MyGame", None, None, None),
            "<home>/Saved Games/MyGame"
        );
    }

    #[test]
    fn proton_real_home_path_stays_literal() {
        // A Proton game writing to the real Linux home (not the prefix) must stay
        // literal — <home> would be misread as the prefix under --wine-prefix.
        let picked = "/home/deck/.config/MyGame";
        assert_eq!(classify(picked, Some(PFX), None, Some("/home/deck")), picked);
    }

    // ── Non-ASCII folder names must not panic (strip_prefix_ci boundary) ─────

    #[test]
    fn non_ascii_folder_under_drive_c_does_not_panic() {
        // A Cyrillic folder directly under drive_c: byte offset 5 ("users".len())
        // lands mid-character. Must return (literal here), not panic.
        let picked = format!("{PFX}/drive_c/Игрок/save");
        assert_eq!(classify(&picked, Some(PFX), None, None), picked);
    }

    #[test]
    fn non_ascii_profile_subfolder_maps_to_home() {
        // A Cyrillic folder under the user profile → <home>/… (and no panic).
        let picked = format!("{PFX}/drive_c/users/steamuser/Игры/Slot1");
        assert_eq!(classify(&picked, Some(PFX), None, None), "<home>/Игры/Slot1");
    }

    #[test]
    fn strip_prefix_ci_non_boundary_returns_none() {
        // Direct guard check: prefix length landing mid-multibyte-char → None.
        assert_eq!(strip_prefix_ci("Игрок", "users"), None);
        assert_eq!(strip_prefix_ci("café/x", "café"), Some("x"));
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

    #[test]
    fn install_dir_inside_prefix_uses_base() {
        // A Windows game installed *into* the prefix (game_folder under drive_c):
        // a save under the install folder must still become the portable <base>/…
        // rather than a device-local literal embedding prefixes/<id>.
        let game_folder = format!("{PFX}/drive_c/Program Files/ULTRAKILL");
        let picked = format!("{game_folder}/Saves/Slot1");
        assert_eq!(
            classify(&picked, Some(PFX), Some(&game_folder), None),
            "<base>/Saves/Slot1"
        );
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
        assert_eq!(
            expand_base("<base-extra>/X", Some("/whatever")).as_deref(),
            Some("<base-extra>/X")
        );
        assert_eq!(
            expand_base("<base>", Some("D:/Games/ULTRAKILL")).as_deref(),
            Some("D:/Games/ULTRAKILL")
        );
    }

    #[test]
    fn expand_base_without_folder_is_none() {
        assert!(expand_base("<base>/Saves", None).is_none());
        assert!(expand_base("<base>/Saves", Some("   ")).is_none());
    }
}
