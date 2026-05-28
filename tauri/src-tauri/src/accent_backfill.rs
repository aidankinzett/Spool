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
    // Snapshot the entries that need a backfill — id + cover path.
    // Drop the lock before any spawn_blocking so we're not holding
    // the library mutex during disk + CPU work. The State binding
    // has to live for the whole `lib.lock()` borrow, hence the
    // explicit `library_state` variable rather than a chained call.
    let library_state = app.state::<SharedLibrary>();
    let todo: Vec<(String, PathBuf)> = {
        let lib = match library_state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        lib.entries
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
            .collect()
    };
    drop(library_state);
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

    // Apply results + save once. Re-resolve entries by id since the
    // library may have been mutated by add/remove during our run.
    let library_state = app.state::<SharedLibrary>();
    let applied = {
        let mut lib = match library_state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let mut applied = 0;
        for (id, accent) in &results {
            if let Some(entry) = lib.entries.iter_mut().find(|e| &e.id == id) {
                // Don't overwrite if a concurrent update has already
                // set the accent (e.g. via SteamGridDB refresh).
                if entry.accent_color.is_none() {
                    entry.accent_color = Some(accent.clone());
                    applied += 1;
                }
            }
        }
        if applied > 0 {
            if let Err(e) = lib.save() {
                tracing::warn!(error = %e, "accent backfill: library save failed");
            }
        }
        applied
    };
    tracing::info!(applied, "accent backfill: done");

    if applied > 0 {
        if let Err(e) = app.emit("library:changed", &()) {
            tracing::warn!(error = %e, "accent backfill: emit library:changed failed");
        }
    }
}
