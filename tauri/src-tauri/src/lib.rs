//! Crate root for the Spool Tauri backend.
//!
//! Modules are organised by domain concern:
//!   - [`error`]   — unified error type used across the backend
//!   - [`paths`]   — single source of truth for filesystem locations
//!   - [`library`] — the game library: data model, persistence, commands
//!
//! As new features land (config, ludusavi runner, steamgriddb client, etc.)
//! each gets its own module + Tauri commands registered below.

mod error;
mod library;
mod paths;

use library::{Library, SharedLibrary};
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Loading the library off the main thread would be nicer, but doing it
    // synchronously at startup is fine — the file is small and the first
    // frame doesn't render until the webview boots anyway.
    let library = Library::load().unwrap_or_else(|err| {
        eprintln!("failed to load library, starting empty: {err}");
        Library::default()
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage::<SharedLibrary>(Mutex::new(library))
        .invoke_handler(tauri::generate_handler![library::list_games])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
