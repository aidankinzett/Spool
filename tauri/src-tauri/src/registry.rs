//! Windows registry probe for the per-exe Run-As-Admin compatibility flag.
//!
//! Windows lets a user (or an installer) flag an exe as "always run as
//! administrator" via the AppCompatFlags layers registry key. Spool's
//! launch path needs to honour that even when the library entry's own
//! `run_as_admin` toggle is off — otherwise the game would silently
//! fail to elevate and the user wouldn't understand why launching
//! from Armoury Crate / Steam works but launching from Spool doesn't.
//!
//! Mirrors `RegistryHelper.cs` from the C# Spool app. Reads from
//! both HKCU (per-user) and HKLM (machine-wide). Returns false on
//! non-Windows platforms — the elevation concept doesn't apply there.

/// Returns `true` if the AppCompatFlags layers registry value for
/// `exe_path` contains the `RUNASADMIN` token (per-user OR
/// machine-wide). Best-effort: any error opening the keys returns
/// `false` rather than propagating — registry access can fail on
/// locked-down corporate machines and that shouldn't block launches.
#[cfg(windows)]
pub fn run_as_admin_in_registry(exe_path: &str) -> bool {
    use winreg::enums::*;
    use winreg::RegKey;

    const KEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";

    if exe_path.is_empty() {
        return false;
    }

    let probe = |hive: RegKey| -> bool {
        let Ok(key) = hive.open_subkey(KEY) else {
            return false;
        };
        let Ok(value): std::io::Result<String> = key.get_value(exe_path) else {
            return false;
        };
        value.to_ascii_uppercase().contains("RUNASADMIN")
    };

    probe(RegKey::predef(HKEY_CURRENT_USER)) || probe(RegKey::predef(HKEY_LOCAL_MACHINE))
}

/// Stub for non-Windows builds — elevation is Windows-only.
#[cfg(not(windows))]
pub fn run_as_admin_in_registry(_exe_path: &str) -> bool {
    false
}

/// Tauri command: lets the Edit dialog show a small "registry has
/// this set" hint next to the Run-as-administrator toggle, so the
/// user understands they don't need to flip the per-entry flag if
/// the OS already does it for them.
#[tauri::command]
pub fn get_run_as_admin_in_registry(exe_path: String) -> bool {
    run_as_admin_in_registry(&exe_path)
}
