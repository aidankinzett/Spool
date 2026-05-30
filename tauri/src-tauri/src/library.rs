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
use tauri::{AppHandle, Emitter, State};

/// One game in the library. Matches the C# `GameEntry` JSON shape exactly
/// for the legacy fields, plus a small set of manifest-derived metadata
/// new to the Tauri rewrite (steam id, gog id, save paths, …).
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

    // ── Proton / Linux launch (inert on Windows) ────────────────────────────
    /// Legacy on/off toggle for launching through Proton. **No longer read** —
    /// Proton is now used automatically for Windows `.exe` games on Linux (see
    /// [`GameEntry::uses_proton`] / issue #80). Kept only so existing
    /// `library.json` files round-trip unchanged; nothing sets or consults it.
    pub use_proton: bool,
    /// Override the Proton build directory. `None` = use the global default
    /// (`ConfigData.default_proton_path`) or auto-pick the newest.
    pub proton_version_path: Option<String>,
    /// Override the Wine prefix ROOT. `None` = `paths::proton_prefixes_dir()/<id>`.
    pub wine_prefix_path: Option<String>,
    /// Extra command-line args appended after the exe (space-split at use).
    pub launch_args: Option<String>,

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

    // ── Manifest-derived metadata (new in Tauri rewrite) ────────────────────
    //
    // Snapshot of the ludusavi manifest entry that matched this game at
    // add-time. Stays stable for the entry's lifetime; the user can re-run
    // identification to refresh it. Empty/None when the user added a game
    // without save tracking (no ludusavi match).
    pub steam_id: Option<u64>,
    pub gog_id: Option<u64>,
    pub lutris_slug: Option<String>,
    /// The folder name ludusavi expects, e.g. `"Hades"`. Useful for hinting
    /// at the install dir when the user picks an exe.
    pub manifest_install_dir: Option<String>,
    /// Save path templates from the manifest, in display form (e.g.
    /// `%APPDATA%/Hades`). First entry is the canonical / primary location.
    pub save_paths: Vec<String>,
    /// Dominant cover-art colour as `#rrggbb`, extracted when the cover
    /// downloads. Drives hero / button / accent tinting in the detail
    /// view; falls back to the brand `spool` colour when None.
    pub accent_color: Option<String>,

    /// Cross-device save-sync status, set by the sync server's
    /// `/events/:game/latest-backup` probe. One of:
    ///
    ///   "synced"        latest server event came from this device
    ///                   and our local mtime matches
    ///   "local-newer"   we backed up more recently than the server
    ///                   knows (offline backup or sync was disabled)
    ///   "cloud-newer"   another device backed up after us — our
    ///                   local saves are behind
    ///
    /// `None` means we don't have enough info to badge (sync off,
    /// no backup history, or never queried). The library sidebar
    /// renders a small coloured dot on the cover when this is set.
    pub sync_badge: Option<String>,

    /// Latest ludusavi backup name (the `mapping.yaml` tip) that was last
    /// reconciled with the cloud on THIS device. Acts as the merge-base for
    /// fast-forward vs. true-divergence detection: when ludusavi reports a
    /// cloud conflict, comparing this baseline against the local and cloud
    /// tips tells us whether one side is cleanly ahead (auto-resolve) or both
    /// changed (real conflict — prompt the user). `None` for legacy entries /
    /// games never synced — the workflow falls back to a timestamp heuristic
    /// until the first sync records a baseline.
    pub cloud_sync_baseline: Option<String>,
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
            use_proton: false,
            proton_version_path: None,
            wine_prefix_path: None,
            launch_args: None,
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
            install_source: "manual".to_string(),
            lan_install_source_device_name: None,
            lan_install_source_device_id: None,
            steam_id: None,
            gog_id: None,
            lutris_slug: None,
            manifest_install_dir: None,
            save_paths: Vec::new(),
            accent_color: None,
            sync_badge: None,
            cloud_sync_baseline: None,
        }
    }
}

impl GameEntry {
    /// Whether this entry launches through Proton. Derived from the platform +
    /// executable type rather than a stored flag — see
    /// [`crate::proton::exe_needs_proton`] (issue #80).
    pub fn uses_proton(&self) -> bool {
        crate::proton::exe_needs_proton(&self.exe_path)
    }
}

/// Payload accepted by the `add_game` command. The frontend constructs this
/// from a picked `SearchCandidate` plus the user-chosen exe path. Empty
/// ludusavi-derived fields represent the "add without save tracking" path.
#[derive(Debug, Deserialize)]
pub struct NewGame {
    pub game_name: String,
    pub exe_path: String,
    #[serde(default)]
    pub steam_id: Option<u64>,
    #[serde(default)]
    pub gog_id: Option<u64>,
    #[serde(default)]
    pub lutris_slug: Option<String>,
    #[serde(default)]
    pub manifest_install_dir: Option<String>,
    #[serde(default)]
    pub save_paths: Vec<String>,
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
            let _ = lib.save();
        }
        Ok(lib)
    }

    /// Returns the next catalog number to assign to a new entry. Numbers are
    /// monotonically increasing; gaps from deletions are preserved.
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
            let _ = std::fs::rename(&path, &bak);
        }
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Locates an entry by id without removing it.
    pub fn find(&self, id: &str) -> Option<&GameEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Shared library state. Wrapping in [`Mutex`] is fine here because every
/// access is a quick read/clone — we never hold the guard across an `.await`.
pub type SharedLibrary = Mutex<Library>;

/// Filesystem-safe filename derived from a game name.
///
/// Invalid path characters are replaced with spaces (not stripped) so word
/// boundaries are preserved — `"A: B/C"` becomes `"A B C"` rather than
/// `"A BC"`. Non-ASCII characters are dropped to avoid codepage issues in
/// legacy non-Unicode tools (some launchers / Inno Setup scripts).
/// Whitespace runs are then collapsed.
pub fn make_safe_filename(name: &str) -> String {
    const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    // Pass 1: ASCII-only, invalid-or-control chars become spaces.
    let stage: String = name
        .chars()
        .filter(|c| c.is_ascii())
        .map(|c| {
            if INVALID.contains(&c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();

    // Pass 2: collapse runs of whitespace to a single space.
    let mut collapsed = String::with_capacity(stage.len());
    let mut last_space = false;
    for c in stage.chars() {
        if c.is_whitespace() {
            if !last_space {
                collapsed.push(' ');
                last_space = true;
            }
        } else {
            collapsed.push(c);
            last_space = false;
        }
    }
    let trimmed = collapsed.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "Game".to_string()
    } else {
        trimmed.to_string()
    }
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_games(state: State<'_, SharedLibrary>) -> AppResult<Vec<GameEntry>> {
    let lib = state.lock().map_err(|_| AppError::LockPoisoned)?;
    Ok(lib.entries.clone())
}

/// Adds a new game. Assigns id/catalog/timestamps server-side; persists
/// atomically; emits `library.changed` so any open windows can refresh.
#[tauri::command]
pub fn add_game(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    new_game: NewGame,
) -> AppResult<GameEntry> {
    let entry = {
        let mut lib = state.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = GameEntry {
            id: uuid::Uuid::new_v4().to_string(),
            catalog_number: lib.next_catalog_number(),
            game_name: new_game.game_name.clone(),
            exe_path: new_game.exe_path,
            safe_name: make_safe_filename(&new_game.game_name),
            added_at: Some(Utc::now()),
            steam_id: new_game.steam_id,
            gog_id: new_game.gog_id,
            lutris_slug: new_game.lutris_slug,
            manifest_install_dir: new_game.manifest_install_dir,
            save_paths: new_game.save_paths,
            ..GameEntry::default()
        };
        lib.entries.push(entry.clone());
        lib.save()?;
        entry
    };
    if let Err(e) = app.emit("library:changed", &entry.id) {
        tracing::warn!(error = %e, "failed to emit library:changed after add_game");
    }

    // Kick off an async cover-art fetch. Non-blocking — the user sees the
    // new card immediately with the synthetic sleeve fallback, and the
    // real cover lands a moment later via a second library:changed emit.
    let app_for_task = app.clone();
    let id_for_task = entry.id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::steamgriddb::fetch_and_save_cover(&app_for_task, &id_for_task).await
        {
            tracing::warn!(game_id = %id_for_task, error = %e, "cover fetch failed");
        }
    });

    Ok(entry)
}

/// Replaces an entry by id with the provided value. The id field on
/// `entry` is the lookup key; mismatches between in-memory state and
/// disk are resolved by overwriting.
#[tauri::command]
pub fn update_game(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    entry: GameEntry,
) -> AppResult<GameEntry> {
    let updated = {
        let mut lib = state.lock().map_err(|_| AppError::LockPoisoned)?;
        let idx = lib
            .entries
            .iter()
            .position(|e| e.id == entry.id)
            .ok_or_else(|| AppError::Other(format!("game with id {} not found", entry.id)))?;
        lib.entries[idx] = entry.clone();
        lib.save()?;
        entry
    };
    if let Err(e) = app.emit("library:changed", &updated.id) {
        tracing::warn!(error = %e, "failed to emit library:changed after update_game");
    }
    Ok(updated)
}

/// Removes an entry by id. No-op if the id isn't present (returns false).
/// Emits `library.changed` when something was actually removed.
#[tauri::command]
pub fn remove_game(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    id: String,
) -> AppResult<bool> {
    let removed = {
        let mut lib = state.lock().map_err(|_| AppError::LockPoisoned)?;
        let before = lib.entries.len();
        lib.entries.retain(|e| e.id != id);
        if lib.entries.len() < before {
            lib.save()?;
            true
        } else {
            false
        }
    };
    if removed {
        if let Err(e) = app.emit("library:changed", &id) {
            tracing::warn!(error = %e, "failed to emit library:changed after remove_game");
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_safe_filename_handles_basics() {
        assert_eq!(make_safe_filename("Hades II"), "Hades II");
        assert_eq!(make_safe_filename("Game: Bad/Chars?"), "Game Bad Chars");
        assert_eq!(make_safe_filename("  many   spaces  "), "many spaces");
        assert_eq!(make_safe_filename(""), "Game");
        assert_eq!(make_safe_filename("...."), "Game");
    }

    #[test]
    fn make_safe_filename_strips_non_ascii() {
        // Non-ASCII characters get dropped — same behaviour as the C#
        // version, intended to keep legacy non-Unicode tools happy.
        assert_eq!(make_safe_filename("Tëst Gämé"), "Tst Gm");
    }
}
