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
#[tauri::command]
pub async fn open_path(path: String) -> AppResult<()> {
    spawn_opener(&path)
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
    use super::validate_external_url;

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
