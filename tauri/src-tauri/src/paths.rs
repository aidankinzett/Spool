//! Centralised filesystem path resolution.
//!
//! Every module that needs to read or write an app file goes through here, so
//! there's one place to change if the layout ever moves. The directory layout
//! mirrors the existing C# Spool app so an existing user's library and config
//! are picked up automatically on first launch.

use std::path::PathBuf;

/// Root data directory: `%LOCALAPPDATA%\Spool` on Windows,
/// `~/.local/share/Spool` on Linux, `~/Library/Application Support/Spool` on macOS.
pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .expect("local data dir must be resolvable")
        .join("Spool")
}

pub fn library_file() -> PathBuf {
    app_data_dir().join("library.json")
}

#[allow(dead_code)]
pub fn config_file() -> PathBuf {
    app_data_dir().join("config.json")
}

#[allow(dead_code)]
pub fn covers_dir() -> PathBuf {
    app_data_dir().join("covers")
}

#[allow(dead_code)]
pub fn launchers_dir() -> PathBuf {
    app_data_dir().join("launchers")
}
