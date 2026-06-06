//! Custom save locations for non-manifest games — orchestration + commands.
//!
//! Ties together the pieces that let a game ludusavi's manifest doesn't cover
//! still get its saves backed up, restored, and synced across devices:
//!
//!   * [`crate::library::CustomSave`] — the per-game definition (portable
//!     templates), persisted in the library.
//!   * [`crate::save_template`]       — folder → portable template, and the
//!     `<base>` expansion.
//!   * [`crate::ludusavi_config::set_custom_games`] — writes the `customGames`
//!     block so ludusavi *recognises* the game (otherwise the run workflow sees
//!     it under `unknownGames` and skips its backup).
//!   * [`crate::rclone`] custom-save blobs — replicate the definition so the
//!     user only picks the folder once, on any device.
//!
//! Once the `customGames` block is in place, a custom-save game rides the
//! existing `runner.rs` / `redirects.rs` machinery unchanged: backup, restore,
//! cross-OS redirects, and cloud-conflict detection all key off the game name
//! and the recorded path *format*, never off whether ludusavi learned the game
//! from the manifest or a custom entry.

use crate::error::{AppError, AppResult};
use crate::library::{CustomSave, GameEntry, SharedLibrary};
use crate::ludusavi_config::{self, CustomGameDef};
use crate::{rclone, save_template};
use tauri::{AppHandle, Emitter, Manager, State};

/// Project the library's custom-save games into ludusavi's `customGames` block.
/// Idempotent — regenerated from the library each time. Expands the one token
/// ludusavi can't resolve on its own (`<base>` → the game's install folder);
/// other tokens (Windows known folders, `<home>`) are handed through for
/// ludusavi to resolve (into the Proton prefix when the workflow passes
/// `--wine-prefix`). A `<base>` token with no install folder is dropped.
pub async fn sync_ludusavi_custom_games(library: &SharedLibrary) -> AppResult<()> {
    let games = library.list().await?;
    let defs: Vec<CustomGameDef> = games
        .iter()
        .filter_map(|g| g.custom_save.as_ref().map(|cs| (g, cs)))
        .map(|(g, cs)| {
            let files = cs
                .files
                .iter()
                .filter_map(|t| save_template::expand_base(t, g.game_folder_path.as_deref()))
                .collect();
            CustomGameDef {
                name: g.game_name.clone(),
                files,
                registry: cs.registry.clone(),
            }
        })
        .collect();
    ludusavi_config::set_custom_games(&defs)
}

/// [`sync_ludusavi_custom_games`] but log-and-continue — for the hot launch path
/// and startup, where a config-write hiccup must not abort the operation.
pub async fn sync_best_effort(library: &SharedLibrary) {
    if let Err(e) = sync_ludusavi_custom_games(library).await {
        tracing::warn!(error = %e, "failed to sync ludusavi customGames block");
    }
}

/// The Proton/Wine prefix ROOT for a game (the dir containing `drive_c`), or
/// `None` when it doesn't launch through Proton. Mirrors the resolution the
/// runner and `backup_game_core` use.
fn prefix_root_for(entry: &GameEntry) -> Option<String> {
    if !entry.uses_proton() {
        return None;
    }
    let root = entry
        .wine_prefix_path
        .clone()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| {
            crate::proton::game_prefix_path(&entry.id)
                .to_string_lossy()
                .into_owned()
        });
    Some(root)
}

/// Classify a picked folder into a portable save template (preview for the
/// editor). Pure aside from looking up the game's prefix/install folder.
#[tauri::command]
pub async fn derive_save_template(
    library: State<'_, SharedLibrary>,
    game_id: String,
    picked_path: String,
) -> AppResult<String> {
    let entry = library
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
    let prefix_root = prefix_root_for(&entry);
    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
    Ok(save_template::classify(
        &picked_path,
        prefix_root.as_deref(),
        entry.game_folder_path.as_deref(),
        home.as_deref(),
    ))
}

/// The directory the Saves folder-picker should open at: deep inside the game's
/// Proton prefix (its user profile, where AppData / Documents / Saved Games
/// live) when it launches through Proton, else the install folder or the home
/// dir. Returns the deepest path that actually exists so the dialog opens
/// somewhere valid — the prefix's user dir only appears after the first launch,
/// so we fall back outward. `None` when nothing suitable exists yet.
#[tauri::command]
pub async fn save_picker_start_dir(
    library: State<'_, SharedLibrary>,
    game_id: String,
) -> AppResult<Option<String>> {
    let entry = library
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
    Ok(pick_start_dir(&entry))
}

fn pick_start_dir(entry: &GameEntry) -> Option<String> {
    use std::path::PathBuf;
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(prefix) = prefix_root_for(entry) {
        let pfx = PathBuf::from(prefix);
        candidates.push(pfx.join("drive_c/users/steamuser"));
        candidates.push(pfx.join("drive_c"));
        candidates.push(pfx);
    }
    if let Some(folder) = entry
        .game_folder_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        candidates.push(PathBuf::from(folder));
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(home);
    }
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Set (or replace) a game's custom save location: persist it, refresh
/// ludusavi's `customGames` block so the next backup tracks it, and publish the
/// portable definition so other devices adopt it.
#[tauri::command]
pub async fn set_custom_save(
    app: AppHandle,
    library: State<'_, SharedLibrary>,
    game_id: String,
    files: Vec<String>,
    registry: Vec<String>,
) -> AppResult<()> {
    let files: Vec<String> = files
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if files.is_empty() {
        return Err(AppError::Other("Pick at least one save folder.".into()));
    }
    let registry: Vec<String> = registry
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let game_name = library
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?
        .game_name;

    let custom = CustomSave { files: files.clone(), registry: registry.clone() };
    if !library.set_custom_save(&game_id, Some(&custom)).await? {
        return Err(AppError::Other(format!("game not found: {game_id}")));
    }

    sync_best_effort(library.inner()).await;
    rclone::publish_custom_save(&app, &game_name, &files, &registry).await;
    let _ = app.emit("library:changed", &game_id);
    Ok(())
}

/// Stop tracking a custom save location: clear it locally, refresh the
/// `customGames` block, and remove the published definition.
#[tauri::command]
pub async fn clear_custom_save(
    app: AppHandle,
    library: State<'_, SharedLibrary>,
    game_id: String,
) -> AppResult<()> {
    let game_name = library
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?
        .game_name;

    library.set_custom_save(&game_id, None).await?;
    sync_best_effort(library.inner()).await;
    rclone::delete_custom_save(&app, &game_name).await;
    let _ = app.emit("library:changed", &game_id);
    Ok(())
}

/// Adopt a published custom-save definition for a freshly-added game whose name
/// matches one another device published — so the user only specifies the folder
/// once. Best-effort and spawned from `add_game`; a no-op when the game was
/// added with its own custom save, when none is published, or when cloud isn't
/// configured.
pub async fn adopt_for_new_game(app: &AppHandle, game_id: &str, game_name: &str) {
    let library = app.state::<SharedLibrary>().inner().clone();
    match library.find(game_id).await {
        Ok(Some(e)) if e.custom_save.is_some() => return,
        Ok(Some(_)) => {}
        _ => return,
    }
    let Some(def) = rclone::fetch_custom_save(app, game_name).await else {
        return;
    };
    if library.set_custom_save(game_id, Some(&def)).await.unwrap_or(false) {
        sync_best_effort(&library).await;
        let _ = app.emit("library:changed", game_id);
        tracing::info!(game_name, "adopted published custom-save definition");
    }
}

/// Startup task: adopt any published definitions for games already in this
/// device's library, then sync the `customGames` block. The sync runs even with
/// no cloud / nothing adopted, so a custom save set on THIS device offline is
/// still recognised by ludusavi. Runs after the device-fold settle window.
pub fn spawn_startup_adopt(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let applied = rclone::fold_custom_saves(&app).await;
        let library = app.state::<SharedLibrary>().inner().clone();
        sync_best_effort(&library).await;
        if applied > 0 {
            tracing::info!(applied, "adopted cross-device custom-save definitions");
            let _ = app.emit("library:changed", &());
        }
    });
}
