//! Disk persistence for the LAN manifest hash cache.
//!
//! The cache itself ([`super::server::HashCache`]) maps absolute file
//! path → (mtime, blake3 hex). Hashing a large game takes ~1 s/GB, so
//! losing the cache on every app restart means the first peer to request
//! a big game's manifest waits minutes and can time out (#435). This
//! module round-trips the cache through a JSON file so a restart only
//! costs mtime stats, not re-hashing.
//!
//! Cache validation compares mtimes by exact `SystemTime` equality
//! (see `walk_game_files_with_hashes`), so the persisted form stores
//! `(secs, nanos)` since `UNIX_EPOCH` — a millisecond representation
//! would lose precision and silently miss on every reload, re-hashing
//! everything while appearing to work.
//!
//! Both functions are synchronous by design: loading happens once at
//! startup before the async runtime matters, and saving is called from
//! the folder walk, which already runs under `spawn_blocking`.

use super::server::{HashCache, HASH_CACHE_MAX_ENTRIES};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(default)]
struct PersistedHashCache {
    entries: Vec<PersistedEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(default)]
struct PersistedEntry {
    path: String,
    mtime_secs: u64,
    mtime_nanos: u32,
    hash: String,
}

/// Reads the persisted cache from `path`. Best-effort: a missing or
/// corrupt file yields an empty map (logged, never an error) — the cost
/// is just re-hashing, the same as before persistence existed. An
/// oversized file is truncated to [`HASH_CACHE_MAX_ENTRIES`].
pub(crate) fn load_blocking(path: &Path) -> HashMap<PathBuf, (SystemTime, String)> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
        Err(e) => {
            tracing::warn!(error = %e, "lan hash cache: read failed, starting empty");
            return HashMap::new();
        }
    };
    let parsed: PersistedHashCache = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "lan hash cache: parse failed, starting empty");
            return HashMap::new();
        }
    };
    let mut out = HashMap::new();
    for e in parsed.entries.into_iter().take(HASH_CACHE_MAX_ENTRIES) {
        // Defensive against a corrupt / hand-edited file: nanos ≥ 1 s would
        // make `Duration::new` carry (panicking near u64::MAX secs), and a
        // time past the platform's `SystemTime` range would make `+` panic —
        // either would crash startup. Entries we saved always satisfy both
        // (`subsec_nanos()` < 1e9, mtimes from real files); anything else is
        // junk, and skipping it costs one re-hash.
        if e.mtime_nanos >= 1_000_000_000 {
            tracing::warn!(path = %e.path, nanos = e.mtime_nanos, "lan hash cache: invalid mtime, skipping entry");
            continue;
        }
        let Some(mtime) = UNIX_EPOCH.checked_add(Duration::new(e.mtime_secs, e.mtime_nanos))
        else {
            tracing::warn!(path = %e.path, secs = e.mtime_secs, "lan hash cache: mtime out of range, skipping entry");
            continue;
        };
        out.insert(PathBuf::from(e.path), (mtime, e.hash));
    }
    tracing::debug!(entries = out.len(), "lan hash cache: loaded from disk");
    out
}

/// Persists the cache to `path` (atomic tmp → rename). Snapshots under a
/// read guard and drops it before serialising so the lock isn't held for
/// the file write. Pre-epoch mtimes (can't happen on real game files,
/// but the type allows them) are skipped. Failures log and continue —
/// the in-memory cache still works for this session.
pub(crate) fn save_blocking(path: &Path, cache: &HashCache) {
    let entries: Vec<PersistedEntry> = {
        let g = match cache.read() {
            Ok(g) => g,
            Err(_) => return,
        };
        g.iter()
            .filter_map(|(p, (mtime, hash))| {
                let d = mtime.duration_since(UNIX_EPOCH).ok()?;
                Some(PersistedEntry {
                    path: p.to_string_lossy().into_owned(),
                    mtime_secs: d.as_secs(),
                    mtime_nanos: d.subsec_nanos(),
                    hash: hash.clone(),
                })
            })
            .collect()
    };
    let count = entries.len();
    let bytes = match serde_json::to_vec(&PersistedHashCache { entries }) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "lan hash cache: serialise failed");
            return;
        }
    };
    if let Err(e) = crate::paths::write_atomic(path, &bytes, false) {
        tracing::warn!(error = %e, "lan hash cache: write failed");
    } else {
        tracing::debug!(entries = count, "lan hash cache: saved to disk");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    fn cache_of(entries: Vec<(PathBuf, (SystemTime, String))>) -> HashCache {
        Arc::new(RwLock::new(entries.into_iter().collect()))
    }

    /// Round-trip must preserve mtimes *exactly* — the walk validates by
    /// `SystemTime` equality, so any precision loss (e.g. persisting
    /// milliseconds) would make every reloaded entry a silent cache miss.
    #[test]
    fn round_trips_exact_mtimes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cache.json");
        let mt = UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_789);
        let cache = cache_of(vec![(
            PathBuf::from("/games/Foo/data.pak"),
            (mt, "abc123".to_string()),
        )]);
        save_blocking(&file, &cache);
        let loaded = load_blocking(&file);
        assert_eq!(
            loaded.get(Path::new("/games/Foo/data.pak")),
            Some(&(mt, "abc123".to_string()))
        );
    }

    /// Same guarantee against a real filesystem mtime: stat a file we just
    /// wrote, persist that mtime, reload, and compare with a fresh stat.
    #[test]
    fn round_trips_real_file_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("game.bin");
        std::fs::write(&data, b"bytes").unwrap();
        let mt = std::fs::metadata(&data).unwrap().modified().unwrap();

        let file = dir.path().join("cache.json");
        let cache = cache_of(vec![(data.clone(), (mt, "deadbeef".to_string()))]);
        save_blocking(&file, &cache);
        let loaded = load_blocking(&file);

        let fresh = std::fs::metadata(&data).unwrap().modified().unwrap();
        assert_eq!(loaded.get(&data).map(|(m, _)| *m), Some(fresh));
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_blocking(&dir.path().join("nope.json")).is_empty());
    }

    #[test]
    fn corrupt_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cache.json");
        std::fs::write(&file, b"{ not json").unwrap();
        assert!(load_blocking(&file).is_empty());
    }

    /// Out-of-range mtime values in the file must be skipped, not panic the
    /// startup load — `Duration::new`'s nanos carry and the `SystemTime`
    /// addition both abort on overflow if fed unchecked.
    #[test]
    fn out_of_range_mtimes_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cache.json");
        let entries = vec![
            PersistedEntry {
                path: "/games/ok".into(),
                mtime_secs: 1_700_000_000,
                mtime_nanos: 0,
                hash: "h1".into(),
            },
            PersistedEntry {
                path: "/games/bad-nanos".into(),
                mtime_secs: u64::MAX,
                mtime_nanos: u32::MAX,
                hash: "h2".into(),
            },
            PersistedEntry {
                path: "/games/bad-secs".into(),
                mtime_secs: u64::MAX,
                mtime_nanos: 0,
                hash: "h3".into(),
            },
        ];
        let bytes = serde_json::to_vec(&PersistedHashCache { entries }).unwrap();
        std::fs::write(&file, bytes).unwrap();
        let loaded = load_blocking(&file);
        assert_eq!(loaded.len(), 1);
        assert!(loaded.contains_key(Path::new("/games/ok")));
    }

    #[test]
    fn oversized_file_truncates_to_cap() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cache.json");
        let entries: Vec<PersistedEntry> = (0..HASH_CACHE_MAX_ENTRIES + 100)
            .map(|i| PersistedEntry {
                path: format!("/games/f{i}"),
                mtime_secs: 1,
                mtime_nanos: 0,
                hash: "h".into(),
            })
            .collect();
        let bytes = serde_json::to_vec(&PersistedHashCache { entries }).unwrap();
        std::fs::write(&file, bytes).unwrap();
        assert_eq!(load_blocking(&file).len(), HASH_CACHE_MAX_ENTRIES);
    }
}
