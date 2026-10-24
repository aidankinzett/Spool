//! Firewall check for LAN sharing, surfaced as a hint in Settings → LAN
//! sharing.
//!
//! Peers find each other over UDP broadcast and then connect to the TCP
//! share port, so a host firewall that blocks either one makes sharing fail
//! with timeouts rather than a clear error. This reports whether the
//! firewall looks like it allows both, and the command that opens them.
//!
//! * Linux: `firewalld` is queried directly (no root needed), in each zone
//!   an interface is currently using. `ufw` can't be
//!   queried without root, so an active `ufw` gives a "may be blocking" hint.
//! * Windows: looks for the `Spool` rule the installer adds (see
//!   `windows/installer-hooks.nsh`).

use serde::Serialize;

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
pub async fn check_lan_firewall(port: u16) -> Result<FirewallStatus, String> {
    tokio::task::spawn_blocking(move || check(port))
        .await
        .map_err(|e| e.to_string())
}

/// Longest a firewall query may run. `firewall-cmd` and `systemctl` talk to
/// D-Bus and can hang while the service restarts.
#[cfg(target_os = "linux")]
const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Runs a host tool with the AppImage's bundled library paths removed, and
/// kills it if it outlives [`QUERY_TIMEOUT`].
#[cfg(target_os = "linux")]
fn run_tool(prog: &str, args: &[&str]) -> Option<(bool, String)> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::process::strip_appimage_env_blocking(&mut cmd);
    let mut child = cmd.spawn().ok()?;
    let deadline = std::time::Instant::now() + QUERY_TIMEOUT;
    let status = loop {
        match child.try_wait().ok()? {
            Some(status) => break status,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    };
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    Some((status.success(), out))
}

/// Zone names from `firewall-cmd --get-active-zones`: zone lines are
/// unindented, the interface and source lines under each are indented.
#[cfg(target_os = "linux")]
fn parse_active_zones(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with(char::is_whitespace))
        .map(|l| l.trim().to_string())
        .collect()
}

#[cfg(target_os = "linux")]
fn check(port: u16) -> FirewallStatus {
    let tcp = format!("{port}/tcp");
    let udp = format!("{DISCOVERY_PORT}/udp");

    if let Some((true, state)) = run_tool("firewall-cmd", &["--state"]) {
        if state.trim() == "running" {
            let zones = run_tool("firewall-cmd", &["--get-active-zones"])
                .map(|(_, out)| parse_active_zones(&out))
                .unwrap_or_default();
            let open_in = |zone: Option<&str>| {
                [&tcp, &udp].iter().all(|spec| {
                    let mut args = Vec::new();
                    let zone_arg = zone.map(|z| format!("--zone={z}"));
                    if let Some(z) = &zone_arg {
                        args.push(z.as_str());
                    }
                    let query = format!("--query-port={spec}");
                    args.push(&query);
                    matches!(run_tool("firewall-cmd", &args), Some((true, _)))
                })
            };
            // Open in any active zone is enough: a docker or VPN zone being
            // closed doesn't affect the LAN interface.
            let open = if zones.is_empty() {
                open_in(None)
            } else {
                zones.iter().any(|z| open_in(Some(z)))
            };
            if open {
                return FirewallStatus::ok();
            }
            let zone_flag = zones
                .first()
                .map(|z| format!(" --zone={z}"))
                .unwrap_or_default();
            return FirewallStatus {
                state: FirewallState::Blocked,
                firewall: "firewalld".into(),
                command: format!(
                    "sudo firewall-cmd --permanent{zone_flag} --add-port={tcp} --add-port={udp} && sudo firewall-cmd --reload"
                ),
            };
        }
    }

    if let Some((true, _)) = run_tool("systemctl", &["is-active", "ufw"]) {
        return FirewallStatus {
            state: FirewallState::Unknown,
            firewall: "ufw".into(),
            command: format!("sudo ufw allow {tcp} && sudo ufw allow {udp}"),
        };
    }

    FirewallStatus::ok()
}

#[cfg(windows)]
fn check(_port: u16) -> FirewallStatus {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "spool.exe".into());

    // The rule must exist and point at this exe: a rule left over from an
    // install in another folder doesn't cover this binary. The verbose output
    // is localised, but the program path in it is not, so match on that.
    let output = std::process::Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            "name=Spool",
            "verbose",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(o) = &output {
        if o.status.success()
            && String::from_utf8_lossy(&o.stdout)
                .to_lowercase()
                .contains(&exe.to_lowercase())
        {
            return FirewallStatus::ok();
        }
    }
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::parse_active_zones;

    #[test]
    fn active_zones_are_the_unindented_lines() {
        let out = "FedoraWorkstation\n  interfaces: enp3s0\ndocker\n  interfaces: docker0\n";
        assert_eq!(parse_active_zones(out), ["FedoraWorkstation", "docker"]);
        assert!(parse_active_zones("").is_empty());
    }
}
