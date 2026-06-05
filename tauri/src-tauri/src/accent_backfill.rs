//! One-shot accent-colour backfill for legacy library entries.
//!
//! Pre-rename `ludusavi-wrap` data — and any entry added before the
//! per-game accent extraction landed — has `accent_color: None`. The
//! UI falls back to the brand spool tone in that case, which works
//! but looks flat next to the rich per-game accents on freshly-added
//! entries.
//!
//! This module walks the library at startup, picks the entries that
//! have a cover on disk but no accent yet, and runs the same
//! `extract_vibrant_color` we use during the SteamGridDB cover fetch
//! against each. The library file gets saved once at the end (not
//! per-entry) so a 200-game library doesn't write the json file 200
//! times.
//!
//! Runs as a background task so the app starts instantly — the user
//! sees the brand-default accents for the first second or two on
//! first launch, then the real accents pop in via `library:changed`.

use crate::library::SharedLibrary;
use crate::steamgriddb::extract_vibrant_color;
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
    // Snapshot the entries that need a backfill — id + cover path — with a
    // single `library.list().await`, then work off that owned `Vec`. The async
    // borrow is dropped before any `spawn_blocking` below, so we never hold a
    // library reference across the disk + CPU work of decoding covers.
    let library = app.state::<SharedLibrary>().inner().clone();
    let entries = match library.list().await {
        Ok(e) => e,
        Err(_) => return,
    };
    let todo: Vec<(String, PathBuf)> = entries
        .iter()
        .filter(|e| e.accent_color.is_none())
        .filter_map(|e| {
            e.cover_image_path.as_ref().and_then(|p| {
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
    tracing::info!(count = todo.len(), "accent backfill: starting");

    // Hash each cover off the async runtime — image decode is sync
    // CPU + I/O, same reason `m07-concurrency` says to use
    // spawn_blocking. Collect (id, accent) results.
    let mut results: Vec<(String, String)> = Vec::with_capacity(todo.len());
    for (id, path) in todo {
        if !path.is_file() {
            continue;
        }
        let p = path.clone();
        let accent =
            tokio::task::spawn_blocking(move || extract_vibrant_color(&p)).await.ok().flatten();
        if let Some(a) = accent {
            results.push((id, a));
        }
    }
    if results.is_empty() {
        tracing::info!("accent backfill: no colors extracted");
        return;
    }

    // Apply results. `set_accent_if_empty` is a no-op when a concurrent update
    // (e.g. a SteamGridDB refresh) already set the accent, so we never clobber.
    let mut applied = 0;
    for (id, accent) in &results {
        match library.set_accent_if_empty(id, accent).await {
            Ok(true) => applied += 1,
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "accent backfill: update failed"),
        }
    }
    tracing::info!(applied, "accent backfill: done");

    if applied > 0 {
        if let Err(e) = app.emit("library:changed", &()) {
            tracing::warn!(error = %e, "accent backfill: emit library:changed failed");
        }
    }
}
