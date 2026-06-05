//! Install-size backfill for entries that don't have one yet.
//!
//! Library entries from the C# `ludusavi-wrap` era — and any entry
//! added before the install-size scan landed — have
//! `install_size_mb: 0` despite having a real `game_folder_path` on
//! disk. The UI shows "—" instead of a real size, and the sort-by-
//! size filter misranks them.
//!
//! This module walks the library at startup, picks entries with a
//! folder on disk but zero recorded size, computes the recursive
//! sum, and saves. Same shape as `accent_backfill`: spawn_blocking
//! per directory walk so disk I/O never blocks the runtime, single
//! library save at the end, `library:changed` emit so the UI
//! repaints.

use crate::library::SharedLibrary;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

/// Spawns the backfill task. Returns immediately; the heavy lifting
/// happens off the foreground.
pub fn spawn_backfill(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        run_backfill(app).await;
    });
}

async fn run_backfill(app: AppHandle) {
    // Snapshot entries that need a size — (id, folder_path). Drop
    // the lock before any blocking disk walks.
    let library = app.state::<SharedLibrary>().inner().clone();
    let entries = match library.list().await {
        Ok(e) => e,
        Err(_) => return,
    };
    let todo: Vec<(String, PathBuf)> = entries
        .iter()
        .filter(|e| e.install_size_mb <= 0.0)
        .filter_map(|e| {
            e.game_folder_path.as_ref().and_then(|p| {
                if p.is_empty() {
                    None
                } else {
                    Some((e.id.clone(), PathBuf::from(p)))
                }
            })
        })
        .collect();
    if todo.is_empty() {
        return;
    }
    tracing::info!(count = todo.len(), "size backfill: starting");

    // Walk each folder off the runtime. spawn_blocking per directory
    // so a 100GB game doesn't stall others.
    let mut results: Vec<(String, f64)> = Vec::with_capacity(todo.len());
    for (id, folder) in todo {
        if !folder.is_dir() {
            continue;
        }
        let f = folder.clone();
        let bytes = tokio::task::spawn_blocking(move || directory_size(&f))
            .await
            .unwrap_or(0);
        if bytes > 0 {
            let mb = (bytes as f64) / (1024.0 * 1024.0);
            results.push((id, mb));
        }
    }
    if results.is_empty() {
        tracing::info!("size backfill: no sizes computed");
        return;
    }

    // Apply. `set_install_size_if_empty` no-ops when a concurrent action
    // already recorded a size, so we never clobber.
    let mut applied = 0;
    for (id, mb) in &results {
        match library.set_install_size_if_empty(id, *mb).await {
            Ok(true) => applied += 1,
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "size backfill: update failed"),
        }
    }
    tracing::info!(applied, "size backfill: done");

    if applied > 0 {
        if let Err(e) = app.emit("library:changed", &()) {
            tracing::warn!(error = %e, "size backfill: emit library:changed failed");
        }
    }
}

/// Recursive directory size in bytes. Follows symlinks (matches
/// `walkdir::WalkDir::follow_links(true)` used by the LAN walk).
/// Errors on individual files are silently skipped — a partial
/// total is still informative.
fn directory_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(true) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}
