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
//!   spool --run "Game Name" "ExePath" --attached
//!                                      → as above, but force attached-launch
//!                                        (fullscreen splash + exit on close),
//!                                        used by Apollo/Sunshine streaming-host
//!                                        shortcuts so the stream sees the same
//!                                        flow as SteamOS Game Mode
//!   spool --headless-server            → start the plugin loopback server, run forever
//!
//! Anything else is treated as `Normal`. We can extend with more
//! subcommands (--quit, --backup-all, etc.) as use cases arrive.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliMode {
    /// Open / focus the library window.
    Normal,
    /// Find a game by name and run its launch workflow. `attached` is set by a
    /// trailing `--attached` flag (Apollo/Sunshine shortcuts) to force the
    /// fullscreen-splash, exit-on-close behavior regardless of gamescope.
    Run {
        game_name: String,
        exe_path: String,
        attached: bool,
    },
    /// Start the plugin loopback server and run until killed. No tray, no
    /// window, no single-instance registration. Used by the Decky plugin
    /// (`spool --headless-server`) to get a persistent IPC endpoint it can
    /// query for session state, library data, and backup operations. This is the
    /// only way the plugin talks to Spool — game-stop backups, the unsynced
    /// "release lock" marker, and "Back up now" are all server endpoints.
    HeadlessServer,
}

/// Parses argv, skipping the program-name arg at position 0.
pub fn parse_args<S: AsRef<str>>(args: &[S]) -> CliMode {
    let rest: Vec<&str> = args.iter().skip(1).map(|s| s.as_ref()).collect();
    if rest.len() >= 3 && rest[0] == "--run" {
        return CliMode::Run {
            game_name: rest[1].to_string(),
            exe_path: rest[2].to_string(),
            attached: rest[3..].contains(&"--attached"),
        };
    }
    if rest.len() == 1 && rest[0] == "--headless-server" {
        return CliMode::HeadlessServer;
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
                attached: false,
            }
        );
    }

    #[test]
    fn run_with_attached_flag_sets_attached() {
        let argv = [
            "spool.exe",
            "--run",
            "Hades",
            "C:/Games/Hades/Hades.exe",
            "--attached",
        ];
        assert_eq!(
            parse_args(&argv),
            CliMode::Run {
                game_name: "Hades".to_string(),
                exe_path: "C:/Games/Hades/Hades.exe".to_string(),
                attached: true,
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
    fn headless_server_parses() {
        let argv = ["spool", "--headless-server"];
        assert_eq!(parse_args(&argv), CliMode::HeadlessServer);
    }

    #[test]
    fn headless_server_with_extra_args_falls_back_to_normal() {
        // Extra args → unrecognised, fall through to Normal.
        let argv = ["spool", "--headless-server", "--extra"];
        assert_eq!(parse_args(&argv), CliMode::Normal);
    }
}
