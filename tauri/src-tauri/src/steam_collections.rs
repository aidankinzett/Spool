//! Steam library collections — maintaining a "Spool" collection.
//!
//! Steam stores library collections in
//! `<steam>/userdata/<uid>/config/cloudstorage/cloud-storage-namespace-1.json`.
//! The file is a JSON array of `[key, record]` pairs (a serialised JS Map).
//! Each collection is keyed `user-collections.<id>`; its record's `value` field
//! is a *stringified* JSON object `{ id, name, added: [appid], removed: [] }`.
//! Non-Steam shortcuts are referenced in `added` by the same CRC32-based appid
//! `steam.rs` stamps into `shortcuts.vdf`.
//!
//! We keep one collection (`spool-managed`, displayed as "Spool") in sync with
//! the set of Spool-managed game shortcuts. Steam merges collections across
//! devices by union (`strMethodId: "union-collections"`), so our additions
//! survive cloud sync even against a stale `version`.
//!
//! The same caveat as `shortcuts.vdf` applies: Steam owns this file in memory
//! and rewrites it on sync/exit, so a write lands reliably on the next Steam
//! restart (which is already required for a freshly added shortcut to appear).
//!
//! Once the collection exists, users see it natively in Steam's library, and
//! TabMaster can surface it as a dedicated tab via its Collection filter —
//! without Spool patching Steam's UI.

use crate::error::{AppError, AppResult};
use crate::steam::{read_shortcuts, SteamUser};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use steam_shortcuts_util::shortcut::ShortcutOwned;

/// Stable id for our collection. The `id` inside the record's `value` must equal
/// this, and the record's key is `user-collections.<id>`.
const SPOOL_COLLECTION_ID: &str = "spool-managed";
/// Display name shown in Steam's library / TabMaster.
const SPOOL_COLLECTION_NAME: &str = "Spool";

fn collection_key() -> String {
    format!("user-collections.{SPOOL_COLLECTION_ID}")
}

/// The decoded inner `value` object of a collection record.
#[derive(serde::Serialize, serde::Deserialize)]
struct CollectionValue {
    id: String,
    name: String,
    #[serde(default)]
    added: Vec<i64>,
    #[serde(default)]
    removed: Vec<i64>,
}

/// Converts a non-Steam shortcut appid (`u32`, high bit set) to the integer form
/// Steam stores in a collection's `added` array — the unsigned 32-bit value.
///
/// Isolated here on purpose: this is the one representation that needs empirical
/// confirmation against a real `cloud-storage-namespace-1.json`. If Steam turns
/// out to store the signed-32-bit form, this is the only line to change
/// (`shortcut_app_id as i32 as i64`).
fn collection_appid(shortcut_app_id: u32) -> i64 {
    shortcut_app_id as i64
}

/// Appids of Spool-managed *game* shortcuts. A shortcut is a Spool game when its
/// launch options drive our `--run` workflow (set by
/// [`crate::steam::build_launch_options`]). The bare "Spool" library-launcher
/// shortcut has empty launch options and is intentionally excluded — the
/// collection is for games, not the launcher.
fn spool_managed_appids(shortcuts: &[ShortcutOwned]) -> Vec<u32> {
    shortcuts
        .iter()
        .filter(|s| s.launch_options.starts_with("--run"))
        .map(|s| s.app_id)
        .collect()
}

/// Picks the next `version` value for a record. Steam treats these as opaque
/// sync-ordering tokens; we keep them monotonic by taking `max(previous+1, now)`
/// so our write is never ordered behind the copy we just read.
fn bump_version(existing: Option<&serde_json::Value>, now: u64) -> String {
    let prev = existing.and_then(|v| {
        v.as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| v.as_u64())
    });
    match prev {
        Some(p) => (p + 1).max(now).to_string(),
        None => now.to_string(),
    }
}

/// Upserts the Spool-managed collection into the parsed namespace file. Replaces
/// the collection's `added` set wholesale from `appids` (rebuild semantics — a
/// game removed from Steam simply drops out). Every other record is left
/// untouched, including unknown keys.
fn upsert_spool_collection(file: &mut Vec<(String, serde_json::Value)>, appids: &[u32], now: u64) {
    let key = collection_key();

    let mut added: Vec<i64> = appids.iter().copied().map(collection_appid).collect();
    added.sort_unstable();
    added.dedup();

    let value = CollectionValue {
        id: SPOOL_COLLECTION_ID.to_string(),
        name: SPOOL_COLLECTION_NAME.to_string(),
        added,
        removed: Vec::new(),
    };
    let value_str = serde_json::to_string(&value).unwrap_or_default();

    if let Some((_, record)) = file.iter_mut().find(|(k, _)| k == &key) {
        if let Some(obj) = record.as_object_mut() {
            let next_version = bump_version(obj.get("version"), now);
            obj.insert("value".into(), serde_json::Value::String(value_str));
            obj.insert("timestamp".into(), serde_json::json!(now));
            obj.insert("is_deleted".into(), serde_json::json!(false));
            obj.insert("version".into(), serde_json::Value::String(next_version));
        }
        return;
    }

    let record = serde_json::json!({
        "key": key,
        "timestamp": now,
        "value": value_str,
        "version": now.to_string(),
        "conflictResolutionMethod": "custom",
        "strMethodId": "union-collections",
        "is_deleted": false,
    });
    file.push((key, record));
}

/// Path to the user's collections namespace file (may not exist yet).
fn cloudstorage_namespace_path(user: &SteamUser) -> Option<PathBuf> {
    user.shortcuts_path
        .parent()
        .map(|config| config.join("cloudstorage").join("cloud-storage-namespace-1.json"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Serialises + writes atomically (write `.tmp`, rename), keeping a `.bak` of the
/// previous file — mirrors [`crate::steam::write_shortcuts`].
fn write_namespace_file(path: &Path, file: &[(String, serde_json::Value)]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec(file)
        .map_err(|e| AppError::Other(format!("serialise collections: {e}")))?;
    if path.is_file() {
        let _ = std::fs::copy(path, path.with_extension("json.bak"));
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Reconciles the "Spool" collection for one Steam user with the current set of
/// Spool-managed game shortcuts. Reads `shortcuts.vdf` to derive the appid set,
/// merges into the existing namespace file (or creates one), writes atomically.
pub fn sync_spool_collection(user: &SteamUser) -> AppResult<()> {
    let shortcuts = read_shortcuts(&user.shortcuts_path)?;
    let appids = spool_managed_appids(&shortcuts);

    let path = cloudstorage_namespace_path(user)
        .ok_or_else(|| AppError::Other("can't resolve cloudstorage path".into()))?;

    let mut file: Vec<(String, serde_json::Value)> = if path.is_file() {
        let bytes = std::fs::read(&path)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| AppError::Other(format!("failed to parse {}: {e}", path.display())))?
    } else {
        Vec::new()
    };

    upsert_spool_collection(&mut file, &appids, now_secs());
    write_namespace_file(&path, &file)?;
    Ok(())
}

/// Rebuilds the "Spool" collection for the most-recently-used Steam user. Tauri
/// command behind the Settings "Rebuild Spool collection" button.
#[tauri::command]
pub async fn sync_spool_steam_collection() -> AppResult<()> {
    let users = crate::steam::locate_steam_users()?;
    let user = users
        .first()
        .ok_or_else(|| AppError::Other("No Steam user accounts found".into()))?;
    sync_spool_collection(user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(s: &str) -> Vec<(String, Value)> {
        serde_json::from_str(s).unwrap()
    }

    fn spool_record(file: &[(String, Value)]) -> &Value {
        &file
            .iter()
            .find(|(k, _)| k == &collection_key())
            .expect("spool collection present")
            .1
    }

    fn spool_value(file: &[(String, Value)]) -> CollectionValue {
        let s = spool_record(file)
            .as_object()
            .unwrap()
            .get("value")
            .unwrap()
            .as_str()
            .unwrap();
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn creates_fresh_record_with_union_method() {
        let mut file = Vec::new();
        upsert_spool_collection(&mut file, &[0x8000_0001, 0x8000_0002], 1000);

        assert_eq!(file.len(), 1);
        let (key, rec) = &file[0];
        assert_eq!(key, "user-collections.spool-managed");
        let obj = rec.as_object().unwrap();
        assert_eq!(obj.get("strMethodId").unwrap(), "union-collections");
        assert_eq!(obj.get("is_deleted").unwrap(), &Value::Bool(false));

        let v = spool_value(&file);
        assert_eq!(v.id, "spool-managed");
        assert_eq!(v.name, "Spool");
        assert_eq!(v.added, vec![0x8000_0001_i64, 0x8000_0002]);
        assert!(v.removed.is_empty());
    }

    #[test]
    fn updates_existing_and_preserves_other_records() {
        let mut file = parse(
            r#"[
            ["apps.123", {"key":"apps.123","value":"keep-me","version":"5"}],
            ["user-collections.spool-managed", {"key":"user-collections.spool-managed","timestamp":1,"value":"{\"id\":\"spool-managed\",\"name\":\"Spool\",\"added\":[111],\"removed\":[]}","version":"7","conflictResolutionMethod":"custom","strMethodId":"union-collections","is_deleted":false}],
            ["user-collections.other", {"key":"user-collections.other","value":"{\"id\":\"other\",\"name\":\"Other\",\"added\":[1,2,3]}"}]
        ]"#,
        );

        upsert_spool_collection(&mut file, &[0x8000_0009], 2000);

        // Unrelated records are left exactly as they were.
        assert_eq!(file.len(), 3);
        let apps = file.iter().find(|(k, _)| k == "apps.123").unwrap();
        assert_eq!(apps.1.as_object().unwrap().get("value").unwrap(), "keep-me");
        let other = file
            .iter()
            .find(|(k, _)| k == "user-collections.other")
            .unwrap();
        assert!(other
            .1
            .as_object()
            .unwrap()
            .get("value")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("Other"));

        // Spool collection is replaced wholesale (rebuild), not appended to.
        let v = spool_value(&file);
        assert_eq!(v.added, vec![0x8000_0009_i64]);

        let rec = spool_record(&file).as_object().unwrap();
        assert_eq!(rec.get("timestamp").unwrap(), 2000);
        // Version is bumped past both the previous value (7) and `now`.
        let ver: u64 = rec
            .get("version")
            .unwrap()
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(ver >= 2000);
    }

    #[test]
    fn dedups_and_sorts_added() {
        let mut file = Vec::new();
        upsert_spool_collection(&mut file, &[0x8000_0005, 0x8000_0001, 0x8000_0005], 1);
        let v = spool_value(&file);
        assert_eq!(v.added, vec![0x8000_0001_i64, 0x8000_0005]);
    }

    #[test]
    fn value_round_trips_as_stringified_json() {
        let mut file = Vec::new();
        upsert_spool_collection(&mut file, &[0x8000_0001], 1);
        // The `value` field must be a JSON *string*, not a nested object.
        let value_field = spool_record(&file)
            .as_object()
            .unwrap()
            .get("value")
            .unwrap();
        assert!(value_field.is_string(), "value must be stringified JSON");
    }

    #[test]
    fn selects_only_run_shortcuts() {
        let mut shortcuts = Vec::new();
        // Bare launcher (no --run) — excluded.
        crate::steam::upsert_spool_shortcut(&mut shortcuts, "Spool", "spool", "/", "");
        // A game (--run) — included.
        let game_id = crate::steam::upsert_spool_shortcut(
            &mut shortcuts,
            "Hades",
            "spool",
            "/",
            "--run \"Hades\" \"h.exe\" --attached",
        );
        assert_eq!(spool_managed_appids(&shortcuts), vec![game_id]);
    }
}
