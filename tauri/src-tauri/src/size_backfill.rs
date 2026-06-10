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
            results.push((id, bytes_to_mb(bytes)));
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

/// Recursive `(file count, total bytes)` for a directory tree. Follows symlinks
/// (matches `walkdir::WalkDir::follow_links(true)` used by the LAN walk). Errors
/// on individual files are silently skipped — a partial total is still
/// informative. The move-install flow uses the pair as a copy-verification
/// fingerprint; [`directory_size`] wraps it for callers that only need the bytes.
pub(crate) fn directory_stats(path: &std::path::Path) -> (u64, u64) {
    let mut count = 0u64;
    let mut bytes = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(true) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            count += 1;
            bytes += meta.len();
        }
    }
    (count, bytes)
}

/// Recursive directory size in bytes — the byte half of [`directory_stats`].
pub(crate) fn directory_size(path: &std::path::Path) -> u64 {
    directory_stats(path).1
}

/// Bytes → the MiB unit stored in `GameEntry::install_size_mb`. One definition
/// shared by the backfill, the LAN installer, and the move-install flow so the
/// recorded sizes stay comparable.
pub(crate) fn bytes_to_mb(bytes: u64) -> f64 {
    (bytes as f64) / (1024.0 * 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_stats_counts_files_and_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("sub/b.bin"), b"world!").unwrap();
        assert_eq!(directory_stats(tmp.path()), (2, 11));
        assert_eq!(directory_size(tmp.path()), 11);
    }
}
