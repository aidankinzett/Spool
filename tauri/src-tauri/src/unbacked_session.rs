//! Per-game marker for a play session whose saves haven't reached the local
//! backup store yet.
//!
//! The run workflow writes it when a session starts and removes it once the
//! post-session backup has captured the saves. A marker left behind means the
//! session's Spool process stopped before backing up (killed by a streaming
//! host, Steam, a crash) while the game may have kept running, so the live
//! save can hold progress no backup has. Every path that restores over the
//! live save checks for it first and backs the live save up
//! (`runner::rescue_unbacked_session`).
//!
//! When that rescue writes a revision, the marker records which tip it
//! replaced and which it wrote. It stays until the restore that follows
//! succeeds, so an aborted launch (e.g. a cloud conflict) still knows on retry
//! that the local tip is a rescue, not ordinary play.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Marker {
    pub started_at: Option<DateTime<Utc>>,
    /// Set once a rescue backup has written a revision for this session.
    pub rescue: Option<Rescue>,
}

/// A rescue backup's effect on the local backup store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rescue {
    /// The local tip before the rescue — what the interrupted session started
    /// from. `None` when the game had no backup yet.
    pub from: Option<String>,
    /// The revision the rescue wrote.
    pub to: String,
}

/// Record that a session for `game_id` has started. Best-effort: a failed
/// write only loses the safety net, so it's logged rather than returned.
pub async fn mark_started(game_id: &str) {
    write(
        game_id,
        Marker {
            started_at: Some(Utc::now()),
            rescue: None,
        },
    )
    .await;
}

/// The marker for `game_id`, if a session's saves haven't been backed up. A
/// marker file that doesn't parse still counts — its existence is the signal.
pub async fn read(game_id: &str) -> Option<Marker> {
    let path = crate::paths::unbacked_session_file(game_id);
    let bytes = tokio::fs::read(path).await.ok()?;
    Some(parse(&bytes))
}

/// Store a rescue on the existing marker.
pub async fn record_rescue(game_id: &str, marker: &Marker, rescue: Rescue) {
    write(
        game_id,
        Marker {
            rescue: Some(rescue),
            ..marker.clone()
        },
    )
    .await;
}

/// Remove the marker once the session's saves are safe.
pub async fn clear(game_id: &str) {
    let path = crate::paths::unbacked_session_file(game_id);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(game_id, error = %e, "failed to remove unbacked-session marker"),
    }
}

/// [`clear`], but only when the marker was written before `t`. The Decky
/// game-stop backup uses this so a backup that finishes after the same game
/// was relaunched can't remove the new session's marker. A marker with no
/// start time predates this check and is treated as older.
#[cfg_attr(not(unix), allow(dead_code))]
pub async fn clear_if_started_before(game_id: &str, t: DateTime<Utc>) {
    let Some(marker) = read(game_id).await else {
        return;
    };
    if started_before(&marker, t) {
        clear(game_id).await;
    }
}

#[cfg_attr(not(unix), allow(dead_code))]
fn started_before(marker: &Marker, t: DateTime<Utc>) -> bool {
    marker.started_at.is_none_or(|s| s < t)
}

async fn write(game_id: &str, marker: Marker) {
    let path = crate::paths::unbacked_session_file(game_id);
    let res = tokio::task::spawn_blocking(move || {
        let bytes = serde_json::to_vec(&marker).map_err(std::io::Error::other)?;
        crate::paths::write_atomic(&path, &bytes, false)
    })
    .await
    .map_err(std::io::Error::other)
    .and_then(|r| r);
    if let Err(e) = res {
        tracing::warn!(game_id, error = %e, "failed to write unbacked-session marker");
    }
}

fn parse(bytes: &[u8]) -> Marker {
    serde_json::from_slice(bytes).unwrap_or_default()
}

/// The rescue to record after a rescue backup moved the local tip from
/// `tip_before` to `tip_after`, given the rescue already on the marker (from an
/// earlier attempt whose restore didn't complete). Keeps the earliest `from`:
/// that's the tip the interrupted session started from, the merge-base for the
/// cloud comparison. Returns `existing` unchanged when the tip didn't move.
pub fn merge_rescue(
    existing: Option<&Rescue>,
    tip_before: Option<&str>,
    tip_after: Option<&str>,
) -> Option<Rescue> {
    match tip_after {
        Some(after) if Some(after) != tip_before => Some(Rescue {
            from: match existing {
                Some(r) => r.from.clone(),
                None => tip_before.map(str::to_string),
            },
            to: after.to_string(),
        }),
        _ => existing.cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unparsable_marker_still_counts() {
        assert_eq!(parse(b"2026-10-21T02:24:32Z"), Marker::default());
        assert_eq!(parse(b""), Marker::default());
    }

    #[test]
    fn marker_round_trips() {
        let m = Marker {
            started_at: None,
            rescue: Some(Rescue {
                from: Some("A".into()),
                to: "R".into(),
            }),
        };
        assert_eq!(parse(&serde_json::to_vec(&m).unwrap()), m);
    }

    #[test]
    fn only_markers_written_before_game_stop_are_cleared() {
        let stop = Utc::now();
        let earlier = Marker {
            started_at: Some(stop - chrono::Duration::hours(2)),
            rescue: None,
        };
        let relaunched = Marker {
            started_at: Some(stop + chrono::Duration::seconds(5)),
            rescue: None,
        };
        assert!(started_before(&earlier, stop));
        assert!(!started_before(&relaunched, stop));
        assert!(started_before(&Marker::default(), stop));
    }

    #[test]
    fn first_rescue_records_previous_tip() {
        assert_eq!(
            merge_rescue(None, Some("A"), Some("R")),
            Some(Rescue {
                from: Some("A".into()),
                to: "R".into()
            })
        );
        // No backup before the session.
        assert_eq!(
            merge_rescue(None, None, Some("R")),
            Some(Rescue {
                from: None,
                to: "R".into()
            })
        );
    }

    #[test]
    fn unchanged_tip_keeps_existing_rescue() {
        assert_eq!(merge_rescue(None, Some("A"), Some("A")), None);
        let r = Rescue {
            from: Some("A".into()),
            to: "R".into(),
        };
        assert_eq!(merge_rescue(Some(&r), Some("R"), Some("R")), Some(r));
    }

    #[test]
    fn later_rescue_keeps_original_base() {
        let r = Rescue {
            from: Some("A".into()),
            to: "R".into(),
        };
        assert_eq!(
            merge_rescue(Some(&r), Some("R"), Some("R2")),
            Some(Rescue {
                from: Some("A".into()),
                to: "R2".into()
            })
        );
    }
}
