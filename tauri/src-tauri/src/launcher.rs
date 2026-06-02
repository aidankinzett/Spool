//! Armoury Crate launcher generation.
//!
//! Builds a standalone `.exe` per game that, when launched, calls
//! `spool.exe --run "<name>" "<exe>"`. The use case is Armoury
//! Crate (ASUS handheld / desktop game launcher) — its Library →
//! Manage Library → Add accepts only `.exe` paths, so we ship a
//! game-specific stub whose only job is to bounce launches through
//! Spool's restore-launch-backup workflow.
//!
//! The mechanism is unchanged from the C# Spool app: a pre-built
//! `launcher_stub.exe` (compiled from `launcher_stub.cs` with
//! `csc.exe`) is embedded via `include_bytes!`; at generation time we
//! write it to `<launchers_dir>/<safe_name>.exe` and append a config
//! payload bracketed by marker strings. The stub reads its own bytes
//! at startup, extracts the markers, and runs Spool. Identical
//! payload format to the C# generator so existing launchers stay
//! compatible — and so anyone with the C# Spool still installed can
//! migrate without regenerating.
//!
//! Note on portability: the generated `.exe` is a Windows PE binary
//! and only runs on Windows. The command itself works on any
//! platform (it just writes bytes to a file); calling it from a
//! Linux or macOS Spool build would produce a useless artifact, but
//! that's the user's problem — no feature gate needed.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::{make_safe_filename, SharedLibrary};
use crate::paths;
use std::io::Write;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

/// The C# Spool launcher stub binary, embedded at compile time.
///
/// Source: `tauri/src-tauri/launcher_stub.cs`, compiled via:
///   `csc.exe /target:winexe /win32icon:launcher_stub.ico /out:launcher_stub.exe launcher_stub.cs`
///
/// Format the stub expects inside this .exe at runtime:
///   `\r\nLUDUSAVI_WRAP_CFG_START\r\n`
///   `<game_name>\r\n`
///   `<game_exe_path>\r\n`
///   `<spool_exe_path>\r\n`
///   `LUDUSAVI_WRAP_CFG_END\r\n`
///
/// The stub also reads `%LOCALAPPDATA%\Spool\config.json` at startup
/// to pick up the latest `spool_exe` path — falling back to the
/// embedded value if config isn't there. So launchers keep working
/// after the user updates Spool to a new location.
const LAUNCHER_STUB: &[u8] = include_bytes!("../launcher_stub.exe");

/// Generates a launcher.exe for the given game. Returns the absolute
/// path on success. Side effects: writes the file, updates the
/// entry's `launcher_exe_path` field, persists the library, emits
/// `library:changed`.
///
/// Errors:
///   - game id not found → `AppError::Other`
///   - filesystem failure (mkdir / write / append) → `AppError::Io`
#[tauri::command]
pub fn generate_armoury_launcher(
    app: AppHandle,
    library: State<'_, SharedLibrary>,
    config: State<'_, SharedConfig>,
    game_id: String,
) -> AppResult<String> {
    // Snapshot inputs before any I/O so we drop the locks fast and
    // don't hold them across filesystem ops.
    let (game_name, exe_path, safe_name) = {
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .entries
            .iter()
            .find(|e| e.id == game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        let safe = if entry.safe_name.is_empty() {
            make_safe_filename(&entry.game_name)
        } else {
            entry.safe_name.clone()
        };
        (entry.game_name.clone(), entry.exe_path.clone(), safe)
    };
    let spool_exe = {
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        cfg.data.spool_exe.clone()
    };

    let launchers_dir = paths::launchers_dir();
    std::fs::create_dir_all(&launchers_dir)?;
    let dest: PathBuf = launchers_dir.join(format!("{safe_name}.exe"));

    // Write the stub bytes first, then append the config payload.
    // The C# stub finds the markers anywhere in the file so it
    // doesn't matter that we're appending past the PE image's
    // logical end — Windows just ignores trailing bytes.
    std::fs::write(&dest, LAUNCHER_STUB)?;
    let payload = format!(
        "\r\nLUDUSAVI_WRAP_CFG_START\r\n{game_name}\r\n{exe_path}\r\n{spool_exe}\r\nLUDUSAVI_WRAP_CFG_END\r\n"
    );
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&dest)
            .map_err(|e| AppError::Other(format!("open append {dest:?}: {e}")))?;
        f.write_all(payload.as_bytes())
            .map_err(|e| AppError::Other(format!("write payload {dest:?}: {e}")))?;
    }

    let path_str = dest.to_string_lossy().to_string();

    // Update the library entry + persist.
    {
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
            entry.launcher_exe_path = Some(path_str.clone());
        }
        lib.save()?;
    }
    if let Err(e) = app.emit("library:changed", &game_id) {
        tracing::warn!(error = %e, "library:changed emit failed after launcher generation");
    }

    Ok(path_str)
}
