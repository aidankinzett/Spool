//! Open a filesystem path or an external URL with the OS's default handler.
//!
//! This replaces the frontend `@tauri-apps/plugin-opener` `openPath` for the
//! "Open folder" actions. The plugin spawns the platform opener (`xdg-open` on
//! Linux) as a child of Spool, **inheriting Spool's environment**. When Spool
//! runs as an AppImage that environment is rewritten to point at the bundled
//! runtime (`LD_LIBRARY_PATH`, `GTK_*`/`GDK_*`, `PYTHONHOME`, …); the host file
//! manager launched by `xdg-open` then inherits those stale bundled libs and
//! fails to start — so "Open folder" silently did nothing on Linux AppImage
//! builds (issue #95).
//!
//! Routing through Rust lets us reuse `process::strip_appimage_env`, the same
//! sanitisation game launches already rely on, so the spawned file manager
//! sees the host environment.
//!
//! `open_url` exists for the same reason plus one of its own: WebKitGTK (the
//! Linux webview) has no default handler for a link that asks for a new window,
//! so an `<a target="_blank">` click ends there and no browser opens (issue
//! #493). The frontend intercepts those clicks and calls this command instead.

use crate::error::{AppError, AppResult};
use url::Url;

/// Open `path` (a file or directory) with the OS default handler.
///
/// Like [`open_url`], the argument comes from the webview and reaches an
/// opener that dispatches on scheme, so it must be an absolute filesystem path:
/// `steam://run/440` or `file:///…` would otherwise launch whatever handles
/// that scheme.
#[tauri::command]
pub async fn open_path(path: String) -> AppResult<()> {
    spawn_opener(&validate_open_path(&path, cfg!(windows))?)
}

/// Check `raw` is an absolute filesystem path for the host OS and return it
/// unchanged for the opener.
///
/// `windows` selects the rule set (the caller passes `cfg!(windows)`); it is a
/// parameter rather than a `cfg` so both rule sets are tested on every host.
/// Unix paths must start with `/`. Windows paths must start with a drive letter
/// and separator (`C:\`, `C:/`) or be UNC (`\\server\share`, `\\?\C:\…`,
/// `//server/share`). A string with a URL scheme matches neither, and since an
/// absolute path never starts with `-` it cannot be read as an option either.
fn validate_open_path(raw: &str, windows: bool) -> AppResult<String> {
    if raw.trim().is_empty() {
        return Err(AppError::Other("no path to open".into()));
    }
    if raw.contains('\0') {
        return Err(AppError::Other("path contains a NUL byte".into()));
    }
    let absolute = if windows {
        let b = raw.as_bytes();
        let drive = b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'\\' | b'/');
        drive || raw.starts_with("\\\\") || raw.starts_with("//")
    } else {
        raw.starts_with('/')
    };
    if absolute {
        Ok(raw.to_string())
    } else {
        Err(AppError::Other(format!(
            "refusing to open a path that isn't absolute: {raw}"
        )))
    }
}

/// Open an external `url` in the user's default browser.
///
/// The argument comes from the webview, so it is validated before it reaches
/// the opener: `xdg-open` dispatches on scheme, and every desktop registers
/// handlers beyond `http` (`file:`, `mailto:`, and whatever other apps claim),
/// so an unchecked string is a request to launch an arbitrary application.
#[tauri::command]
pub async fn open_url(url: String) -> AppResult<()> {
    spawn_opener(&validate_external_url(&url)?)
}

/// Check `raw` is an ordinary web URL and return the normalised form to hand
/// the opener.
///
/// Accepts `http` and `https` only, and requires a host — `http:///x` or a bare
/// scheme names no site. A leading `-` is rejected up front so the string can
/// never be read as an option by the opener binary; the value actually spawned
/// is `Url`'s serialisation, which always starts with the scheme, so the guard
/// is about refusing suspicious input rather than the spawn being unsafe.
fn validate_external_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('-') {
        return Err(AppError::Other(format!(
            "refusing to open URL that could be read as a flag: {trimmed}"
        )));
    }
    let parsed = Url::parse(trimmed)
        .map_err(|e| AppError::Other(format!("not a valid URL: {trimmed} ({e})")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::Other(format!(
            "refusing to open non-web URL scheme: {}",
            parsed.scheme()
        )));
    }
    match parsed.host_str() {
        Some(host) if !host.is_empty() => Ok(parsed.to_string()),
        _ => Err(AppError::Other(format!("URL has no host: {trimmed}"))),
    }
}

#[cfg(target_os = "linux")]
fn spawn_opener(target: &str) -> AppResult<()> {
    use tokio::process::Command;

    let mut cmd = Command::new("xdg-open");
    cmd.arg(target);
    // Hand the host file manager the host environment, not Spool's AppImage
    // bundle — see the module note and `process::strip_appimage_env`.
    crate::process::strip_appimage_env(&mut cmd);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("failed to open: {e}")))
}

#[cfg(target_os = "macos")]
fn spawn_opener(target: &str) -> AppResult<()> {
    use tokio::process::Command;

    Command::new("open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("failed to open: {e}")))
}

#[cfg(target_os = "windows")]
fn spawn_opener(target: &str) -> AppResult<()> {
    use tokio::process::Command;

    // explorer.exe opens a folder (or selects/launches a file) with the shell
    // default — matching the previous opener-plugin behaviour on Windows — and
    // hands an http(s) URL to the default browser.
    Command::new("explorer")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("failed to open: {e}")))
}

#[cfg(test)]
mod tests {
    use super::{validate_external_url, validate_open_path};

    #[test]
    fn open_path_accepts_unix_absolute_paths() {
        for p in [
            "/home/me/Games/Hades",
            "/",
            "/tmp/with space/x",
            "//double/slash",
        ] {
            assert_eq!(validate_open_path(p, false).unwrap(), p);
        }
    }

    #[test]
    fn open_path_accepts_windows_drive_and_unc_paths() {
        for p in [
            r"C:\Games\Hades",
            "D:/Games/Hades",
            r"c:\lower",
            r"\\server\share\Games",
            r"\\?\C:\Very\Long\Path",
            "//server/share/Games",
        ] {
            assert_eq!(validate_open_path(p, true).unwrap(), p);
        }
    }

    #[test]
    fn open_path_rejects_urls_on_both_rule_sets() {
        // The point of the check: a scheme-bearing string must never reach the
        // opener through the path command.
        for p in [
            "steam://run/440",
            "file:///etc/passwd",
            "mailto:someone@example.com",
            "https://example.com",
            "vscode://file/etc/passwd",
            "javascript:alert(1)",
        ] {
            assert!(validate_open_path(p, false).is_err(), "unix accepted {p}");
            assert!(validate_open_path(p, true).is_err(), "windows accepted {p}");
        }
    }

    #[test]
    fn open_path_rejects_relative_and_malformed_input() {
        for p in [
            "",
            "   ",
            "relative/path",
            "./x",
            "../x",
            "~/Games",
            "-version",
            "--help",
            "a\0b",
        ] {
            assert!(validate_open_path(p, false).is_err(), "unix accepted {p:?}");
            assert!(
                validate_open_path(p, true).is_err(),
                "windows accepted {p:?}"
            );
        }
    }

    #[test]
    fn open_path_rules_are_per_platform() {
        // A drive path is not absolute on Unix (it would parse as a `c:` scheme
        // for xdg-open), and a Unix path is not absolute on Windows.
        assert!(validate_open_path(r"C:\Games", false).is_err());
        assert!(validate_open_path("/home/me", true).is_err());
        // Drive-relative and rooted-without-drive Windows forms are not absolute.
        assert!(validate_open_path("C:Games", true).is_err());
        assert!(validate_open_path(r"\Games", true).is_err());
    }

    #[test]
    fn accepts_http_and_https() {
        for url in [
            "https://www.steamgriddb.com/profile/preferences/api",
            "http://192.168.1.10:47632/games",
            "https://spool.kinzett.io/guides/sftp-remote/",
            "https://github.com/aidankinzett/Spool/issues/new?title=a%20b&body=c",
        ] {
            assert_eq!(validate_external_url(url).unwrap(), url);
        }
    }

    #[test]
    fn scheme_and_host_are_case_normalised() {
        assert_eq!(
            validate_external_url("HTTPS://Example.COM/Path").unwrap(),
            "https://example.com/Path"
        );
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        assert_eq!(
            validate_external_url("  https://example.com/  ").unwrap(),
            "https://example.com/"
        );
    }

    #[test]
    fn rejects_non_web_schemes() {
        // Anything the desktop might have a handler registered for: local files,
        // script URLs the webview itself would evaluate, and schemes claimed by
        // other installed applications.
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "steam://run/440",
            "mailto:someone@example.com",
            "ftp://example.com/x",
            "vscode://file/etc/passwd",
        ] {
            assert!(validate_external_url(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn rejects_scheme_smuggled_past_a_prefix_check() {
        // A string that merely *contains* "https://" is not a web URL — the
        // check is on the parsed scheme, not on the text.
        assert!(validate_external_url("javascript:location='https://x.test'").is_err());
        assert!(validate_external_url("file:///tmp/x?u=https://x.test").is_err());
    }

    #[test]
    fn rejects_leading_dash_so_it_cannot_be_read_as_a_flag() {
        assert!(validate_external_url("-version").is_err());
        assert!(validate_external_url("--help").is_err());
        assert!(validate_external_url("  -https://example.com").is_err());
    }

    #[test]
    fn rejects_urls_without_a_host() {
        for url in ["http://", "http://?q=1", "https://#frag", "http:///"] {
            assert!(validate_external_url(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn rejects_unparseable_and_relative_input() {
        for url in ["", "   ", "example.com", "/settings", "not a url"] {
            assert!(validate_external_url(url).is_err(), "accepted {url:?}");
        }
    }
}
