//! Small cross-module helpers shared by the subprocess callers.
//!
//! Several modules shell out to the bundled sidecars (`ludusavi`, `rclone`)
//! and parse their `--api` / `lsjson` JSON. Two patterns recurred at every
//! call site: configuring a child process to capture its output without
//! flashing a console window, and turning a JSON parse failure into an
//! [`AppError`] with a human-readable context. They live here so the wording
//! and the Windows console-window flag stay consistent everywhere.

use crate::error::{AppError, AppResult};
use serde::de::DeserializeOwned;

/// Configure a `Command` (either `std::process::Command` or
/// `tokio::process::Command`) to capture stdout/stderr and read nothing from
/// stdin, suppressing the console window on Windows.
///
/// Expands to the same tokens that were previously copied at each subprocess
/// site: stdin/stdout/stderr redirection plus `CREATE_NO_WINDOW`
/// (`0x0800_0000`, winbase.h) so a bundled CLI doesn't flash a console. A
/// macro rather than a function because the two `Command` types don't share a
/// single trait covering both the stdio setters and `creation_flags`.
#[macro_export]
macro_rules! capture_stdio {
    ($cmd:expr) => {{
        $cmd.stdin(std::process::Stdio::null());
        $cmd.stdout(std::process::Stdio::piped());
        $cmd.stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            #[allow(unused_imports)]
            use std::os::windows::process::CommandExt;
            $cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
    }};
}

/// Deserialize `bytes` as JSON, mapping a parse failure to
/// `AppError::Other("failed to parse <context>: …")`.
///
/// `context` names what was being parsed (e.g. `"ludusavi find output"`) so
/// the surfaced error points at the offending subprocess.
pub fn parse_json<T: DeserializeOwned>(bytes: &[u8], context: &str) -> AppResult<T> {
    serde_json::from_slice(bytes)
        .map_err(|e| AppError::Other(format!("failed to parse {context}: {e}")))
}
