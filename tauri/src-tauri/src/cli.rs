//! Command-line argument parsing.
//!
//! Used in two places:
//!   1. App startup — the initial argv determines whether to open the
//!      library window normally or queue a `--run` workflow.
//!   2. The single-instance forwarding callback — when a secondary
//!      `spool` invocation lands on the running primary, its argv tells
//!      us whether to focus the library or kick off a game launch.
//!
//! Format:
//!   spool                              → normal library launch
//!   spool --run "Game Name" "ExePath"  → launch this game's workflow
//!   spool --backup "Game Name"         → headless one-shot backup, then exit
//!   spool --release-lock "Game Name"   → headless one-shot lock release, then exit
//!
//! Anything else is treated as `Normal`. We can extend with more
//! subcommands (--quit, --backup-all, etc.) as use cases arrive.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliMode {
    /// Open / focus the library window.
    Normal,
    /// Find a game by name and run its launch workflow.
    Run { game_name: String, exe_path: String },
    /// Headless one-shot: back up a single game's saves, then exit. Used by
    /// the Decky plugin's forced-close fallback. No GUI, no tray.
    ///
    /// Deliberately does NOT release the sync-server play lock — that's a
    /// separate concern (`ReleaseLock`) so a plain backup never has the hidden
    /// side effect of dropping a lock. The Decky fallback invokes both.
    Backup { game_name: String },
    /// Headless one-shot: release a single game's sync-server play lock, then
    /// exit. Used by the Decky plugin's forced-close fallback alongside
    /// `--backup` — Steam SIGKILLs Spool before its run workflow can release the
    /// lock, so this drops it directly. No GUI, no tray.
    ReleaseLock { game_name: String },
}

/// Parses argv, skipping the program-name arg at position 0.
pub fn parse_args<S: AsRef<str>>(args: &[S]) -> CliMode {
    let rest: Vec<&str> = args.iter().skip(1).map(|s| s.as_ref()).collect();
    if rest.len() >= 3 && rest[0] == "--run" {
        return CliMode::Run {
            game_name: rest[1].to_string(),
            exe_path: rest[2].to_string(),
        };
    }
    if rest.len() >= 2 && rest[0] == "--backup" {
        return CliMode::Backup {
            game_name: rest[1].to_string(),
        };
    }
    if rest.len() >= 2 && rest[0] == "--release-lock" {
        return CliMode::ReleaseLock {
            game_name: rest[1].to_string(),
        };
    }
    CliMode::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_args_means_normal() {
        assert_eq!(parse_args::<&str>(&["spool.exe"]), CliMode::Normal);
        assert_eq!(parse_args::<&str>(&[]), CliMode::Normal);
    }

    #[test]
    fn run_with_two_args_parses() {
        let argv = ["spool.exe", "--run", "Hades", "C:/Games/Hades/Hades.exe"];
        assert_eq!(
            parse_args(&argv),
            CliMode::Run {
                game_name: "Hades".to_string(),
                exe_path: "C:/Games/Hades/Hades.exe".to_string(),
            }
        );
    }

    #[test]
    fn run_with_missing_args_falls_back_to_normal() {
        let argv = ["spool.exe", "--run", "Hades"];
        assert_eq!(parse_args(&argv), CliMode::Normal);
    }

    #[test]
    fn unknown_flags_fall_through() {
        let argv = ["spool.exe", "--help"];
        assert_eq!(parse_args(&argv), CliMode::Normal);
    }

    #[test]
    fn backup_with_name_parses() {
        let argv = ["spool.exe", "--backup", "Hades"];
        assert_eq!(
            parse_args(&argv),
            CliMode::Backup {
                game_name: "Hades".to_string(),
            }
        );
    }

    #[test]
    fn backup_missing_name_falls_back_to_normal() {
        assert_eq!(parse_args::<&str>(&["spool.exe", "--backup"]), CliMode::Normal);
    }

    #[test]
    fn release_lock_with_name_parses() {
        let argv = ["spool.exe", "--release-lock", "Hades"];
        assert_eq!(
            parse_args(&argv),
            CliMode::ReleaseLock {
                game_name: "Hades".to_string(),
            }
        );
    }

    #[test]
    fn release_lock_missing_name_falls_back_to_normal() {
        assert_eq!(
            parse_args::<&str>(&["spool.exe", "--release-lock"]),
            CliMode::Normal
        );
    }
}
