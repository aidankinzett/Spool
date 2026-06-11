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
use crate::library::{CustomSave, GameEntry, ManifestOverride, SharedLibrary};
use crate::ludusavi::{LudusaviClient, ManifestPath, ManifestSaveData};
use crate::ludusavi_config::{self, CustomGameDef};
use crate::{rclone, save_template};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager, State};

/// Project the library's custom-save games **and** manifest overrides into
/// ludusavi's `customGames` block. Idempotent — regenerated from the library each
/// time. Expands the one token ludusavi can't resolve on its own (`<base>` → the
/// game's install folder); other tokens (Windows known folders, `<home>`) are
/// handed through for ludusavi to resolve (into the Proton prefix when the
/// workflow passes `--wine-prefix`). A `<base>` token with no install folder is
/// dropped.
///
/// A manifest override is re-derived **on this device** from its own manifest
/// (the stored intent is exclusions, not resolved paths), so the right OS-
/// appropriate paths are emitted even when a game is native on one OS and a
/// Windows build under Proton on another. Needs the `LudusaviClient` (manifest
/// cache) and so takes the `AppHandle`; the manifest is only loaded when some
/// game actually has an active override.
pub async fn sync_ludusavi_custom_games(app: &AppHandle) -> AppResult<()> {
    let library = app.state::<SharedLibrary>().inner().clone();
    let games = library.list().await?;

    let ludusavi = app.state::<LudusaviClient>();
    let ludusavi_exe = crate::paths::resolve_ludusavi_path();
    let config_dir = crate::paths::ludusavi_config_dir();

    let mut defs: Vec<CustomGameDef> = Vec::new();
    for g in &games {
        if let Some(def) =
            custom_game_def_for(g, &ludusavi, ludusavi_exe.as_deref(), &config_dir).await
        {
            defs.push(def);
        }
    }
    ludusavi_config::set_custom_games(&defs)
}

/// Build the `customGames` entry for one game, or `None` to leave it on the plain
/// manifest path. Handles three cases: an active manifest override (re-derive the
/// kept manifest paths on this device, emit an `override`), a user custom-save
/// (the existing extend-or-define behavior), or neither.
async fn custom_game_def_for(
    g: &GameEntry,
    ludusavi: &LudusaviClient,
    ludusavi_exe: Option<&Path>,
    config_dir: &Path,
) -> Option<CustomGameDef> {
    let manifest_covered = !g.save_paths.is_empty();
    let override_active = g.manifest_override.as_ref().is_some_and(|o| o.is_active());

    // User-added literal save folders (existing custom-save behavior).
    let user_files: Vec<String> = g
        .custom_save
        .as_ref()
        .map(|cs| {
            cs.files
                .iter()
                .filter_map(|t| save_template::expand_base(t, g.game_folder_path.as_deref()))
                .collect()
        })
        .unwrap_or_default();
    let user_registry: Vec<String> = g
        .custom_save
        .as_ref()
        .map(|cs| cs.registry.clone())
        .unwrap_or_default();

    if override_active && manifest_covered {
        let derived = match ludusavi_exe {
            Some(exe) => {
                let target_os = crate::ludusavi::target_os_for(g.uses_proton());
                ludusavi
                    .manifest_save_data(exe, config_dir, &g.game_name, target_os)
                    .await
            }
            None => Err(AppError::Other("ludusavi sidecar not found".into())),
        };
        match derived {
            Ok(data) => {
                let ov = g.manifest_override.as_ref().expect("override_active");
                let (kept_file_templates, kept_registry) = apply_override(&data, ov);
                // Resolve the kept manifest templates. `expand_base` returns None
                // only for a `<base>`-relative path when the install folder is
                // unset on this device — and an override (`integration: override`)
                // fully replaces the manifest entry, so emitting a reduced set that
                // silently omits that path would stop backing up a save the user
                // never excluded. Leave the full manifest entry instead (ludusavi
                // resolves `<base>` via its own install-dir detection); the
                // exclusion applies once the install folder is known.
                let mut files: Vec<String> = Vec::with_capacity(kept_file_templates.len());
                for t in &kept_file_templates {
                    match save_template::expand_base(t, g.game_folder_path.as_deref()) {
                        Some(p) => files.push(p),
                        None => {
                            tracing::warn!(
                                game = %g.game_name, template = %t,
                                "manifest override can't resolve <base> (install folder unset) — leaving full manifest entry"
                            );
                            return None;
                        }
                    }
                }
                files.extend(user_files);
                let mut registry = kept_registry;
                registry.extend(user_registry);
                if files.is_empty() && registry.is_empty() {
                    // The override excluded everything resolvable. An override with
                    // no paths would make ludusavi recognise the game yet back up
                    // nothing while reporting it synced — leave the manifest entry.
                    tracing::warn!(
                        game = %g.game_name,
                        "manifest override excluded every resolvable path — leaving full manifest entry"
                    );
                    return None;
                }
                // `extend: false` ⇒ ludusavi's default `override`: replace the
                // manifest entry with exactly this (reduced) path set.
                return Some(CustomGameDef {
                    name: g.game_name.clone(),
                    files,
                    registry,
                    extend: false,
                });
            }
            Err(e) => {
                // Manifest unavailable — fall back to NOT applying the override
                // (the full manifest entry stands). Safe: at worst the excluded
                // config is still backed up until the manifest loads.
                tracing::warn!(
                    game = %g.game_name, error = %e,
                    "manifest override could not be re-derived; leaving full manifest entry"
                );
            }
        }
    }

    // No active override (or re-derivation failed): existing custom-save behavior.
    g.custom_save.as_ref()?; // nothing to register without a user custom save
    if user_files.is_empty() && user_registry.is_empty() {
        // A definition with no resolvable files AND no registry (e.g. a
        // `<base>`-relative save whose install folder isn't set on this device)
        // would make ludusavi recognise the game yet back up nothing — and the
        // runner would then falsely report it synced. Skip it so it stays in
        // `unknownGames` and the post-session backup is honestly skipped.
        tracing::warn!(
            game = %g.game_name,
            "custom save has no resolvable files (install folder unset?) — not registering"
        );
        return None;
    }
    Some(CustomGameDef {
        name: g.game_name.clone(),
        files: user_files,
        registry: user_registry,
        // Supplement (don't replace) the manifest's locations when the game is
        // already manifest-covered (has manifest save paths).
        extend: manifest_covered,
    })
}

/// Apply a manifest override's exclusions to the device's manifest data, yielding
/// the kept file *templates* and kept registry keys. Pure — unit-tested.
///
/// A path is tag-excluded only when it has tags and *every* one is excluded — so
/// "exclude settings" drops a pure `config` file but keeps a file tagged both
/// `save` and `config` (dropping it would lose the save). `excluded_tags` covers
/// registry entries too (registry-stored settings). `excluded_paths` drop specific
/// file templates only — literal path clicks, meaningless against registry keys.
/// Only paths that `applies` on this device are considered.
fn apply_override(data: &ManifestSaveData, ov: &ManifestOverride) -> (Vec<String>, Vec<String>) {
    let tag_excluded = |p: &ManifestPath| {
        !p.tags.is_empty() && p.tags.iter().all(|t| ov.excluded_tags.contains(t))
    };
    let files = data
        .files
        .iter()
        .filter(|p| p.applies && !ov.excluded_paths.contains(&p.template) && !tag_excluded(p))
        .map(|p| p.template.clone())
        .collect();
    let registry = data
        .registry
        .iter()
        .filter(|p| p.applies && !tag_excluded(p))
        .map(|p| p.template.clone())
        .collect();
    (files, registry)
}

/// [`sync_ludusavi_custom_games`] but log-and-continue — for the hot launch path
/// and startup, where a config-write hiccup must not abort the operation.
pub async fn sync_best_effort(app: &AppHandle) {
    if let Err(e) = sync_ludusavi_custom_games(app).await {
        tracing::warn!(error = %e, "failed to sync ludusavi customGames block");
    }
}

/// The Proton/Wine prefix ROOT for a game (the dir containing `drive_c`), or
/// `None` when it doesn't launch through Proton. Mirrors the resolution the
/// runner and `backup_game_core` use.
fn prefix_root_for(entry: &GameEntry) -> Option<String> {
    crate::proton::resolve_prefix_root(
        entry.uses_proton(),
        entry.wine_prefix_path.as_deref(),
        &entry.id,
    )
    .map(|p| p.to_string_lossy().into_owned())
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
        candidates.push(pfx.join(crate::proton::WINE_STEAMUSER_PROFILE));
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

/// Whether a Proton game's Wine prefix has been generated yet — i.e. its user
/// profile (`drive_c/users/steamuser`) exists. Drives a hint in the Saves editor
/// telling the user to launch the game once before picking a save folder, since
/// the prefix (and therefore the save location) is created on the first run.
/// Always `true` for native games / on Windows, where no prefix is involved.
#[tauri::command]
pub async fn prefix_ready(library: State<'_, SharedLibrary>, game_id: String) -> AppResult<bool> {
    let entry = library
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
    if !entry.uses_proton() {
        return Ok(true);
    }
    Ok(prefix_root_for(&entry)
        .map(|p| {
            std::path::Path::new(&p)
                .join(crate::proton::WINE_STEAMUSER_PROFILE)
                .is_dir()
        })
        .unwrap_or(false))
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

    let custom = CustomSave {
        files: files.clone(),
        registry: registry.clone(),
    };
    if !library.set_custom_save(&game_id, Some(&custom)).await? {
        return Err(AppError::Other(format!("game not found: {game_id}")));
    }

    sync_best_effort(&app).await;
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
    sync_best_effort(&app).await;
    rclone::delete_custom_save(&app, &game_name).await;
    let _ = app.emit("library:changed", &game_id);
    Ok(())
}

/// Trim, drop blanks, and de-dupe a user-supplied exclusion list.
fn clean_list(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in items {
        let s = s.trim().to_string();
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// Progress event for the post-override forced backup, consumed by the frontend
/// to toast and to flip the game's Play button to "Backing up…". `phase` is one of
/// `started` / `done` / `failed`; `cloud_synced` is only meaningful on `done`.
#[derive(Clone, serde::Serialize)]
struct SavesBackupEvent<'a> {
    game_id: &'a str,
    game_name: &'a str,
    phase: &'a str,
    cloud_synced: Option<bool>,
}

fn emit_backup(
    app: &AppHandle,
    game_id: &str,
    game_name: &str,
    phase: &str,
    cloud_synced: Option<bool>,
) {
    let _ = app.emit(
        "saves:backup",
        SavesBackupEvent {
            game_id,
            game_name,
            phase,
            cloud_synced,
        },
    );
}

/// Force a fresh backup (local + cloud) in the background so the *latest* snapshot
/// — the one normal launches restore — reflects a just-narrowed override. Spawned
/// detached so the editor doesn't block on the (possibly multi-minute) cloud
/// upload or up to 180 s of contention on the cross-process backup lock; the
/// override itself is already persisted and synced before this runs, so future
/// backups apply it regardless. Emits `saves:backup` (started/done/failed) so the
/// UI can toast and reflect progress, plus `library:changed` on success so the
/// badge refreshes. Best-effort otherwise. Assumes the `customGames` block was
/// already re-synced (call [`sync_best_effort`] first).
fn spawn_force_backup(app: AppHandle, game_id: String, game_name: String) {
    tauri::async_runtime::spawn(async move {
        emit_backup(&app, &game_id, &game_name, "started", None);
        let Some(exe) = crate::paths::resolve_ludusavi_path() else {
            tracing::warn!(
                "ludusavi sidecar not found — skipping forced backup after override change"
            );
            emit_backup(&app, &game_id, &game_name, "failed", None);
            return;
        };
        let config_dir = crate::paths::ludusavi_config_dir();
        let client = app.state::<LudusaviClient>();
        let library = app.state::<SharedLibrary>();
        match crate::runner::backup_game_core(&client, &exe, &config_dir, &library, &game_id).await
        {
            Ok(res) => {
                emit_backup(&app, &game_id, &game_name, "done", Some(res.cloud_synced));
                let _ = app.emit("library:changed", &game_id);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "forced backup after manifest-override change failed; future backups will still apply it"
                );
                emit_backup(&app, &game_id, &game_name, "failed", None);
            }
        }
    });
}

/// Set (or update) a game's manifest override — the user's choice of which
/// manifest-declared save locations actually sync. Persists it, re-syncs
/// ludusavi's `customGames` block so the reduced set takes effect, forces an
/// immediate backup so the latest snapshot (which normal launches restore) drops
/// the excluded paths, and publishes the exclusion intent so other devices adopt
/// it. An empty selection (nothing excluded) is treated as a clear.
#[tauri::command]
pub async fn set_manifest_override(
    app: AppHandle,
    library: State<'_, SharedLibrary>,
    game_id: String,
    excluded_tags: Vec<String>,
    excluded_paths: Vec<String>,
) -> AppResult<()> {
    let ov = ManifestOverride {
        excluded_tags: clean_list(excluded_tags),
        excluded_paths: clean_list(excluded_paths),
    };
    let game_name = library
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?
        .game_name;

    // Nothing excluded ⇒ the plain manifest entry stands. Clear instead.
    if !ov.is_active() {
        library.set_manifest_override(&game_id, None).await?;
        sync_best_effort(&app).await;
        rclone::delete_manifest_override(&app, &game_name).await;
        let _ = app.emit("library:changed", &game_id);
        return Ok(());
    }

    if !library.set_manifest_override(&game_id, Some(&ov)).await? {
        return Err(AppError::Other(format!("game not found: {game_id}")));
    }
    // Re-sync the customGames block (so ludusavi sees the reduced set) BEFORE
    // kicking off the forced backup, so the new snapshot omits the excluded paths.
    sync_best_effort(&app).await;
    rclone::publish_manifest_override(&app, &game_name, &ov).await;
    let _ = app.emit("library:changed", &game_id);
    // Background: refresh the latest snapshot so the exclusion takes effect for the
    // next launch without blocking the editor on the cloud upload.
    spawn_force_backup(app.clone(), game_id, game_name);
    Ok(())
}

/// Clear a game's manifest override — back to syncing the full manifest entry.
/// Doesn't force a backup: re-including paths is what the next normal backup does
/// anyway, and restores will bring the previously-excluded data back.
#[tauri::command]
pub async fn clear_manifest_override(
    app: AppHandle,
    library: State<'_, SharedLibrary>,
    game_id: String,
) -> AppResult<()> {
    let game_name = library
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?
        .game_name;

    library.set_manifest_override(&game_id, None).await?;
    sync_best_effort(&app).await;
    rclone::delete_manifest_override(&app, &game_name).await;
    let _ = app.emit("library:changed", &game_id);
    Ok(())
}

/// Adopt a published custom-save definition and/or manifest override for a
/// freshly-added game whose name matches one another device published — so the
/// user only configures saves once. Best-effort and spawned from `add_game`; each
/// is a no-op when the game already has its own, when none is published, or when
/// cloud isn't configured.
pub async fn adopt_for_new_game(app: &AppHandle, game_id: &str, game_name: &str) {
    let library = app.state::<SharedLibrary>().inner().clone();
    let (has_custom, has_override) = match library.find(game_id).await {
        Ok(Some(e)) => (e.custom_save.is_some(), e.manifest_override.is_some()),
        _ => return,
    };

    let mut adopted = false;
    // Conditional writes: if the user set their own during the network fetch,
    // don't clobber it (the earlier check was a TOCTOU).
    if !has_custom {
        if let Some(def) = rclone::fetch_custom_save(app, game_name).await {
            if library
                .set_custom_save_if_absent(game_id, &def)
                .await
                .unwrap_or(false)
            {
                adopted = true;
                tracing::info!(game_name, "adopted published custom-save definition");
            }
        }
    }
    if !has_override {
        if let Some(ov) = rclone::fetch_manifest_override(app, game_name).await {
            if library
                .set_manifest_override_if_absent(game_id, &ov)
                .await
                .unwrap_or(false)
            {
                adopted = true;
                tracing::info!(game_name, "adopted published manifest override");
            }
        }
    }
    if adopted {
        sync_best_effort(app).await;
        let _ = app.emit("library:changed", game_id);
    }
}

/// Startup task: adopt any published definitions for games already in this
/// device's library, then sync the `customGames` block. The sync runs even with
/// no cloud / nothing adopted, so a custom save set on THIS device offline is
/// still recognised by ludusavi. Runs after the device-fold settle window.
pub fn spawn_startup_adopt(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        // Independent network passes over the (quota-limited) remote — run them
        // concurrently so adoption isn't two serial round trips.
        let (custom, overrides) = tokio::join!(
            rclone::fold_custom_saves(&app),
            rclone::fold_manifest_overrides(&app)
        );
        let applied = custom + overrides;
        sync_best_effort(&app).await;
        if applied > 0 {
            tracing::info!(
                applied,
                "adopted cross-device custom-save / manifest-override definitions"
            );
            let _ = app.emit("library:changed", &());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(template: &str, tags: &[&str], applies: bool) -> ManifestPath {
        ManifestPath {
            template: template.to_string(),
            pretty: template.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            applies,
        }
    }

    fn ov(tags: &[&str], paths: &[&str]) -> ManifestOverride {
        ManifestOverride {
            excluded_tags: tags.iter().map(|s| s.to_string()).collect(),
            excluded_paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn apply_override_drops_config_tag_keeps_saves() {
        let data = ManifestSaveData {
            files: vec![
                path("<winLocalAppData>/Game/Saved", &["save"], true),
                path("<winLocalAppData>/Game/Config", &["config"], true),
            ],
            registry: vec![],
        };
        let (files, registry) = apply_override(&data, &ov(&["config"], &[]));
        assert_eq!(files, vec!["<winLocalAppData>/Game/Saved".to_string()]);
        assert!(registry.is_empty());
    }

    #[test]
    fn apply_override_tag_exclusion_also_drops_registry() {
        // "exclude config" covers registry-stored settings too.
        let data = ManifestSaveData {
            files: vec![path("<base>/save.dat", &["save"], true)],
            registry: vec![
                path("HKEY_CURRENT_USER/Software/Game/Save", &["save"], true),
                path(
                    "HKEY_CURRENT_USER/Software/Game/Graphics",
                    &["config"],
                    true,
                ),
            ],
        };
        let (files, registry) = apply_override(&data, &ov(&["config"], &[]));
        assert_eq!(files, vec!["<base>/save.dat".to_string()]);
        assert_eq!(
            registry,
            vec!["HKEY_CURRENT_USER/Software/Game/Save".to_string()]
        );
    }

    #[test]
    fn apply_override_keeps_path_tagged_both_save_and_config() {
        // A file that is BOTH a save and config isn't dropped by excluding config
        // — dropping it would lose the save. Shown to the user as a mixed row.
        let data = ManifestSaveData {
            files: vec![
                path("<base>/profile.dat", &["save", "config"], true),
                path("<base>/graphics.ini", &["config"], true),
            ],
            registry: vec![],
        };
        let (files, _) = apply_override(&data, &ov(&["config"], &[]));
        assert_eq!(files, vec!["<base>/profile.dat".to_string()]);
    }

    #[test]
    fn apply_override_excluded_path_is_template_specific() {
        let data = ManifestSaveData {
            files: vec![
                path("<winLocalAppData>/Game/Saved", &["save"], true),
                path("<winDocuments>/Game/options.ini", &["config"], true),
            ],
            registry: vec![],
        };
        // Per-path exclusion matches only the exact template; registry keys are
        // never matched by excluded_paths.
        let (files, _) = apply_override(&data, &ov(&[], &["<winDocuments>/Game/options.ini"]));
        assert_eq!(files, vec!["<winLocalAppData>/Game/Saved".to_string()]);
    }

    #[test]
    fn apply_override_skips_paths_that_dont_apply_here() {
        // A Linux-only path on a Windows/Proton device is excluded by `applies`,
        // not by the override — reconstructing the manifest's `when:` gating.
        let data = ManifestSaveData {
            files: vec![
                path("<winLocalAppData>/Game/Saved", &["save"], true),
                path("<home>/.local/share/Game", &["save"], false),
            ],
            registry: vec![],
        };
        let (files, _) = apply_override(&data, &ov(&["config"], &[]));
        assert_eq!(files, vec!["<winLocalAppData>/Game/Saved".to_string()]);
    }
}
