//! Detects whether Spool is running inside a SteamOS / Steam Deck "Game Mode"
//! session (the gamescope-composited Big Picture session) vs a normal desktop
//! session. The `--run` startup path uses this to switch into attached-launch
//! mode: in Game Mode, Spool runs the game workflow then EXITS so Steam sees
//! the game stop, instead of staying tray-resident.

/// Pure decision core, separated from env reads so it is unit-testable.
/// `override_val` is `$SPOOL_ATTACHED_LAUNCH`, `gamescope` is
/// `$GAMESCOPE_WAYLAND_DISPLAY`, `is_linux` gates the gamescope signal.
fn decide(override_val: Option<&str>, gamescope: Option<&str>, is_linux: bool) -> bool {
    if let Some(v) = override_val {
        let v = v.trim();
        if v == "1" || v.eq_ignore_ascii_case("true") {
            return true;
        }
        if v == "0" || v.eq_ignore_ascii_case("false") {
            return false;
        }
    }
    is_linux && gamescope.map(|s| !s.is_empty()).unwrap_or(false)
}

/// True when Spool should use attached-launch behavior. See `decide`.
#[allow(dead_code)]
pub fn is_steam_game_mode() -> bool {
    let override_val = std::env::var("SPOOL_ATTACHED_LAUNCH").ok();
    let gamescope = std::env::var("GAMESCOPE_WAYLAND_DISPLAY").ok();
    decide(
        override_val.as_deref(),
        gamescope.as_deref(),
        cfg!(target_os = "linux"),
    )
}

#[cfg(test)]
mod tests {
    use super::decide;

    #[test]
    fn gamescope_present_on_linux_is_game_mode() {
        assert!(decide(None, Some("gamescope-0"), true));
    }

    #[test]
    fn gamescope_present_off_linux_is_not_game_mode() {
        assert!(!decide(None, Some("gamescope-0"), false));
    }

    #[test]
    fn no_gamescope_is_not_game_mode() {
        assert!(!decide(None, None, true));
        assert!(!decide(None, Some(""), true));
    }

    #[test]
    fn override_forces_on_and_off() {
        assert!(decide(Some("1"), None, false));
        assert!(decide(Some("true"), None, false));
        assert!(!decide(Some("0"), Some("gamescope-0"), true));
        assert!(!decide(Some("false"), Some("gamescope-0"), true));
    }
}
