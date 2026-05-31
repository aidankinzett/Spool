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
use std::path::PathBuf;

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
///     a default (3) only when absent, so the user's Settings choice
///     (`save_retention_full`, applied via [`set_retention`]) survives the next
///     startup instead of being stomped back to 3.
///   * `cloud:` block present            — Phase 4 fills in the remote
pub fn ensure_config() -> AppResult<()> {
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
    changed |= ensure_key_exists(&mut v, &["backup", "retention", "full"], Value::Number(3.into()));
    // Differentials always off (see invariants above).
    changed |= set_path(&mut v, &["backup", "retention", "differential"], Value::Number(0.into()));

    // Ensure cloud block exists with at least a remote key; leave existing
    // values intact so a user-configured remote survives a restart.
    changed |= ensure_key_exists(&mut v, &["cloud", "remote"], Value::Null);

    if changed || !file.exists() {
        write_value(&v)?;
    }

    Ok(())
}

/// Write the save-revision retention count (`backup.retention.full`) into the
/// owned config.yaml. Called from `update_config` when the user changes the
/// "save revisions to keep" setting. Clamped to 1–10 so a stray value can't
/// disable backups (0) or balloon disk/cloud use. Differentials are left
/// untouched (always 0 — see [`ensure_config`]). Lowering the count prunes on
/// the next backup; raising it accumulates going forward.
pub fn set_retention(full: u32) -> AppResult<()> {
    let mut v = read_value_or_empty();
    apply_retention(&mut v, full);
    write_value(&v)
}

/// Pure value-mutation half of [`set_retention`] — no file IO, so it can be
/// unit tested. Clamps `full` to 1–10 and writes `backup.retention.full`.
fn apply_retention(v: &mut Value, full: u32) {
    let clamped = full.clamp(1, 10);
    set_path(
        v,
        &["backup", "retention", "full"],
        Value::Number(clamped.into()),
    );
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
    let mut v = read_value_or_empty();
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
            match prov {
                "custom" => {
                    let mut custom_map = serde_yaml::Mapping::new();
                    custom_map.insert(Value::String("id".into()), Value::String(rem.to_string()));
                    let mut remote_map = serde_yaml::Mapping::new();
                    remote_map.insert(Value::String("Custom".into()), Value::Mapping(custom_map));
                    set_path(v, &["cloud", "remote"], Value::Mapping(remote_map));
                }
                "box" => { set_path(v, &["cloud", "remote"], Value::String("Box".into())); }
                "dropbox" => { set_path(v, &["cloud", "remote"], Value::String("Dropbox".into())); }
                "google-drive" => { set_path(v, &["cloud", "remote"], Value::String("GoogleDrive".into())); }
                "onedrive" => { set_path(v, &["cloud", "remote"], Value::String("OneDrive".into())); }
                "ftp" => { set_path(v, &["cloud", "remote"], Value::String("Ftp".into())); }
                "smb" => { set_path(v, &["cloud", "remote"], Value::String("Smb".into())); }
                "webdav" => { set_path(v, &["cloud", "remote"], Value::String("WebDav".into())); }
                "spool-server" => {
                    // Remote was configured by `ludusavi cloud set webdav` as a
                    // named rclone remote — leave it untouched.
                }
                _ => {
                    set_path(v, &["cloud", "remote"], Value::Null);
                }
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
    let mut v = read_value_or_empty();
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

fn read_value() -> AppResult<Value> {
    let raw = std::fs::read_to_string(paths::ludusavi_config_file())?;
    serde_yaml::from_str(&raw)
        .map_err(|e| AppError::Other(format!("failed to parse ludusavi config.yaml: {e}")))
}

fn read_value_or_empty() -> Value {
    read_value().unwrap_or_else(|_| Value::Mapping(Default::default()))
}

fn write_value(v: &Value) -> AppResult<()> {
    let file = paths::ludusavi_config_file();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = file.with_extension("yaml.tmp");
    let yaml = serde_yaml::to_string(v)
        .map_err(|e| AppError::Other(format!("failed to serialize ludusavi config: {e}")))?;
    std::fs::write(&tmp, yaml)?;
    if file.exists() {
        let _ = std::fs::rename(&file, file.with_extension("yaml.bak"));
    }
    std::fs::rename(&tmp, &file)?;
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
        apply_retention(&mut v, 0); // below floor → 1
        assert_eq!(full(&v), Some(1));
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
    fn apply_cloud_preset_remote_is_a_bare_string() {
        let mut v = Value::Mapping(Default::default());
        apply_cloud(&mut v, Some("google-drive"), Some(""), None, None, None);
        assert_eq!(
            get_path(&v, &["cloud", "remote"]),
            Some(&Value::String("GoogleDrive".into())),
        );
    }

    #[test]
    fn apply_cloud_every_preset_maps_to_ludusavi_variant() {
        let cases = [
            ("box", "Box"),
            ("dropbox", "Dropbox"),
            ("google-drive", "GoogleDrive"),
            ("onedrive", "OneDrive"),
            ("ftp", "Ftp"),
            ("smb", "Smb"),
            ("webdav", "WebDav"),
        ];
        for (provider, expected) in cases {
            let mut v = Value::Mapping(Default::default());
            apply_cloud(&mut v, Some(provider), Some(""), None, None, None);
            assert_eq!(
                get_path(&v, &["cloud", "remote"]),
                Some(&Value::String(expected.into())),
                "provider {provider} should map to {expected}",
            );
        }
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
        assert_eq!(
            get_path(&v, &["cloud", "remote"]),
            Some(&Value::String("Dropbox".into())),
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
}
