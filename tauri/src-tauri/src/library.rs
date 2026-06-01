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
use std::sync::{Arc, Mutex};
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
    /// (`ConfigData.launch.default_proton_path`) or auto-pick the newest.
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

    /// Cross-device save-sync status, derived from the rclone device blobs
    /// at startup and updated after each backup. One of:
    ///
    ///   "synced"        this device holds the most recent backup
    ///   "local-newer"   we backed up locally but the cloud hasn't
    ///                   confirmed it yet (offline / sync disabled)
    ///   "cloud-newer"   another device backed up after us — our
    ///                   local saves are behind
    ///
    /// `None` means not enough info to badge (cloud not configured,
    /// no backup history). The library sidebar renders a small
    /// coloured dot on the cover when this is set.
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

/// Shared library state. The outer [`Arc`] lets callers clone a handle into
/// spawned tasks without touching Tauri's `State<'_, _>` lifetime — in
/// particular `lan/install.rs`'s download task and the headless plugin
/// server both need to push a new entry after the partial rename.
pub type SharedLibrary = Arc<Mutex<Library>>;

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

    // Fetch Steam Store metadata (description, developer, publisher,
    // genres, release date) in parallel. Best-effort and only fills
    // empty fields — a no-op when the game has no steam_id.
    let app_for_meta = app.clone();
    let id_for_meta = entry.id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::metadata::fetch_and_save_metadata(&app_for_meta, &id_for_meta).await {
            tracing::warn!(game_id = %id_for_meta, error = %e, "metadata fetch failed");
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

/// Deletes a game's install folder from disk, then removes its library entry.
///
/// Unlike [`remove_game`] (which only forgets the entry), this reclaims the
/// disk space by deleting the folder recorded in `game_folder_path`. The
/// folder is removed first; only if that succeeds is the library entry
/// dropped, so a failed delete leaves the library pointing at a folder that
/// still exists rather than orphaning files the UI can no longer find.
///
/// On Linux it also deletes the game's per-game Proton/Wine prefix (the
/// `wine_prefix_path` override, or `prefixes/<id>` under Spool's data dir) as
/// a best-effort step — a failed or missing prefix delete doesn't abort the
/// operation. On Windows that path doesn't exist, so it's a no-op.
///
/// Refuses to run when the game has no known install folder, and rejects
/// obviously-too-broad targets (filesystem root, the user's home dir, Spool's
/// own data dir, or any path fewer than two components deep) so a bad
/// `game_folder_path` can't wipe out unrelated files. A folder that's already
/// gone is treated as success.
#[tauri::command]
pub async fn delete_game_from_disk(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    id: String,
) -> AppResult<()> {
    delete_game_core(state.inner(), &id).await?;
    if let Err(e) = app.emit("library:changed", &id) {
        tracing::warn!(error = %e, "failed to emit library:changed after delete_game_from_disk");
    }
    Ok(())
}

/// Folder-delete + entry-removal shared by the [`delete_game_from_disk`]
/// command and the Decky plugin server's `DELETE /games/:id`. Does not emit
/// `library:changed` — the caller does that where a Tauri `AppHandle` exists.
pub async fn delete_game_core(library: &SharedLibrary, id: &str) -> AppResult<()> {
    // Snapshot the folder + prefix paths under the lock, then drop the guard
    // before any blocking IO or await (lock discipline: never hold a std Mutex
    // across await).
    let (folder, prefix_root) = {
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| AppError::Other(format!("game with id {id} not found")))?;
        // Per-game Proton prefix: the override if set, else the default
        // `prefixes/<id>` under Spool's data dir.
        let prefix_root = entry
            .wine_prefix_path
            .clone()
            .filter(|p| !p.trim().is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| crate::proton::game_prefix_path(id));
        (entry.game_folder_path.clone(), prefix_root)
    };

    let Some(folder) = folder.filter(|f| !f.trim().is_empty()) else {
        return Err(AppError::Other(
            "This game has no known install folder to delete.".to_string(),
        ));
    };

    // Recursive delete can be slow for a large game — run it off the async
    // runtime's worker threads.
    tokio::task::spawn_blocking(move || delete_install_dir(&folder))
        .await
        .map_err(|e| AppError::Other(format!("delete task join failed: {e}")))??;

    // Best-effort Proton prefix cleanup — never aborts the removal. A missing
    // prefix (e.g. a never-launched game, or Windows) is a no-op.
    let prefix_str = prefix_root.to_string_lossy().to_string();
    match tokio::task::spawn_blocking(move || delete_install_dir(&prefix_str)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(
            prefix = %prefix_root.display(),
            error = %e,
            "couldn't delete Proton prefix; leaving it in place",
        ),
        Err(e) => tracing::warn!(error = %e, "prefix delete task join failed"),
    }

    // Folder gone (or already absent) — now forget the entry. Reuse the same
    // retain + save path as remove_game.
    let id = id.to_string();
    let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
    lib.entries.retain(|e| e.id != id);
    lib.save()?;
    Ok(())
}

/// Recursively deletes `folder` after validating it's a safe target. A path
/// that doesn't exist is treated as already-deleted (`Ok`). See
/// [`delete_game_from_disk`] for the guard rationale.
fn delete_install_dir(folder: &str) -> AppResult<()> {
    let path = std::path::Path::new(folder);
    if !path.exists() {
        // Already gone — nothing to reclaim, and removing the entry is still
        // the right outcome.
        return Ok(());
    }
    if !path.is_dir() {
        return Err(AppError::Other(format!(
            "Install path is not a folder: {folder}"
        )));
    }
    // Resolve symlinks / `..` so the safety check sees the real target.
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| AppError::Other(format!("couldn't resolve {folder}: {e}")))?;
    if is_unsafe_delete_target(&canonical) {
        return Err(AppError::Other(format!(
            "Refusing to delete '{}' — it looks too broad to be a single game folder.",
            canonical.display()
        )));
    }
    std::fs::remove_dir_all(&canonical)
        .map_err(|e| AppError::Other(format!("couldn't delete {}: {e}", canonical.display())))?;
    Ok(())
}

/// True when `path` is too dangerous to recursively delete: fewer than two
/// path components below root, the user's home directory, or an ancestor of
/// (or equal to) Spool's own data directory.
fn is_unsafe_delete_target(path: &std::path::Path) -> bool {
    use std::path::Component;
    let normals = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if normals < 2 {
        return true;
    }
    if let Some(home) = dirs::home_dir() {
        if path == home {
            return true;
        }
    }
    // Never delete Spool's data dir, nor any ancestor that contains it.
    let app_data = paths::app_data_dir();
    if app_data.starts_with(path) {
        return true;
    }
    false
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

    #[test]
    fn unsafe_delete_target_rejects_shallow_paths() {
        // Root and one-component paths are too broad to be a game folder.
        assert!(is_unsafe_delete_target(std::path::Path::new("/")));
        assert!(is_unsafe_delete_target(std::path::Path::new("/games")));
        // Two components or deeper is fine.
        assert!(!is_unsafe_delete_target(std::path::Path::new(
            "/games/Hades"
        )));
        assert!(!is_unsafe_delete_target(std::path::Path::new(
            "/home/user/Games/Hades"
        )));
    }

    #[test]
    fn unsafe_delete_target_rejects_home_dir() {
        if let Some(home) = dirs::home_dir() {
            assert!(is_unsafe_delete_target(&home));
        }
    }

    #[test]
    fn delete_install_dir_noop_when_missing() {
        // A path that doesn't exist is treated as already-deleted.
        let missing = std::env::temp_dir().join("spool-delete-test-does-not-exist-xyz");
        assert!(delete_install_dir(&missing.to_string_lossy()).is_ok());
    }
}
