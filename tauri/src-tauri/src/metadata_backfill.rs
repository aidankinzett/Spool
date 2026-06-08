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
    let library = app.state::<SharedLibrary>().inner().clone();
    let entries = match library.list().await {
        Ok(e) => e,
        Err(_) => return,
    };
    let todo: Vec<(String, u64)> = entries
        .iter()
        // Not yet fetched (Steam never successfully queried) and still missing a
        // headline field. The `metadata_fetched` marker is what stops this from
        // re-selecting forever for titles Steam returns no developer/etc. for —
        // the old `description || developer empty` filter matched those every boot.
        .filter(|e| !e.metadata_fetched && (e.description.is_empty() || e.developer.is_empty()))
        .filter_map(|e| e.steam_id.map(|sid| (e.id.clone(), sid)))
        .collect();
    if todo.is_empty() {
        return;
    }
    tracing::info!(count = todo.len(), "metadata backfill: starting");

    let client = app.state::<MetadataClient>();
    // Records every entry Steam *responded* for: Some = it had metadata, None =
    // it has no data for that appid. Both are marked fetched below so they are
    // not re-requested next boot; only a network error (logged, not recorded)
    // is left to retry later.
    let mut responded: Vec<(String, Option<GameMetadata>)> = Vec::with_capacity(todo.len());
    for (i, (id, steam_id)) in todo.iter().enumerate() {
        // Throttle between requests (but not before the first).
        if i > 0 {
            tokio::time::sleep(THROTTLE).await;
        }
        match fetch_steam_metadata(client.http(), *steam_id).await {
            Ok(meta) => responded.push((id.clone(), meta)),
            Err(e) => tracing::warn!(game_id = %id, error = %e, "metadata backfill: fetch failed"),
        }
    }
    if responded.is_empty() {
        tracing::info!("metadata backfill: no Steam responses");
        return;
    }

    // Apply. Re-resolve by id (the library may have been mutated during our
    // run) and persist only the metadata fields so concurrent runtime writes
    // aren't clobbered.
    let mut applied = 0;
    for (id, meta) in &responded {
        let Some(mut entry) = library.find(id).await.ok().flatten() else {
            continue;
        };
        // Apply any returned fields, then always set the fetched marker so this
        // entry is not re-requested next boot even when Steam had nothing to add.
        let changed = meta
            .as_ref()
            .map(|m| apply_to_entry(&mut entry, m))
            .unwrap_or(false);
        entry.metadata_fetched = true;
        match library
            .update_fields(id, &crate::metadata::metadata_fields(&entry))
            .await
        {
            // Only count (and later emit for) entries whose visible metadata
            // actually changed; a marker-only write is silent.
            Ok(true) if changed => applied += 1,
            Ok(true) => {}
            // The entry vanished between find and update — nothing written.
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "metadata backfill: update failed"),
        }
    }
    tracing::info!(applied, "metadata backfill: done");

    if applied > 0 {
        if let Err(e) = app.emit("library:changed", &()) {
            tracing::warn!(error = %e, "metadata backfill: emit library:changed failed");
        }
    }
}
