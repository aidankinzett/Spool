//! Firewall check for LAN sharing, surfaced as a hint in Settings → LAN
//! sharing.
//!
//! Peers find each other over UDP broadcast and then connect to the TCP
//! share port, so a host firewall that blocks either one makes sharing fail
//! with timeouts rather than a clear error. This reports whether the
//! firewall looks like it allows both, and the command that opens them.
//!
//! * Linux: `firewalld` is queried directly (no root needed). `ufw` can't be
//!   queried without root, so an active `ufw` gives a "may be blocking" hint.
//! * Windows: looks for the `Spool` rule the installer adds (see
//!   `windows/installer-hooks.nsh`).

use crate::config::SharedConfig;
use serde::Serialize;
use tauri::State;

/// UDP port the discovery announce uses (`lan/discovery.rs`).
const DISCOVERY_PORT: u16 = 47631;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FirewallState {
    /// No firewall found, or its rules allow the ports.
    Ok,
    /// A firewall is active and the ports are not open.
    Blocked,
    /// A firewall is active but its rules can't be read without elevation.
    Unknown,
}

/// Mirrored to `types.ts` as `FirewallStatus`.
#[derive(Debug, Clone, Serialize)]
pub struct FirewallStatus {
    pub state: FirewallState,
    /// Which firewall was found (`firewalld`, `ufw`, `Windows Firewall`), or `""`.
    pub firewall: String,
    /// Command that opens the ports, or `""` when nothing needs doing.
    pub command: String,
}

impl FirewallStatus {
    fn ok() -> Self {
        Self {
            state: FirewallState::Ok,
            firewall: String::new(),
            command: String::new(),
        }
    }
}

#[tauri::command]
pub async fn check_lan_firewall(config: State<'_, SharedConfig>) -> Result<FirewallStatus, String> {
    let port = {
        let cfg = config.lock().unwrap_or_else(|e| e.into_inner());
        cfg.data.lan.share_port
    };
    tokio::task::spawn_blocking(move || check(port))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
fn check(port: u16) -> FirewallStatus {
    use std::process::Command;

    // Run host tools with the AppImage's bundled library paths removed.
    let run = |prog: &str, args: &[&str]| -> Option<std::process::Output> {
        let mut cmd = Command::new(prog);
        cmd.args(args);
        crate::process::strip_appimage_env_blocking(&mut cmd);
        cmd.output().ok()
    };

    let tcp = format!("{port}/tcp");
    let udp = format!("{DISCOVERY_PORT}/udp");

    if let Some(out) = run("firewall-cmd", &["--state"]) {
        if out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "running" {
            let open = |spec: &str| {
                run("firewall-cmd", &[&format!("--query-port={spec}")])
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            };
            if open(&tcp) && open(&udp) {
                return FirewallStatus::ok();
            }
            return FirewallStatus {
                state: FirewallState::Blocked,
                firewall: "firewalld".into(),
                command: format!(
                    "sudo firewall-cmd --permanent --add-port={tcp} --add-port={udp} && sudo firewall-cmd --reload"
                ),
            };
        }
    }

    if let Some(out) = run("systemctl", &["is-active", "ufw"]) {
        if out.status.success() {
            return FirewallStatus {
                state: FirewallState::Unknown,
                firewall: "ufw".into(),
                command: format!("sudo ufw allow {tcp} && sudo ufw allow {udp}"),
            };
        }
    }

    FirewallStatus::ok()
}

#[cfg(windows)]
fn check(_port: u16) -> FirewallStatus {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // Exit status is 0 only when a rule with this name exists, whatever the
    // Windows display language.
    let has_rule = std::process::Command::new("netsh")
        .args(["advfirewall", "firewall", "show", "rule", "name=Spool"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(true);
    if has_rule {
        return FirewallStatus::ok();
    }
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "spool.exe".into());
    FirewallStatus {
        state: FirewallState::Unknown,
        firewall: "Windows Firewall".into(),
        command: format!(
            "netsh advfirewall firewall add rule name=\"Spool\" dir=in action=allow program=\"{exe}\" enable=yes profile=private,domain"
        ),
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn check(_port: u16) -> FirewallStatus {
    FirewallStatus::ok()
}
