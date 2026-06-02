//! One-shot Steam Store metadata backfill.
//!
//! Library entries added before metadata fetching shipped — and any
//! entry whose enrichment failed at add-time — have a `steam_id` but
//! empty `description` / `developer` fields. This walks the library at
//! startup, picks those entries, fetches Steam Store metadata for each,
//! and folds in any missing fields.
//!
//! Same shape as `accent_backfill` / `size_backfill`: snapshot the todo
//! list, do the network work off the foreground, save the library once
//! at the end, emit `library:changed` so the UI repaints. The Steam
//! Store endpoint is rate-limited (~200 req / 5 min), so we throttle
//! between requests rather than firing the whole library at once.

use crate::library::SharedLibrary;
use crate::metadata::{apply_to_entry, fetch_steam_metadata, GameMetadata, MetadataClient};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Pause between Steam Store requests to stay well under the rate limit.
const THROTTLE: Duration = Duration::from_millis(1500);

/// Spawns the backfill task. Returns immediately; the network work
/// happens off the foreground.
pub fn spawn_backfill(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        run_backfill(app).await;
    });
}

async fn run_backfill(app: AppHandle) {
    // Snapshot entries that have a steam_id but are missing the headline
    // metadata fields. Drop the lock before any network work.
    let library_state = app.state::<SharedLibrary>();
    let todo: Vec<(String, u64)> = {
        let lib = match library_state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        lib.entries
            .iter()
            .filter(|e| e.description.is_empty() || e.developer.is_empty())
            .filter_map(|e| e.steam_id.map(|sid| (e.id.clone(), sid)))
            .collect()
    };
    if todo.is_empty() {
        return;
    }
    tracing::info!(count = todo.len(), "metadata backfill: starting");

    let client = app.state::<MetadataClient>();
    let mut results: Vec<(String, GameMetadata)> = Vec::with_capacity(todo.len());
    for (i, (id, steam_id)) in todo.iter().enumerate() {
        // Throttle between requests (but not before the first).
        if i > 0 {
            tokio::time::sleep(THROTTLE).await;
        }
        match fetch_steam_metadata(client.http(), *steam_id).await {
            Ok(Some(meta)) => results.push((id.clone(), meta)),
            Ok(None) => {}
            Err(e) => tracing::warn!(game_id = %id, error = %e, "metadata backfill: fetch failed"),
        }
    }
    if results.is_empty() {
        tracing::info!("metadata backfill: nothing fetched");
        return;
    }

    // Apply + save once. Re-resolve by id since the library may have
    // been mutated by add/remove during our run.
    let library_state = app.state::<SharedLibrary>();
    let applied = {
        let mut lib = match library_state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let mut applied = 0;
        for (id, meta) in &results {
            if let Some(entry) = lib.entries.iter_mut().find(|e| &e.id == id) {
                if apply_to_entry(entry, meta) {
                    applied += 1;
                }
            }
        }
        if applied > 0 {
            if let Err(e) = lib.save() {
                tracing::warn!(error = %e, "metadata backfill: library save failed");
            }
        }
        applied
    };
    tracing::info!(applied, "metadata backfill: done");

    if applied > 0 {
        if let Err(e) = app.emit("library:changed", &()) {
            tracing::warn!(error = %e, "metadata backfill: emit library:changed failed");
        }
    }
}
