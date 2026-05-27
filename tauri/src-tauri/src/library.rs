//! Persistent game library — the heart of the app.
//!
//! The on-disk format intentionally mirrors the existing C# `library.json`
//! produced by the WPF app, field-name for field-name, so an existing user's
//! library loads without migration. All fields use [`serde(default)`] so older
//! libraries missing newer fields still parse cleanly.

use crate::error::{AppError, AppResult};
use crate::paths;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

/// One game in the library. Matches the C# `GameEntry` JSON shape exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GameEntry {
    pub id: String,
    /// Sequential shelf catalog number, formatted as `SPL-NNNN` in the UI.
    /// Assigned at add-time and stable for the entry's lifetime; deleting a
    /// game leaves a gap rather than reusing the number. Zero means "not yet
    /// assigned" — `Library::load` backfills these for legacy entries.
    pub catalog_number: u32,
    pub game_name: String,
    pub exe_path: String,
    pub safe_name: String,

    pub cover_image_path: Option<String>,
    pub hero_image_path: Option<String>,

    pub added_at: Option<DateTime<Utc>>,
    pub last_played_at: Option<DateTime<Utc>>,

    pub launcher_exe_path: Option<String>,
    pub game_folder_path: Option<String>,

    pub run_as_admin: bool,

    // Metadata
    pub description: String,
    pub developer: String,
    pub publisher: String,
    pub genres: Vec<String>,
    pub release_date: Option<DateTime<Utc>>,
    pub install_size_mb: f64,

    // Play tracking
    pub playtime_minutes: i32,

    // LAN sharing
    pub lan_shared: bool,
    pub lan_share_folder: Option<String>,

    // Save backup stats (updated by the run workflow)
    pub save_backup_count: i32,
    pub save_last_backed_up_at: Option<DateTime<Utc>>,
    pub save_backup_size_mb: f64,

    pub install_source: String,
    pub lan_install_source_device_name: Option<String>,
    pub lan_install_source_device_id: Option<String>,
}

impl Default for GameEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            catalog_number: 0,
            game_name: String::new(),
            exe_path: String::new(),
            safe_name: String::new(),
            cover_image_path: None,
            hero_image_path: None,
            added_at: None,
            last_played_at: None,
            launcher_exe_path: None,
            game_folder_path: None,
            run_as_admin: false,
            description: String::new(),
            developer: String::new(),
            publisher: String::new(),
            genres: Vec::new(),
            release_date: None,
            install_size_mb: 0.0,
            playtime_minutes: 0,
            lan_shared: false,
            lan_share_folder: None,
            save_backup_count: 0,
            save_last_backed_up_at: None,
            save_backup_size_mb: 0.0,
            // Matches the C# default — keeps the contract identical for
            // entries that were originally created in the WPF app.
            install_source: "manual".to_string(),
            lan_install_source_device_name: None,
            lan_install_source_device_id: None,
        }
    }
}

/// In-memory library, loaded once at startup and held behind a [`Mutex`] in
/// Tauri state. CRUD methods will grow here as the app surface expands.
#[derive(Debug, Default)]
pub struct Library {
    pub entries: Vec<GameEntry>,
}

impl Library {
    /// Loads from disk. Missing file → empty library; corrupt file → error.
    /// Backfills sequential `catalog_number`s for any entries missing one
    /// (legacy data from before the field existed) and persists if so.
    pub fn load() -> AppResult<Self> {
        let path = paths::library_file();
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(&path)?;
        let entries: Vec<GameEntry> = serde_json::from_str(&json)?;
        let mut lib = Self { entries };
        if lib.backfill_catalog_numbers() {
            // Best-effort save — if it fails, the next mutating op will retry.
            let _ = lib.save();
        }
        Ok(lib)
    }

    /// Returns the next catalog number to assign to a new entry. Numbers are
    /// monotonically increasing; gaps from deletions are preserved.
    #[allow(dead_code)]
    pub fn next_catalog_number(&self) -> u32 {
        self.entries.iter().map(|e| e.catalog_number).max().unwrap_or(0) + 1
    }

    /// Assigns sequential catalog numbers to any entries that don't have one.
    /// Preserves existing assignments. Returns true if any entry was modified.
    fn backfill_catalog_numbers(&mut self) -> bool {
        let mut next = self
            .entries
            .iter()
            .map(|e| e.catalog_number)
            .max()
            .unwrap_or(0);
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.catalog_number == 0 {
                next += 1;
                entry.catalog_number = next;
                changed = true;
            }
        }
        changed
    }

    /// Atomic save: write to a temp file then rename, keeping a `.bak` of the
    /// previous contents. Mirrors the C# implementation's safety guarantees.
    #[allow(dead_code)]
    pub fn save(&self) -> AppResult<()> {
        let path = paths::library_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&tmp, json)?;

        if path.exists() {
            let bak = path.with_extension("json.bak");
            // Best-effort backup; ignore failure here so we don't lose the
            // save just because the .bak couldn't be replaced.
            let _ = std::fs::rename(&path, &bak);
        }
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

/// Shared library state. Wrapping in [`Mutex`] is fine here because every
/// access is a quick read/clone — we never hold the guard across an `.await`.
/// If we ever need to do async work while holding state, switch to
/// `tokio::sync::Mutex`.
pub type SharedLibrary = Mutex<Library>;

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_games(state: State<'_, SharedLibrary>) -> AppResult<Vec<GameEntry>> {
    let lib = state.lock().map_err(|_| AppError::LockPoisoned)?;
    Ok(lib.entries.clone())
}
