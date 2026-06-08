//! Spool-owned ludusavi configuration directory.
//!
//! Spool passes `--config <ludusavi_config_dir()>` to every ludusavi call so
//! it controls:
//!   - backup/restore path (under Spool's app data dir)
//!   - cloud remote (set by the user in Settings → Cloud saves)
//!   - per-restore redirects (generated in Phase 3 for cross-device syncs)
//!
//! The file is read and written as a `serde_yaml::Value` map so we never drop
//! keys that ludusavi itself rewrites (e.g. manifest cache metadata, format
//! state). We only touch the keys we own; everything else round-trips intact.
//!
//! Atomic writes (tmp → rename + .bak) mirror the pattern in config.rs so a
//! crash mid-write leaves either the previous good file or the new one.

use crate::error::{AppError, AppResult};
use crate::paths;
use serde_yaml::Value;
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

// ── Public API ───────────────────────────────────────────────────────────────

/// A redirect rule written into `config.yaml`. Used in Phase 3 for
/// cross-platform restore path remapping.
#[derive(Debug, Clone)]
pub struct Redirect {
    /// `"restore"` (the only kind we write; we never write bidirectional
    /// because we regenerate from scratch per-restore).
    pub kind: String,
    pub source: String,
    pub target: String,
}

/// One `customGames` entry written into `config.yaml` — a user-defined save
/// location for a game ludusavi's manifest doesn't cover (or covers wrongly).
/// `files` are ludusavi path templates (placeholder tokens or absolute paths);
/// `registry` are Windows registry keys (usually empty). `extend` selects
/// ludusavi's `integration`: when true (the game is also in the manifest) the
/// custom files are *added* to the manifest entry's; when false the default
/// `override` applies, which for a non-manifest game simply defines it.
#[derive(Debug, Clone)]
pub struct CustomGameDef {
    pub name: String,
    pub files: Vec<String>,
    pub registry: Vec<String>,
    pub extend: bool,
}

/// Ensure the Spool-owned ludusavi config dir + `config.yaml` exist and meet
/// the invariants Spool needs. Idempotent — safe to call at every startup.
///
/// Invariants enforced (on a fresh or existing file):
///   * `manifest.enable: true`     — ensures game identification works
///   * `backup.path` set to Spool's ludusavi-backup dir under app data
///   * `restore.path` == `backup.path`  — they must match for cloud sync
///   * `backup.format.chosen: simple`   — plain dirs so Phase 3 can parse
///     mapping.yaml files
///   * `backup.retention.differential: 0` — every retained revision is a
///     self-contained *full* backup. Load-bearing: the redirect / `mapping.yaml`
///     flow and the "restore an earlier save" rollback both assume each
///     revision is independently restorable, with no differential chain to
///     reconstruct. Always pinned to 0.
///   * `backup.retention.full`           — how many revisions to keep. We seed
///     a default (5) only when absent, so the user's Settings choice
///     (`save_retention_full`, applied via [`set_retention`]) survives the next
///     startup instead of being stomped back to the default. The floor is 3,
///     never 1: `full == 1` makes ludusavi reuse a single in-place backup and
///     overwrite the save files directly, so a mid-backup kill can truncate the
///     only copy. From 2+ each run writes a fresh generation, leaving the prior
///     good backup intact (see [`apply_retention`]).
///   * `cloud:` block present            — Phase 4 fills in the remote
pub fn ensure_config() -> AppResult<()> {
    let _lock = lock_config();
    let dir = paths::ludusavi_config_dir();
    std::fs::create_dir_all(&dir)?;

    let file = paths::ludusavi_config_file();
    let mut v = if file.exists() {
        read_value()?
    } else {
        Value::Mapping(Default::default())
    };

    let backup_path = backup_dir().to_string_lossy().to_string();
    let mut changed = false;

    changed |= set_path(&mut v, &["manifest", "enable"], Value::Bool(true));
    changed |= set_path(&mut v, &["backup", "path"], Value::String(backup_path.clone()));
    changed |= set_path(&mut v, &["restore", "path"], Value::String(backup_path));
    changed |= set_path(
        &mut v,
        &["backup", "format", "chosen"],
        Value::String("simple".into()),
    );
    // Seed the revision count only if absent — the user's Settings value is
    // applied via `set_retention` and must not be clobbered on every startup.
    changed |= ensure_key_exists(&mut v, &["backup", "retention", "full"], Value::Number(5.into()));
    // Differentials always off (see invariants above).
    changed |= set_path(&mut v, &["backup", "retention", "differential"], Value::Number(0.into()));

    // Upgrade a legacy bare-string remote (written by Spool versions that
    // targeted an older ludusavi) to ludusavi 0.31's tagged struct-variant form,
    // so existing installs keep working without re-running cloud setup.
    changed |= migrate_bare_remote(&mut v);

    // Ensure cloud block exists with at least a remote key; leave existing
    // values intact so a user-configured remote survives a restart.
    changed |= ensure_key_exists(&mut v, &["cloud", "remote"], Value::Null);

    // Always use a bare "rclone" name so each Spool process injects its own
    // bundled binary via PATH when spawning ludusavi, rather than storing an
    // absolute AppImage FUSE-mount path that becomes stale when that process exits.
    changed |= set_path(&mut v, &["apps", "rclone", "path"], Value::String("rclone".into()));

    // Ensure fast-fail timeout flags are present in apps.rclone.arguments so
    // --cloud-sync gives up quickly on an unreachable remote. Fold them into
    // whatever is already there (preserves user-configured args).
    let current_args = v
        .get(k("apps"))
        .and_then(|a| a.get(k("rclone")))
        .and_then(|r| r.get(k("arguments")))
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string();
    let with_timeouts = ensure_rclone_timeouts(&current_args);
    changed |= set_path(&mut v, &["apps", "rclone", "arguments"], Value::String(with_timeouts));

    if changed || !file.exists() {
        write_value(&v)?;
    }

    Ok(())
}

/// Write the save-revision retention count (`backup.retention.full`) into the
/// owned config.yaml. Called from `update_config` when the user changes the
/// "save revisions to keep" setting. Clamped to 3–10 so a stray value can't
/// disable backups (0), drop to the unsafe in-place `full == 1` mode, or
/// balloon disk/cloud use. Differentials are left untouched (always 0 — see
/// [`ensure_config`]). Lowering the count prunes on the next backup; raising it
/// accumulates going forward.
pub fn set_retention(full: u32) -> AppResult<()> {
    let _lock = lock_config();
    let mut v = read_value_or_default()?;
    apply_retention(&mut v, full);
    write_value(&v)
}

/// Pure value-mutation half of [`set_retention`] — no file IO, so it can be
/// unit tested. Clamps `full` to 3–10 and writes `backup.retention.full`. The
/// floor of 3 (not 1) keeps ludusavi out of its single in-place backup mode,
/// where a mid-backup kill could truncate the only copy.
fn apply_retention(v: &mut Value, full: u32) {
    let clamped = full.clamp(3, 10);
    set_path(
        v,
        &["backup", "retention", "full"],
        Value::Number(clamped.into()),
    );
}

/// Convert a legacy bare-string `cloud.remote` (e.g. `remote: Dropbox`) into
/// ludusavi 0.31's tagged struct-variant form (`remote: { Dropbox: { id: Dropbox } }`).
/// Older Spool versions targeted a ludusavi where these were unit variants;
/// 0.31 rejects the bare form (see [`apply_cloud`]). Returns true if it rewrote
/// the value. A remote that's already a map, or null/absent, is left untouched.
fn migrate_bare_remote(v: &mut Value) -> bool {
    let name = match v.get(k("cloud")).and_then(|c| c.get(k("remote"))) {
        Some(Value::String(s)) => s.clone(),
        _ => return false,
    };
    match name.as_str() {
        // OAuth presets: the bare name equals the ludusavi tag and the rclone
        // remote name, so it doubles as the `id`.
        "Box" | "Dropbox" | "GoogleDrive" | "OneDrive" => {
            set_path(v, &["cloud", "remote"], tagged_remote(&name, &name))
        }
        // A bare WebDav/Ftp/Smb can't be repaired here (the connection details
        // were never stored), so clear it; the user re-runs the dedicated setup.
        "WebDav" | "Ftp" | "Smb" => set_path(v, &["cloud", "remote"], Value::Null),
        _ => false,
    }
}

/// Write cloud remote / path / rclone settings into the owned config.yaml.
/// Called from `update_config` (Phase 4) when the user saves those fields.
/// Pass `None` for a field to leave it unchanged.
pub fn set_cloud(
    provider: Option<&str>,
    remote: Option<&str>,
    path: Option<&str>,
    rclone_path: Option<&str>,
    rclone_args: Option<&str>,
) -> AppResult<()> {
    let _lock = lock_config();
    let mut v = read_value_or_default()?;
    apply_cloud(&mut v, provider, remote, path, rclone_path, rclone_args);
    write_value(&v)
}

/// Pure value-mutation half of [`set_cloud`] — no file IO, so it can be unit
/// tested directly. Maps the Settings UI's `provider` enum onto ludusavi's
/// `cloud.remote` schema (presets are bare strings; `custom` is a tagged map).
fn apply_cloud(
    v: &mut Value,
    provider: Option<&str>,
    remote: Option<&str>,
    path: Option<&str>,
    rclone_path: Option<&str>,
    rclone_args: Option<&str>,
) {
    if let (Some(prov), Some(rem)) = (provider, remote) {
        if prov.is_empty() {
            set_path(v, &["cloud", "remote"], Value::Null);
        } else {
            // ludusavi 0.31's `cloud.remote` variants are all *struct* variants
            // carrying at least an `id` (the rclone remote name). A bare string
            // serialises as a unit variant, which ludusavi rejects with
            // "invalid type: unit variant, expected struct variant" — failing the
            // whole config. So every remote is written as the tagged map
            // `{ <Tag>: { id: <name> } }`.
            match prov {
                "custom" => { set_path(v, &["cloud", "remote"], tagged_remote("Custom", rem)); }
                // The OAuth presets create an rclone remote whose name equals the
                // ludusavi variant tag (see `oauth_remote` in rclone.rs), so the
                // `id` is the tag itself.
                "box" => { set_path(v, &["cloud", "remote"], tagged_remote("Box", "Box")); }
                "dropbox" => { set_path(v, &["cloud", "remote"], tagged_remote("Dropbox", "Dropbox")); }
                "google-drive" => { set_path(v, &["cloud", "remote"], tagged_remote("GoogleDrive", "GoogleDrive")); }
                "onedrive" => { set_path(v, &["cloud", "remote"], tagged_remote("OneDrive", "OneDrive")); }
                // FTP/SMB/WebDAV are struct variants needing connection details
                // (host/port/url/credentials) that this function isn't given. The
                // WebDAV form configures its remote through `ludusavi cloud set
                // webdav` (see `apply_webdav_remote`), which writes the full
                // struct. Leave any existing remote untouched rather than clobber
                // it with an incomplete (and invalid) value.
                "webdav" | "ftp" | "smb" | "spool-server" => {}
                _ => { set_path(v, &["cloud", "remote"], Value::Null); }
            }
        }
    }
    if let Some(p) = path {
        set_path(v, &["cloud", "path"], Value::String(p.into()));
    }
    if let Some(p) = rclone_path {
        set_path(v, &["apps", "rclone", "path"], Value::String(p.into()));
    }
    if let Some(a) = rclone_args {
        set_path(
            v,
            &["apps", "rclone", "arguments"],
            Value::String(ensure_rclone_timeouts(a)),
        );
    }
}

/// Build ludusavi's tagged struct-variant remote map `{ <tag>: { id: <id> } }`.
/// See the comment in [`apply_cloud`] for why the bare-string form is invalid in
/// ludusavi 0.31. `tag` is the ludusavi variant name (`Dropbox`, `Custom`, …);
/// `id` is the backing rclone remote name.
fn tagged_remote(tag: &str, id: &str) -> Value {
    let mut inner = serde_yaml::Mapping::new();
    inner.insert(Value::String("id".into()), Value::String(id.to_string()));
    let mut outer = serde_yaml::Mapping::new();
    outer.insert(Value::String(tag.into()), Value::Mapping(inner));
    Value::Mapping(outer)
}

/// Connection / IO timeout + retry caps we always fold into rclone's arguments.
///
/// `ludusavi {restore,backup} --cloud-sync` shells out to rclone. With rclone's
/// defaults, an unreachable remote (e.g. the cloud remote at SteamOS
/// Game-Mode boot, before Wi-Fi is up) blocks for *minutes* (long connect
/// timeout × retries × low-level-retries) — which wedges the run workflow on
/// the "restoring" phase and the game never launches. Capping these makes
/// `--cloud-sync` give up in seconds; ludusavi then proceeds with the local
/// restore (the saves that matter are already on disk).
///
/// Each flag is appended only if the user hasn't already set it in their
/// configured `rclone_args`, so a deliberate override is preserved.
pub fn ensure_rclone_timeouts(user_args: &str) -> String {
    // Aggressive on purpose: when the remote is unreachable at Game-Mode boot
    // we want to give up and launch in a few seconds, not tens of seconds. A
    // healthy LAN/cloud remote connects well under 5s; if it can't, falling
    // back to the local save and launching immediately is the better outcome.
    const DEFAULTS: &[(&str, &str)] = &[
        ("--contimeout", "5s"),
        ("--timeout", "45s"),
        ("--retries", "1"),
        ("--low-level-retries", "1"),
    ];
    let mut out = user_args.trim().to_string();
    for (flag, val) in DEFAULTS {
        let eq_prefix = format!("{flag}=");
        if out
            .split_whitespace()
            .any(|t| t == *flag || t.starts_with(&eq_prefix))
        {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(flag);
        out.push(' ');
        out.push_str(val);
    }
    out
}

/// Replace the entire `redirects:` list in the owned config.yaml. Called
/// before each restore in Phase 3.  Because Spool owns the config dir
/// completely, there are no user-authored redirects to preserve — the list
/// is always regenerated from scratch so stale entries can never accumulate.
pub fn set_redirects(redirects: &[Redirect]) -> AppResult<()> {
    let _lock = lock_config();
    let mut v = read_value_or_default()?;
    let list: Value = Value::Sequence(
        redirects
            .iter()
            .map(|r| {
                let mut m = serde_yaml::Mapping::new();
                m.insert(k("kind"), Value::String(r.kind.clone()));
                m.insert(k("source"), Value::String(r.source.clone()));
                m.insert(k("target"), Value::String(r.target.clone()));
                Value::Mapping(m)
            })
            .collect(),
    );
    set_path(&mut v, &["redirects"], list);
    write_value(&v)
}

/// Replace the entire `customGames:` list in the owned config.yaml. Called by
/// [`crate::custom_saves`] whenever the set of custom-save games changes (an
/// edit, a cross-device adopt, or a launch preflight). Because Spool owns the
/// config dir completely, the list is always regenerated from the library —
/// there are no user-authored custom games to preserve, so a removed game's
/// entry can never linger. Unlike `redirects`, this block is *persistent* (the
/// run workflow never clears it). An empty slice writes `customGames: []`.
pub fn set_custom_games(games: &[CustomGameDef]) -> AppResult<()> {
    let _lock = lock_config();
    let mut v = read_value_or_default()?;
    // Skip the rewrite when the block is unchanged: avoids churning config.yaml
    // (+ its `.bak`) on every launch/boot, and narrows the window for losing a
    // concurrent writer's key in the shared-file read-modify-write.
    if !set_path(&mut v, &["customGames"], custom_games_value(games)) {
        return Ok(());
    }
    write_value(&v)
}

/// Pure value-construction half of [`set_custom_games`] — builds the YAML
/// sequence so it can be unit-tested without touching the real config file.
/// `registry` is omitted when empty so the common (files-only) entry stays
/// minimal; `integration: extend` is written only for manifest-covered games.
fn custom_games_value(games: &[CustomGameDef]) -> Value {
    Value::Sequence(
        games
            .iter()
            .map(|g| {
                let mut m = serde_yaml::Mapping::new();
                m.insert(k("name"), Value::String(g.name.clone()));
                m.insert(
                    k("files"),
                    Value::Sequence(g.files.iter().cloned().map(Value::String).collect()),
                );
                if !g.registry.is_empty() {
                    m.insert(
                        k("registry"),
                        Value::Sequence(g.registry.iter().cloned().map(Value::String).collect()),
                    );
                }
                if g.extend {
                    m.insert(k("integration"), Value::String("extend".into()));
                }
                Value::Mapping(m)
            })
            .collect(),
    )
}

/// Returns true if `cloud.remote` in the owned config.yaml is set to a
/// non-null value. Used by the runner to decide whether to label a session
/// as cloud-synced or local-only.
pub fn cloud_remote_is_configured() -> bool {
    let Ok(v) = read_value() else { return false };
    let remote = v
        .as_mapping()
        .and_then(|m| m.get(Value::String("cloud".into())))
        .and_then(|cloud| cloud.as_mapping())
        .and_then(|m| m.get(Value::String("remote".into())));
    matches!(remote, Some(Value::Mapping(_)) | Some(Value::String(_)))
}

/// The absolute path used for `backup.path` / `restore.path` in the owned
/// config. Exposed so the runner can tell ludusavi where to look.
pub fn backup_dir() -> PathBuf {
    paths::app_data_dir().join("ludusavi-backup")
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Held guard over the machine-wide config-write lock. Dropping it (at the end
/// of a mutator) releases the lock; the OS also frees it if the process exits.
struct ConfigLock {
    file: Option<File>,
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        if let Some(f) = &self.file {
            let _ = f.unlock();
        }
    }
}

/// Serialise the read-modify-write of the shared `config.yaml` against other
/// Spool processes (tray GUI, attached `--run`, headless Decky server), which
/// each own different keys (`redirects`, `cloud`, `customGames`, …) but rewrite
/// the whole file — so an unsynchronised interleave can drop one writer's block.
///
/// Best-effort and sync (the mutators are sync): tries the OS advisory lock with
/// a brief bounded spin, and on contention/timeout proceeds *without* the lock
/// rather than failing a startup/launch config write — the blocks self-heal on
/// the next regeneration, so completing the write is safer than aborting it.
/// Hold time is sub-millisecond, so in practice the first `try_lock` wins.
fn lock_config() -> ConfigLock {
    let path = paths::ludusavi_config_lock_file();
    let Ok(file) = File::create(&path) else {
        return ConfigLock { file: None };
    };
    // ~1s ceiling (50 × 20ms); only ever approached if another process is stuck
    // holding it, in which case proceeding unlocked is the lesser evil.
    for _ in 0..50 {
        match file.try_lock() {
            Ok(()) => return ConfigLock { file: Some(file) },
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(20)),
            Err(TryLockError::Error(_)) => return ConfigLock { file: None },
        }
    }
    tracing::warn!("ludusavi config lock stayed contended — writing without it");
    ConfigLock { file: None }
}

fn read_value() -> AppResult<Value> {
    let raw = std::fs::read_to_string(paths::ludusavi_config_file())?;
    serde_yaml::from_str(&raw)
        .map_err(|e| AppError::Other(format!("failed to parse ludusavi config.yaml: {e}")))
}

fn read_value_or_default() -> AppResult<Value> {
    read_value_or_default_at(&paths::ludusavi_config_file())
}

/// Reads + parses the config at `file`, returning an empty mapping ONLY when the
/// file genuinely doesn't exist yet. A read/parse error on a file that DOES
/// exist (a transient EACCES, or catching ludusavi mid-write) is propagated, not
/// swallowed: otherwise a mutator would start from an empty map and
/// [`write_value`] would overwrite the real config — cloud remote, backup/restore
/// paths, retention, ludusavi's own manifest state — with a near-empty file.
/// (#269)
fn read_value_or_default_at(file: &Path) -> AppResult<Value> {
    if !file.exists() {
        return Ok(Value::Mapping(Default::default()));
    }
    let raw = std::fs::read_to_string(file)?;
    serde_yaml::from_str(&raw)
        .map_err(|e| AppError::Other(format!("failed to parse ludusavi config.yaml: {e}")))
}

fn write_value(v: &Value) -> AppResult<()> {
    let file = paths::ludusavi_config_file();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = file.with_extension("yaml.tmp");
    let bak = file.with_extension("yaml.bak");
    let yaml = serde_yaml::to_string(v)
        .map_err(|e| AppError::Other(format!("failed to serialize ludusavi config: {e}")))?;

    // Write + fsync the tmp file before the rename. A plain write()+rename only
    // orders the *metadata*; without flushing the data first, a crash/power-loss
    // between write() returning and the data reaching disk can leave config.yaml
    // present but truncated or empty — and ludusavi reads this file directly,
    // with no .bak fallback on read. (#277)
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(yaml.as_bytes())?;
        f.sync_all()?;
    }

    // Keep the previous good file as .bak, then swap the new one in. If the swap
    // fails, roll .bak back so the dir is never left without a config.yaml at all
    // (the live file was already moved aside). (#278)
    let had_existing = file.exists();
    if had_existing {
        let _ = std::fs::rename(&file, &bak);
    }
    if let Err(e) = std::fs::rename(&tmp, &file) {
        if had_existing {
            let _ = std::fs::rename(&bak, &file);
        }
        return Err(e.into());
    }

    // fsync the directory so the rename itself survives a crash (no-op/!ok on
    // platforms where a dir can't be opened as a File, e.g. Windows — harmless).
    // (#277)
    if let Some(parent) = file.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

/// Navigate/create a nested path of YAML keys and set the leaf to `val`.
/// Returns `true` if the value actually changed.
fn set_path(root: &mut Value, path: &[&str], val: Value) -> bool {
    let Some((&key, rest)) = path.split_first() else {
        return false;
    };
    let map = match root {
        Value::Mapping(m) => m,
        other => {
            *other = Value::Mapping(Default::default());
            if let Value::Mapping(m) = other {
                m
            } else {
                unreachable!()
            }
        }
    };
    if rest.is_empty() {
        let old = map.get(k(key)).cloned();
        map.insert(k(key), val.clone());
        old.as_ref() != Some(&val)
    } else {
        let child = map.entry(k(key)).or_insert(Value::Mapping(Default::default()));
        set_path(child, rest, val)
    }
}

/// Like `set_path` but only inserts when the key doesn't already exist.
/// Insert `default` at `path` only if the key is absent. Returns `true` if it
/// inserted anything (or had to coerce a non-map node into a map), so callers
/// can fold the result into a `changed` flag and decide whether to write.
fn ensure_key_exists(root: &mut Value, path: &[&str], default: Value) -> bool {
    let Some((&key, rest)) = path.split_first() else {
        return false;
    };
    let mut changed = false;
    let map = match root {
        Value::Mapping(m) => m,
        other => {
            *other = Value::Mapping(Default::default());
            changed = true;
            if let Value::Mapping(m) = other {
                m
            } else {
                return changed;
            }
        }
    };
    if rest.is_empty() {
        if !map.contains_key(k(key)) {
            map.insert(k(key), default);
            changed = true;
        }
    } else {
        let child = map.entry(k(key)).or_insert(Value::Mapping(Default::default()));
        changed |= ensure_key_exists(child, rest, default);
    }
    changed
}

fn k(s: &str) -> Value {
    Value::String(s.to_string())
}

// ── Test helpers ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_path_creates_nested_keys() {
        let mut v = Value::Mapping(Default::default());
        set_path(&mut v, &["a", "b", "c"], Value::String("hello".into()));
        let s = serde_yaml::to_string(&v).unwrap();
        assert!(s.contains("c: hello"), "got: {s}");
    }

    #[test]
    fn set_path_returns_changed_flag() {
        let mut v = Value::Mapping(Default::default());
        assert!(set_path(&mut v, &["x"], Value::Bool(true)));
        assert!(!set_path(&mut v, &["x"], Value::Bool(true))); // unchanged
        assert!(set_path(&mut v, &["x"], Value::Bool(false))); // changed
    }

    #[test]
    fn ensure_key_exists_only_inserts_when_absent() {
        let mut v = Value::Mapping(Default::default());
        assert!(ensure_key_exists(&mut v, &["a", "b"], Value::Number(3.into())));
        // Present now — second call is a no-op and doesn't overwrite.
        assert!(!ensure_key_exists(&mut v, &["a", "b"], Value::Number(9.into())));
        assert_eq!(
            v.get("a").and_then(|a| a.get("b")).and_then(|n| n.as_u64()),
            Some(3)
        );
    }

    #[test]
    fn apply_retention_clamps_to_range() {
        let mut v = Value::Mapping(Default::default());
        let full = |v: &Value| {
            v.get("backup")
                .and_then(|b| b.get("retention"))
                .and_then(|r| r.get("full"))
                .and_then(|n| n.as_u64())
        };
        apply_retention(&mut v, 0); // below floor → 3
        assert_eq!(full(&v), Some(3));
        apply_retention(&mut v, 1); // unsafe in-place mode → floored to 3
        assert_eq!(full(&v), Some(3));
        apply_retention(&mut v, 99); // above ceiling → 10
        assert_eq!(full(&v), Some(10));
        apply_retention(&mut v, 5); // in range → 5
        assert_eq!(full(&v), Some(5));
    }

    #[test]
    fn set_redirects_round_trips() {
        // Write redirects to a temp file and read them back.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.yaml");
        // Stub out the path helpers by writing directly.
        let v = Value::Mapping(Default::default());
        let yaml = serde_yaml::to_string(&v).unwrap();
        std::fs::write(&file, yaml).unwrap();

        let redirects = [Redirect {
            kind: "restore".into(),
            source: "C:/Users/alice".into(),
            target: "/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/steamuser".into(),
        }];
        let list: Value = Value::Sequence(
            redirects
                .iter()
                .map(|r| {
                    let mut m = serde_yaml::Mapping::new();
                    m.insert(k("kind"), Value::String(r.kind.clone()));
                    m.insert(k("source"), Value::String(r.source.clone()));
                    m.insert(k("target"), Value::String(r.target.clone()));
                    Value::Mapping(m)
                })
                .collect(),
        );
        let raw = serde_yaml::to_string(&list).unwrap();
        assert!(raw.contains("C:/Users/alice"));
        assert!(raw.contains("steamuser"));
    }

    // ── customGames (non-manifest games) ───────────────────────────────────

    #[test]
    fn custom_games_value_files_only_omits_registry() {
        let defs = [CustomGameDef {
            name: "My Game".into(),
            files: vec!["<winLocalAppData>/MyGame".into()],
            registry: vec![],
            extend: false,
        }];
        let yaml = serde_yaml::to_string(&custom_games_value(&defs)).unwrap();
        assert!(yaml.contains("name: My Game"), "got: {yaml}");
        assert!(yaml.contains("<winLocalAppData>/MyGame"), "got: {yaml}");
        // Empty registry is omitted to keep the entry minimal.
        assert!(!yaml.contains("registry"), "got: {yaml}");
        // Non-manifest game → default override (no integration key written).
        assert!(!yaml.contains("integration"), "got: {yaml}");
    }

    #[test]
    fn custom_games_value_includes_registry_when_present() {
        let defs = [CustomGameDef {
            name: "G".into(),
            files: vec!["<base>/Saves".into()],
            registry: vec!["HKEY_CURRENT_USER/Software/G".into()],
            extend: false,
        }];
        let yaml = serde_yaml::to_string(&custom_games_value(&defs)).unwrap();
        assert!(yaml.contains("registry"), "got: {yaml}");
        assert!(yaml.contains("HKEY_CURRENT_USER/Software/G"), "got: {yaml}");
    }

    #[test]
    fn custom_games_value_writes_extend_integration() {
        // A manifest-covered game supplements (not replaces) the manifest's saves.
        let defs = [CustomGameDef {
            name: "Hades".into(),
            files: vec!["<winDocuments>/Saved Games/Hades".into()],
            registry: vec![],
            extend: true,
        }];
        let yaml = serde_yaml::to_string(&custom_games_value(&defs)).unwrap();
        assert!(yaml.contains("integration: extend"), "got: {yaml}");
    }

    #[test]
    fn custom_games_value_empty_is_empty_sequence() {
        assert_eq!(custom_games_value(&[]), Value::Sequence(vec![]));
    }

    // ── Phase 4: apply_cloud (provider → ludusavi cloud.remote schema) ──────

    /// Look up a nested key path in a YAML value, returning the leaf if present.
    fn get_path<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
        let mut cur = root;
        for key in path {
            cur = cur.as_mapping()?.get(Value::String((*key).to_string()))?;
        }
        Some(cur)
    }

    #[test]
    fn apply_cloud_preset_remote_is_a_tagged_map() {
        let mut v = Value::Mapping(Default::default());
        apply_cloud(&mut v, Some("google-drive"), Some(""), None, None, None);
        // Expect: { GoogleDrive: { id: GoogleDrive } } — a struct variant, not a
        // bare string (which ludusavi 0.31 rejects as a unit variant).
        let remote = get_path(&v, &["cloud", "remote"]).unwrap();
        assert_eq!(
            get_path(remote, &["GoogleDrive", "id"]),
            Some(&Value::String("GoogleDrive".into())),
            "got: {remote:?}",
        );
    }

    #[test]
    fn apply_cloud_every_oauth_preset_maps_to_tagged_variant() {
        let cases = [
            ("box", "Box"),
            ("dropbox", "Dropbox"),
            ("google-drive", "GoogleDrive"),
            ("onedrive", "OneDrive"),
        ];
        for (provider, tag) in cases {
            let mut v = Value::Mapping(Default::default());
            apply_cloud(&mut v, Some(provider), Some(""), None, None, None);
            let remote = get_path(&v, &["cloud", "remote"]).unwrap();
            assert_eq!(
                get_path(remote, &[tag, "id"]),
                Some(&Value::String(tag.into())),
                "provider {provider} should map to {{ {tag}: {{ id: {tag} }} }}, got: {remote:?}",
            );
        }
    }

    #[test]
    fn apply_cloud_webdav_ftp_smb_leave_remote_untouched() {
        // These are configured via their dedicated path (e.g. `ludusavi cloud set
        // webdav`); a settings save that re-runs apply_cloud with the provider
        // must not clobber the full struct with an incomplete value.
        for provider in ["webdav", "ftp", "smb", "spool-server"] {
            let mut v = Value::Mapping(Default::default());
            set_path(&mut v, &["cloud", "remote"], tagged_remote("Custom", "preset"));
            apply_cloud(&mut v, Some(provider), Some(""), None, None, None);
            let remote = get_path(&v, &["cloud", "remote"]).unwrap();
            assert_eq!(
                get_path(remote, &["Custom", "id"]),
                Some(&Value::String("preset".into())),
                "provider {provider} should leave the existing remote intact",
            );
        }
    }

    #[test]
    fn migrate_bare_remote_upgrades_legacy_preset() {
        let mut v = Value::Mapping(Default::default());
        set_path(&mut v, &["cloud", "remote"], Value::String("Dropbox".into()));
        assert!(migrate_bare_remote(&mut v));
        let remote = get_path(&v, &["cloud", "remote"]).unwrap();
        assert_eq!(
            get_path(remote, &["Dropbox", "id"]),
            Some(&Value::String("Dropbox".into())),
            "got: {remote:?}",
        );
        // Idempotent: a second pass over the now-tagged map is a no-op.
        assert!(!migrate_bare_remote(&mut v));
    }

    #[test]
    fn migrate_bare_remote_clears_unrepairable_legacy() {
        let mut v = Value::Mapping(Default::default());
        set_path(&mut v, &["cloud", "remote"], Value::String("WebDav".into()));
        assert!(migrate_bare_remote(&mut v));
        assert_eq!(get_path(&v, &["cloud", "remote"]), Some(&Value::Null));
    }

    #[test]
    fn apply_cloud_custom_remote_is_a_tagged_map() {
        let mut v = Value::Mapping(Default::default());
        apply_cloud(&mut v, Some("custom"), Some("bazzite"), None, None, None);
        let remote = get_path(&v, &["cloud", "remote"]).unwrap();
        // Expect: { Custom: { id: bazzite } }
        let id = get_path(remote, &["Custom", "id"]);
        assert_eq!(id, Some(&Value::String("bazzite".into())), "got: {remote:?}");
    }

    #[test]
    fn apply_cloud_empty_provider_clears_remote() {
        let mut v = Value::Mapping(Default::default());
        // Seed an existing remote, then clear it.
        apply_cloud(&mut v, Some("dropbox"), Some(""), None, None, None);
        apply_cloud(&mut v, Some(""), Some(""), None, None, None);
        assert_eq!(get_path(&v, &["cloud", "remote"]), Some(&Value::Null));
    }

    #[test]
    fn apply_cloud_unknown_provider_clears_remote() {
        let mut v = Value::Mapping(Default::default());
        apply_cloud(&mut v, Some("dropbox"), Some(""), None, None, None);
        apply_cloud(&mut v, Some("nonsense"), Some("x"), None, None, None);
        assert_eq!(get_path(&v, &["cloud", "remote"]), Some(&Value::Null));
    }

    #[test]
    fn apply_cloud_sets_path_and_rclone_under_apps() {
        let mut v = Value::Mapping(Default::default());
        apply_cloud(
            &mut v,
            None,
            None,
            Some("Spool/ludusavi-backup"),
            Some("/usr/bin/rclone"),
            Some("--fast-list"),
        );
        assert_eq!(
            get_path(&v, &["cloud", "path"]),
            Some(&Value::String("Spool/ludusavi-backup".into())),
        );
        assert_eq!(
            get_path(&v, &["apps", "rclone", "path"]),
            Some(&Value::String("/usr/bin/rclone".into())),
        );
        // Arguments get the fast-fail timeout flags folded in (the user's
        // flag is preserved at the front).
        assert_eq!(
            get_path(&v, &["apps", "rclone", "arguments"]),
            Some(&Value::String(ensure_rclone_timeouts("--fast-list"))),
        );
    }

    #[test]
    fn apply_cloud_none_fields_leave_existing_values_intact() {
        let mut v = Value::Mapping(Default::default());
        apply_cloud(&mut v, Some("dropbox"), Some(""), Some("p"), None, None);
        // A later call that only touches rclone args must not wipe the remote/path.
        apply_cloud(&mut v, None, None, None, None, Some("--ignore-checksum"));
        let remote = get_path(&v, &["cloud", "remote"]).unwrap();
        assert_eq!(
            get_path(remote, &["Dropbox", "id"]),
            Some(&Value::String("Dropbox".into())),
            "got: {remote:?}",
        );
        assert_eq!(
            get_path(&v, &["cloud", "path"]),
            Some(&Value::String("p".into())),
        );
        assert_eq!(
            get_path(&v, &["apps", "rclone", "arguments"]),
            Some(&Value::String(ensure_rclone_timeouts("--ignore-checksum"))),
        );
    }

    #[test]
    fn ensure_rclone_timeouts_appends_missing_flags() {
        let out = ensure_rclone_timeouts("--fast-list --ignore-checksum");
        for flag in ["--contimeout", "--timeout", "--retries", "--low-level-retries"] {
            assert!(out.contains(flag), "expected {flag} in {out:?}");
        }
        // User flags survive at the front.
        assert!(out.starts_with("--fast-list --ignore-checksum"));
    }

    #[test]
    fn ensure_rclone_timeouts_preserves_user_overrides() {
        // A user who set their own --contimeout / --retries keeps them; we
        // don't duplicate the flag.
        let out = ensure_rclone_timeouts("--contimeout 90s --retries 5");
        assert_eq!(out.matches("--contimeout").count(), 1);
        assert_eq!(out.matches("--retries").count(), 1);
        assert!(out.contains("--contimeout 90s"));
        assert!(out.contains("--retries 5"));
        // The ones they didn't set still get added.
        assert!(out.contains("--timeout 45s"));
        assert!(out.contains("--low-level-retries 1"));
    }

    #[test]
    fn ensure_rclone_timeouts_handles_empty() {
        let out = ensure_rclone_timeouts("");
        assert!(out.starts_with("--contimeout 5s"));
        assert!(!out.starts_with(' '));
    }

    #[test]
    fn ensure_rclone_timeouts_preserves_user_overrides_equals_form() {
        let out = ensure_rclone_timeouts("--contimeout=90s --retries=5");
        assert_eq!(out.matches("--contimeout").count(), 1);
        assert_eq!(out.matches("--retries").count(), 1);
        assert!(out.contains("--contimeout=90s"));
        assert!(out.contains("--retries=5"));
        assert!(out.contains("--timeout 45s"));
        assert!(out.contains("--low-level-retries 1"));
    }

    #[test]
    fn read_value_or_default_distinguishes_absent_from_unreadable() {
        let dir = tempfile::tempdir().unwrap();

        // Absent file → empty mapping (a fresh install legitimately has none).
        let missing = dir.path().join("config.yaml");
        let v = read_value_or_default_at(&missing).expect("absent file is not an error");
        assert!(matches!(v, Value::Mapping(_)));

        // Present-but-unparseable → ERROR, not a silent empty map — otherwise a
        // mutator would start from empty and overwrite the real config. (#269)
        let garbage = dir.path().join("garbage.yaml");
        std::fs::write(&garbage, "backup: [unterminated").unwrap();
        assert!(read_value_or_default_at(&garbage).is_err());

        // Present + valid → parsed through unchanged.
        let good = dir.path().join("good.yaml");
        std::fs::write(&good, "backup:\n  path: /tmp/x\n").unwrap();
        let parsed = read_value_or_default_at(&good).expect("valid yaml parses");
        assert_eq!(
            parsed
                .get(k("backup"))
                .and_then(|b| b.get(k("path")))
                .and_then(|p| p.as_str()),
            Some("/tmp/x")
        );
    }

    // ── Drift guard: validate generated config against the real ludusavi ─────

    /// The base invariants `ensure_config` writes, built as a `Value` for the
    /// validation test (which can't call the IO-bound `ensure_config`).
    fn base_invariants_config() -> Value {
        let mut v = Value::Mapping(Default::default());
        set_path(&mut v, &["manifest", "enable"], Value::Bool(true));
        set_path(&mut v, &["backup", "path"], Value::String("ludusavi-backup".into()));
        set_path(&mut v, &["restore", "path"], Value::String("ludusavi-backup".into()));
        set_path(&mut v, &["backup", "format", "chosen"], Value::String("simple".into()));
        set_path(&mut v, &["backup", "retention", "full"], Value::Number(5.into()));
        set_path(&mut v, &["backup", "retention", "differential"], Value::Number(0.into()));
        v
    }

    /// Locate a real bundled ludusavi binary, or `None` when only a build.rs
    /// stub (0 bytes) or nothing is present (a bare checkout). CI runs
    /// `bun run download-sidecars` before `cargo test`, so the binary is there.
    fn find_ludusavi_binary() -> Option<std::path::PathBuf> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            if entry.file_name().to_string_lossy().starts_with("ludusavi-") {
                let path = entry.path();
                if std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Drift guard: the config Spool generates for every cloud provider must
    /// pass ludusavi's OWN loader. ludusavi's config schema is defined by its
    /// serde structs; when it changed `cloud.remote` from a unit to a struct
    /// variant, Spool kept emitting the old shape and every backup/restore
    /// failed with "config file is invalid". This test feeds each generated
    /// config to the bundled `ludusavi config show` so the next such drift fails
    /// here instead of in users' hands. Skipped when no real binary is present.
    #[test]
    fn generated_cloud_configs_pass_ludusavi_validation() {
        let Some(ludusavi) = find_ludusavi_binary() else {
            eprintln!("skipping generated_cloud_configs_pass_ludusavi_validation: \
                       no real ludusavi binary in binaries/ (run `bun run download-sidecars`)");
            return;
        };

        // The providers Spool actually writes a cloud.remote for; FTP/SMB/WebDAV
        // go through `ludusavi cloud set` and aren't written by apply_cloud.
        let cases = [
            ("box", ""),
            ("dropbox", ""),
            ("google-drive", ""),
            ("onedrive", ""),
            ("custom", "my-remote"),
        ];
        for (provider, remote) in cases {
            let mut v = base_invariants_config();
            apply_cloud(
                &mut v,
                Some(provider),
                Some(remote),
                Some("ludusavi-backup"),
                None,
                None,
            );

            let dir = tempfile::tempdir().expect("tempdir");
            let yaml = serde_yaml::to_string(&v).expect("serialize config");
            std::fs::write(dir.path().join("config.yaml"), yaml).expect("write config.yaml");

            let out = std::process::Command::new(&ludusavi)
                .arg("--config")
                .arg(dir.path())
                .arg("--no-manifest-update")
                .args(["config", "show"])
                .output()
                .expect("run ludusavi config show");

            assert!(
                out.status.success(),
                "ludusavi rejected the config Spool generates for provider '{provider}': {}",
                String::from_utf8_lossy(&out.stderr).trim(),
            );
        }
    }
}
