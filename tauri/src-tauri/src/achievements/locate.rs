//! Where each Steam (and Epic) emulator keeps its achievement-state file.
//!
//! The emulators write under the Windows user profile, so the same relative
//! paths are resolved against one of two sets of roots:
//!
//! * the host's own folders (Windows, and Linux-native games — Goldberg's Linux
//!   build writes to `~/.local/share`, which `dirs::data_dir()` returns), or
//! * a Proton prefix's `drive_c` (`users/steamuser/AppData/…`,
//!   `users/Public/…`, `ProgramData`).
//!
//! Each candidate is a directory plus the file names an emulator might use in
//! it (the list Achievement Watcher probes). Some emulators can also be set to
//! save beside the game ("local save"), so the install folder is walked for a
//! directory named after the app id that holds one of those files.
//!
//! Games built on Epic Online Services carry RUNE's EOS emulator instead
//! (`epic_emu.ini` beside the game exe). Its file is keyed by the ini's
//! `AppName` rather than a Steam app id.

use super::parse::Format;
use std::path::{Path, PathBuf};

/// The Windows-profile folders the emulators write under.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    /// `%APPDATA%` (Roaming).
    pub appdata: Option<PathBuf>,
    /// `%LOCALAPPDATA%`.
    pub local_appdata: Option<PathBuf>,
    /// The user's Documents folder.
    pub documents: Option<PathBuf>,
    /// `C:\Users\Public`.
    pub public: Option<PathBuf>,
    /// `%PROGRAMDATA%`.
    pub program_data: Option<PathBuf>,
}

impl Roots {
    /// The host's own folders.
    pub fn native() -> Self {
        let env_dir = |k: &str| std::env::var_os(k).map(PathBuf::from);
        Self {
            appdata: dirs::data_dir(),
            local_appdata: dirs::data_local_dir(),
            documents: dirs::document_dir(),
            public: env_dir("PUBLIC"),
            program_data: env_dir("PROGRAMDATA"),
        }
    }

    /// The folders inside a Proton/Wine prefix (`prefix_root` contains `drive_c`).
    pub fn prefix(prefix_root: &Path) -> Self {
        let profile = prefix_root.join(crate::proton::WINE_STEAMUSER_PROFILE);
        let drive_c = prefix_root.join("drive_c");
        Self {
            appdata: Some(profile.join("AppData/Roaming")),
            local_appdata: Some(profile.join("AppData/Local")),
            documents: Some(profile.join("Documents")),
            public: Some(drive_c.join("users/Public")),
            program_data: Some(drive_c.join("ProgramData")),
        }
    }
}

/// One file to watch.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Source {
    pub path: PathBuf,
    pub format: Format,
}

/// File names an emulator may use inside its per-app directory, in the order
/// Achievement Watcher tries them.
const FILE_NAMES: &[&str] = &[
    "achievements.ini",
    "achievements.json",
    "achiev.ini",
    "stats.ini",
    "Achievements.Bin",
    "achieve.dat",
    "Achievements.ini",
    "stats/achievements.ini",
    "stats.bin",
    "stats/CreamAPI.Achievements.cfg",
];

/// Per-app directories for `appid` under `roots`.
fn emulator_dirs(roots: &Roots, appid: u64) -> Vec<PathBuf> {
    let id = appid.to_string();
    let mut out = Vec::new();
    let mut push = |base: &Option<PathBuf>, rel: &str| {
        if let Some(b) = base {
            out.push(b.join(rel.replace("{id}", &id)));
        }
    };
    push(&roots.appdata, "GSE Saves/{id}");
    push(&roots.appdata, "Goldberg SteamEmu Saves/{id}");
    push(&roots.appdata, "EMPRESS/{id}/remote/{id}");
    push(&roots.appdata, "Steam/CODEX/{id}");
    push(&roots.appdata, "SmartSteamEmu/{id}");
    push(&roots.appdata, "CreamAPI/{id}");
    push(&roots.local_appdata, "SKIDROW/{id}");
    push(&roots.local_appdata, "SKIDROW/{id}/SteamEmu/UserStats");
    push(&roots.documents, "SKIDROW/{id}");
    push(&roots.documents, "SkidRow/{id}");
    push(&roots.public, "Documents/Steam/CODEX/{id}");
    push(&roots.public, "Documents/Steam/RUNE/{id}");
    push(&roots.public, "Documents/EMPRESS/{id}/remote/{id}");
    push(&roots.public, "Documents/OnlineFix/{id}/Stats");
    // Reloaded / 3DM: ProgramData/Steam/<profile>/<id>.
    if let Some(pd) = &roots.program_data {
        if let Ok(rd) = std::fs::read_dir(pd.join("Steam")) {
            for e in rd.flatten() {
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    out.push(e.path().join(&id));
                }
            }
        }
    }
    out
}

/// Every candidate file for `appid`, whether or not it exists yet. A watcher
/// stats these each tick, so a file an emulator creates mid-session is seen.
pub fn candidate_sources(roots: &Roots, appid: u64) -> Vec<Source> {
    emulator_dirs(roots, appid)
        .into_iter()
        .flat_map(|dir| {
            FILE_NAMES.iter().map(move |name| Source {
                path: dir.join(name),
                format: Format::for_file(name.rsplit('/').next().unwrap_or(name)),
            })
        })
        .collect()
}

/// How deep the install-folder walk goes. Local saves sit beside the emulator
/// DLL, which is rarely more than a few levels down.
const GAME_FOLDER_DEPTH: usize = 5;

/// What one walk of an install folder finds.
#[derive(Debug, Clone, Default)]
pub struct FolderScan {
    /// From the shallowest `steam_appid.txt` (next to the emulator DLL or in
    /// its `steam_settings/`). The emulator keys its save folder by this id,
    /// so it's preferred over the library entry's `steam_id`, which can be a
    /// different edition's id.
    pub appid: Option<u64>,
    /// The Goldberg/GSE schema file, `steam_settings/achievements.json`.
    pub schema_file: Option<PathBuf>,
    /// Directories whose name is all digits — candidate local-save folders,
    /// one of which is named after the app id.
    numeric_dirs: Vec<PathBuf>,
    /// From the shallowest `epic_emu.ini` (RUNE's Epic Online Services
    /// emulator), which names the folder its achievement file is kept in.
    pub epic: Option<EpicEmu>,
}

/// An Epic Online Services emulator found in the install folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicEmu {
    /// The ini's `AppName`, e.g. `AlanWakeRemastered`.
    pub app_name: String,
    /// The folder holding the ini, which is also where the game exe sits.
    pub dir: PathBuf,
}

/// The `AppName` from an `epic_emu.ini`. `None` when it's missing or isn't a
/// plain folder name (it's joined onto a path).
fn epic_app_name(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && !l.starts_with(';'))
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("AppName"))
        .map(|(_, v)| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty() && v != "." && v != ".." && !v.contains(['/', '\\', ':']))
}

/// RUNE's Epic emulator writes `Users/Public/Documents/Epic/RUNE/<AppName>/`.
pub fn epic_candidate_sources(roots: &Roots, app_name: &str) -> Vec<Source> {
    let Some(public) = &roots.public else {
        return Vec::new();
    };
    vec![Source {
        path: public
            .join("Documents/Epic/RUNE")
            .join(app_name)
            .join("achievements.ini"),
        format: Format::Ini,
    }]
}

impl FolderScan {
    pub fn walk(game_folder: &Path) -> Self {
        let mut out = Self::default();
        let mut appid_depth = usize::MAX;
        let mut epic_depth = usize::MAX;
        for e in walkdir::WalkDir::new(game_folder)
            .max_depth(GAME_FOLDER_DEPTH)
            .into_iter()
            .filter_map(Result::ok)
        {
            let name = e.file_name().to_string_lossy();
            if e.file_type().is_dir() {
                if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
                    out.numeric_dirs.push(e.path().to_path_buf());
                }
            } else if e.depth() < appid_depth && name.eq_ignore_ascii_case("steam_appid.txt") {
                // Tools and redists bundled with a game sometimes ship their
                // own, deeper down; the game's is nearest the root.
                if let Some(id) = std::fs::read_to_string(e.path())
                    .ok()
                    .and_then(|t| t.trim().parse::<u64>().ok())
                    .filter(|id| *id > 0)
                {
                    out.appid = Some(id);
                    appid_depth = e.depth();
                }
            } else if e.depth() < epic_depth && name.eq_ignore_ascii_case("epic_emu.ini") {
                if let Some(app_name) = std::fs::read(e.path())
                    .ok()
                    .and_then(|b| epic_app_name(&String::from_utf8_lossy(&b)))
                {
                    out.epic = Some(EpicEmu {
                        app_name,
                        dir: e.path().parent().unwrap_or(game_folder).to_path_buf(),
                    });
                    epic_depth = e.depth();
                }
            } else if out.schema_file.is_none()
                && name == "achievements.json"
                && e.path()
                    .parent()
                    .and_then(|p| p.file_name())
                    .is_some_and(|n| n.eq_ignore_ascii_case("steam_settings"))
            {
                out.schema_file = Some(e.path().to_path_buf());
            }
        }
        out
    }

    /// The same walk, reused for [`SCAN_TTL`]. Opening a game's detail pane
    /// would otherwise re-walk a large install folder each time.
    pub fn cached(game_folder: &Path) -> Self {
        static CACHE: std::sync::LazyLock<
            std::sync::Mutex<std::collections::HashMap<PathBuf, (std::time::Instant, FolderScan)>>,
        > = std::sync::LazyLock::new(Default::default);
        if let Ok(cache) = CACHE.lock() {
            if let Some((at, scan)) = cache.get(game_folder) {
                if at.elapsed() < SCAN_TTL {
                    return scan.clone();
                }
            }
        }
        let scan = Self::walk(game_folder);
        if let Ok(mut cache) = CACHE.lock() {
            cache.insert(
                game_folder.to_path_buf(),
                (std::time::Instant::now(), scan.clone()),
            );
        }
        scan
    }

    /// Local-save files for `appid`: a directory named after it that holds one
    /// of the known file names. Only existing files are returned, so the
    /// session watcher re-walks periodically to pick up a file created
    /// mid-session.
    pub fn sources(&self, appid: u64) -> Vec<Source> {
        let id = appid.to_string();
        let mut out = Vec::new();
        for dir in self
            .numeric_dirs
            .iter()
            .filter(|d| d.file_name().is_some_and(|n| n.to_string_lossy() == id))
        {
            for name in FILE_NAMES {
                let path = dir.join(name);
                if path.is_file() {
                    out.push(Source {
                        path,
                        format: Format::for_file(name.rsplit('/').next().unwrap_or(name)),
                    });
                }
            }
        }
        out
    }
}

const SCAN_TTL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_roots_point_into_drive_c() {
        let r = Roots::prefix(Path::new("/p"));
        let srcs = candidate_sources(&r, 480);
        let has = |p: &str| srcs.iter().any(|s| s.path == Path::new(p));
        assert!(has(
            "/p/drive_c/users/steamuser/AppData/Roaming/GSE Saves/480/achievements.json"
        ));
        assert!(has(
            "/p/drive_c/users/Public/Documents/Steam/CODEX/480/achievements.ini"
        ));
        assert!(has(
            "/p/drive_c/users/steamuser/AppData/Roaming/SmartSteamEmu/480/stats.bin"
        ));
        let sse = srcs
            .iter()
            .find(|s| s.path.ends_with("SmartSteamEmu/480/stats.bin"))
            .unwrap();
        assert_eq!(sse.format, Format::SseBin);
        let cream = srcs
            .iter()
            .find(|s| {
                s.path
                    .ends_with("CreamAPI/480/stats/CreamAPI.Achievements.cfg")
            })
            .unwrap();
        assert_eq!(cream.format, Format::Ini);
    }

    #[test]
    fn game_folder_scan_finds_local_saves_and_appid() {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path().join("bin/steam_settings/saves/480");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("achievements.json"), "{}").unwrap();
        std::fs::write(dir.path().join("bin/steam_appid.txt"), "480\n").unwrap();
        // A bundled tool's own appid file, deeper down, is ignored.
        std::fs::create_dir_all(dir.path().join("bin/tools/editor")).unwrap();
        std::fs::write(dir.path().join("bin/tools/editor/steam_appid.txt"), "999").unwrap();
        std::fs::write(
            dir.path().join("bin/steam_settings/achievements.json"),
            "[]",
        )
        .unwrap();

        let scan = FolderScan::walk(dir.path());
        let found = scan.sources(480);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].format, Format::Json);
        assert!(scan.sources(10).is_empty());
        assert_eq!(scan.appid, Some(480));
        assert_eq!(
            scan.schema_file,
            Some(dir.path().join("bin/steam_settings/achievements.json"))
        );
    }

    #[test]
    fn roots_without_folders_yield_no_candidates() {
        assert!(candidate_sources(&Roots::default(), 480).is_empty());
        assert!(epic_candidate_sources(&Roots::default(), "Game").is_empty());
    }

    #[test]
    fn folder_scan_reads_the_epic_emulator_app_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("epic_emu.ini"),
            "### Game data is stored at ...\\<AppName>\\\n[Settings]\n###\nAppName=AlanWakeRemastered\nLanguage=en\n#AccountId=0\n",
        )
        .unwrap();
        let scan = FolderScan::walk(dir.path());
        assert_eq!(
            scan.epic,
            Some(EpicEmu {
                app_name: "AlanWakeRemastered".into(),
                dir: dir.path().to_path_buf(),
            })
        );
        let srcs = epic_candidate_sources(&Roots::prefix(Path::new("/p")), "AlanWakeRemastered");
        assert_eq!(
            srcs[0].path,
            Path::new(
                "/p/drive_c/users/Public/Documents/Epic/RUNE/AlanWakeRemastered/achievements.ini"
            )
        );
    }

    #[test]
    fn epic_app_name_must_be_a_plain_folder_name() {
        assert_eq!(epic_app_name("AppName = \"Game\"\n"), Some("Game".into()));
        assert_eq!(epic_app_name("#AppName=Commented\n"), None);
        assert_eq!(epic_app_name("AppName=../escape\n"), None);
        assert_eq!(epic_app_name("AppName=C:\\x\n"), None);
        assert_eq!(epic_app_name("AppName=..\n"), None);
        assert_eq!(epic_app_name("AppName=\n"), None);
    }

    #[test]
    fn program_data_scan_adds_one_dir_per_profile_folder_only() {
        let pd = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(pd.path().join("Steam/Profile1")).unwrap();
        std::fs::create_dir_all(pd.path().join("Steam/Profile2")).unwrap();
        std::fs::write(pd.path().join("Steam/stray.txt"), "").unwrap();
        let roots = Roots {
            program_data: Some(pd.path().to_path_buf()),
            ..Default::default()
        };
        let mut dirs = emulator_dirs(&roots, 480);
        dirs.sort();
        assert_eq!(
            dirs,
            vec![
                pd.path().join("Steam/Profile1/480"),
                pd.path().join("Steam/Profile2/480")
            ]
        );
        // No ProgramData/Steam folder at all is fine.
        let empty = tempfile::tempdir().unwrap();
        let roots = Roots {
            program_data: Some(empty.path().to_path_buf()),
            ..Default::default()
        };
        assert!(emulator_dirs(&roots, 480).is_empty());
    }

    #[test]
    fn folder_scan_ignores_unusable_appid_files_and_uses_the_next_valid_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("steam_appid.txt"), "not a number").unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin/STEAM_APPID.TXT"), " 730 \n").unwrap();
        assert_eq!(FolderScan::walk(dir.path()).appid, Some(730));

        let zero = tempfile::tempdir().unwrap();
        std::fs::write(zero.path().join("steam_appid.txt"), "0").unwrap();
        assert_eq!(FolderScan::walk(zero.path()).appid, None);
    }

    #[test]
    fn folder_scan_stops_at_the_depth_limit() {
        let dir = tempfile::tempdir().unwrap();
        let near = dir.path().join("a/b/c/d/480");
        let far = dir.path().join("a/b/c/d/e/f/999");
        std::fs::create_dir_all(&near).unwrap();
        std::fs::create_dir_all(&far).unwrap();
        std::fs::write(near.join("achievements.json"), "{}").unwrap();
        std::fs::write(far.join("achievements.json"), "{}").unwrap();
        let scan = FolderScan::walk(dir.path());
        assert_eq!(scan.sources(480).len(), 1);
        assert!(scan.sources(999).is_empty());
    }

    #[test]
    fn local_save_dir_must_be_named_exactly_the_appid() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["480", "4800", "48"] {
            let d = dir.path().join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("achievements.ini"), "").unwrap();
        }
        // A directory with the right name but no known file contributes nothing.
        std::fs::create_dir_all(dir.path().join("sub/10")).unwrap();
        let scan = FolderScan::walk(dir.path());
        let found = scan.sources(480);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, dir.path().join("480/achievements.ini"));
        assert_eq!(found[0].format, Format::Ini);
        assert!(scan.sources(10).is_empty());
    }

    #[test]
    fn schema_file_requires_a_steam_settings_parent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("other")).unwrap();
        std::fs::write(dir.path().join("other/achievements.json"), "[]").unwrap();
        std::fs::create_dir_all(dir.path().join("Steam_Settings")).unwrap();
        std::fs::write(dir.path().join("Steam_Settings/Achievements.json"), "[]").unwrap();
        assert_eq!(FolderScan::walk(dir.path()).schema_file, None);
        std::fs::write(dir.path().join("Steam_Settings/achievements.json"), "[]").unwrap();
        assert!(FolderScan::walk(dir.path()).schema_file.is_some());
    }
}
