//! Crate root for the Spool Tauri backend.
//!
//! Modules are organised by domain concern:
//!   - [`error`]   — unified error type used across the backend
//!   - [`paths`]   — single source of truth for filesystem locations
//!   - [`config`]  — app settings: persistence, identity, ludusavi auto-detect
//!   - [`library`] — the game library: data model, persistence, commands
//!
//! As new features land (ludusavi runner, steamgriddb client, etc.)
//! each gets its own module + Tauri commands registered below.

mod config;
mod error;
mod library;
mod paths;

use config::{Config, SharedConfig};
use library::{Library, SharedLibrary};
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Loading library + config off the main thread would be nicer, but doing
    // it synchronously at startup is fine — both files are small and the
    // first frame doesn't render until the webview boots anyway.
    let library = Library::load().unwrap_or_else(|err| {
        eprintln!("failed to load library, starting empty: {err}");
        Library::default()
    });
    let config = Config::load().unwrap_or_else(|err| {
        eprintln!("failed to load config, starting with defaults: {err}");
        Config::default()
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage::<SharedLibrary>(Mutex::new(library))
        .manage::<SharedConfig>(Mutex::new(config))
        .invoke_handler(tauri::generate_handler![
            library::list_games,
            config::get_config,
            config::update_config,
            config::detect_ludusavi,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
