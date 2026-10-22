//! Parsers for the achievement-state files Steam emulators write. Pure, no IO.
//!
//! Each parser returns only the *earned* entries. Field names and layouts
//! follow xan105's Achievement Watcher, which reads the same files:
//!
//! * **JSON** (Goldberg, GSE / gbe_fork, EMPRESS): an object keyed by API name,
//!   `{ "ACH_X": { "earned": true, "earned_time": 1700000000 } }`. Some forks
//!   write an array of `{ "name": "ACH_X", "achieved": 1, … }` instead.
//! * **INI** (CODEX, RUNE, Reloaded / 3DM, Skidrow, CreamAPI, OnlineFix, …):
//!   usually one section per achievement with `Achieved=1` / `UnlockTime=…`,
//!   under assorted key spellings. Three other layouts exist: Hoodlum's
//!   `[Achievements]` + `[AchievementsUnlockTimes]` pair, 3DM's `[State]` +
//!   `[Time]` pair with hex values, and Reloaded's per-section hex `State`.
//! * **SmartSteamEmu `stats.bin`**: a little-endian `i32` count, then 24-byte
//!   records keyed by the CRC32 of the API name, so names are resolved against
//!   the schema later.

use serde_json::Value;
use std::collections::HashMap;

/// How an entry names its achievement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// The API name as written in the file (case may differ from the schema).
    Name(String),
    /// CRC32 of the API name (SmartSteamEmu).
    Crc(u32),
}

/// One earned achievement read from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Earned {
    pub key: Key,
    /// Unix seconds, when the file records a plausible one.
    pub unlocked_at: Option<i64>,
}

/// File layout, chosen from the file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Json,
    Ini,
    SseBin,
}

impl Format {
    pub fn for_file(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".json") {
            Format::Json
        } else if lower == "stats.bin" {
            Format::SseBin
        } else {
            Format::Ini
        }
    }
}

pub fn parse(format: Format, bytes: &[u8]) -> Vec<Earned> {
    match format {
        Format::Json => parse_json(&String::from_utf8_lossy(bytes)),
        Format::Ini => parse_ini(&decode_text(bytes)),
        Format::SseBin => parse_sse(bytes),
    }
}

/// Some emulators write their INI as UTF-16LE with a BOM.
fn decode_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let s = String::from_utf8_lossy(bytes);
    s.strip_prefix('\u{feff}').unwrap_or(&s).to_string()
}

/// Normalises a recorded unlock time to unix seconds. Values that look like
/// milliseconds are scaled down; anything before 2001 (zero, CreamAPI's
/// truncated 7-digit stamps) is treated as unknown.
fn plausible_time(t: i64) -> Option<i64> {
    let secs = if t > 100_000_000_000 { t / 1000 } else { t };
    (secs >= 1_000_000_000).then_some(secs)
}

const ACHIEVED_KEYS: &[&str] = &["achieved", "earned", "state", "haveachieved", "unlocked"];
const TIME_KEYS: &[&str] = &[
    "unlocktime",
    "earned_time",
    "haveachievedtime",
    "havehaveachievedtime",
    "time",
    "timestamp",
];

fn truthy(s: &str) -> bool {
    let s = s.trim();
    s == "1" || s.eq_ignore_ascii_case("true")
}

// ── JSON ─────────────────────────────────────────────────────────────────────

fn json_num(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn json_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64() == Some(1),
        Value::String(s) => truthy(s),
        _ => false,
    }
}

/// Reads one entry object; `None` when it isn't earned.
fn json_entry(obj: &serde_json::Map<String, Value>) -> Option<Option<i64>> {
    let get = |keys: &[&str]| {
        obj.iter()
            .find(|(k, _)| keys.contains(&k.to_ascii_lowercase().as_str()))
            .map(|(_, v)| v)
    };
    let mut earned = get(ACHIEVED_KEYS).is_some_and(json_truthy);
    if !earned {
        let cur = get(&["curprogress", "progress"])
            .and_then(json_num)
            .unwrap_or(0);
        let max = get(&["maxprogress", "max_progress"])
            .and_then(json_num)
            .unwrap_or(0);
        earned = max > 0 && cur >= max;
    }
    earned.then(|| get(TIME_KEYS).and_then(json_num).and_then(plausible_time))
}

fn parse_json(text: &str) -> Vec<Earned> {
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match root {
        Value::Object(map) => {
            for (name, v) in map {
                if let Some(obj) = v.as_object() {
                    if let Some(t) = json_entry(obj) {
                        out.push(Earned {
                            key: Key::Name(name),
                            unlocked_at: t,
                        });
                    }
                }
            }
        }
        Value::Array(items) => {
            for v in items {
                let Some(obj) = v.as_object() else { continue };
                let name = ["name", "apiname", "id"]
                    .iter()
                    .find_map(|k| obj.get(*k).and_then(Value::as_str));
                if let (Some(name), Some(t)) = (name, json_entry(obj)) {
                    out.push(Earned {
                        key: Key::Name(name.to_string()),
                        unlocked_at: t,
                    });
                }
            }
        }
        _ => {}
    }
    out
}

// ── INI ──────────────────────────────────────────────────────────────────────

/// Section name → (lower-cased key → value), in file order.
type Ini = Vec<(String, HashMap<String, String>)>;

fn read_ini(text: &str) -> Ini {
    let mut out: Ini = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push((name.trim().to_string(), HashMap::new()));
        } else if let Some((k, v)) = line.split_once('=') {
            if out.is_empty() {
                out.push((String::new(), HashMap::new()));
            }
            let v = v.trim().trim_matches('"').to_string();
            out.last_mut()
                .expect("pushed above")
                .1
                .insert(k.trim().to_ascii_lowercase(), v);
        }
    }
    out
}

/// Reloaded / 3DM store numbers as 8 hex digits holding a little-endian u32
/// (`01000000` = 1).
fn hex_le_u32(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some(n.swap_bytes() as i64)
}

/// Section names that hold metadata, not achievements.
const INI_SKIP: &[&str] = &["steamachievements", "steam64", "steam", "settings"];

fn parse_ini(text: &str) -> Vec<Earned> {
    let ini = read_ini(text);
    let section = |name: &str| {
        ini.iter()
            .find(|(s, _)| s.eq_ignore_ascii_case(name))
            .map(|(_, kv)| kv)
    };
    let mut out = Vec::new();

    // Hoodlum: [Achievements] NAME=1, [AchievementsUnlockTimes] NAME=<time>.
    // Keys were lower-cased on read, so the names are recovered from the raw
    // text to keep the file's casing.
    if let (Some(state), Some(times)) =
        (section("Achievements"), section("AchievementsUnlockTimes"))
    {
        for name in raw_keys(text, "Achievements") {
            if state
                .get(&name.to_ascii_lowercase())
                .is_some_and(|v| truthy(v))
            {
                let t = times
                    .get(&name.to_ascii_lowercase())
                    .and_then(|v| v.parse().ok())
                    .and_then(plausible_time);
                out.push(Earned {
                    key: Key::Name(name),
                    unlocked_at: t,
                });
            }
        }
        return out;
    }

    // 3DM: [State] NAME=0101, [Time] NAME=<hex LE u32>.
    if let (Some(state), Some(times)) = (section("State"), section("Time")) {
        for name in raw_keys(text, "State") {
            if state.get(&name.to_ascii_lowercase()).map(String::as_str) == Some("0101") {
                let t = times
                    .get(&name.to_ascii_lowercase())
                    .and_then(|v| hex_le_u32(v))
                    .and_then(plausible_time);
                out.push(Earned {
                    key: Key::Name(name),
                    unlocked_at: t,
                });
            }
        }
        return out;
    }

    // ACHIEVE_DATA: NAME=1 lines in one section.
    if section("ACHIEVE_DATA").is_some() {
        for name in raw_keys(text, "ACHIEVE_DATA") {
            if section("ACHIEVE_DATA")
                .and_then(|kv| kv.get(&name.to_ascii_lowercase()))
                .is_some_and(|v| truthy(v))
            {
                out.push(Earned {
                    key: Key::Name(name),
                    unlocked_at: None,
                });
            }
        }
        return out;
    }

    // One section per achievement.
    for (name, kv) in &ini {
        if name.is_empty() || INI_SKIP.iter().any(|s| name.eq_ignore_ascii_case(s)) {
            continue;
        }
        let first = |keys: &[&str]| keys.iter().find_map(|k| kv.get(*k));
        // Reloaded writes State/Time/progress as hex LE u32.
        let rld = kv.get("state").and_then(|v| hex_le_u32(v)).is_some();
        let num = |v: &String| {
            if rld {
                hex_le_u32(v)
            } else {
                v.trim().parse::<i64>().ok()
            }
        };
        let mut earned = if rld {
            kv.get("state").and_then(num) == Some(1)
        } else {
            first(ACHIEVED_KEYS).is_some_and(|v| truthy(v))
        };
        if !earned {
            let cur = kv.get("curprogress").and_then(num).unwrap_or(0);
            let max = kv.get("maxprogress").and_then(num).unwrap_or(0);
            earned = max > 0 && cur >= max;
        }
        if earned {
            let t = first(TIME_KEYS).and_then(num).and_then(plausible_time);
            out.push(Earned {
                key: Key::Name(name.clone()),
                unlocked_at: t,
            });
        }
    }
    out
}

/// The keys of one section with their original casing, in file order.
fn raw_keys(text: &str, section: &str) -> Vec<String> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name.trim().eq_ignore_ascii_case(section);
        } else if inside {
            if let Some((k, _)) = line.split_once('=') {
                out.push(k.trim().to_string());
            }
        }
    }
    out
}

// ── SmartSteamEmu stats.bin ──────────────────────────────────────────────────

fn parse_sse(bytes: &[u8]) -> Vec<Earned> {
    const RECORD: usize = 24;
    if bytes.len() < 4 {
        return Vec::new();
    }
    let count = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let body = &bytes[4..];
    if count < 0 || body.len() != count as usize * RECORD {
        return Vec::new();
    }
    let le = |b: &[u8]| i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    body.as_chunks::<RECORD>()
        .0
        .iter()
        .filter_map(|r| {
            // 0/1 is an achievement's state; larger values are plain stats.
            let value = le(&r[20..24]);
            (value == 1).then(|| Earned {
                key: Key::Crc(u32::from_le_bytes([r[0], r[1], r[2], r[3]])),
                unlocked_at: plausible_time(le(&r[8..12]) as i64),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[Earned]) -> Vec<(String, Option<i64>)> {
        let mut out: Vec<_> = v
            .iter()
            .map(|e| match &e.key {
                Key::Name(n) => (n.clone(), e.unlocked_at),
                Key::Crc(c) => (format!("{c:08x}"), e.unlocked_at),
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn goldberg_json_object() {
        let j = r#"{
            "ACH_A": { "earned": true, "earned_time": 1700000000 },
            "ACH_B": { "earned": false, "earned_time": 0 },
            "ACH_C": { "earned": true, "earned_time": 0 }
        }"#;
        assert_eq!(
            names(&parse(Format::Json, j.as_bytes())),
            vec![
                ("ACH_A".into(), Some(1_700_000_000)),
                ("ACH_C".into(), None)
            ]
        );
    }

    #[test]
    fn json_array_and_millis() {
        let j = r#"[{ "name": "X", "achieved": 1, "UnlockTime": 1700000000123 }, { "name": "Y", "achieved": 0 }]"#;
        assert_eq!(
            names(&parse(Format::Json, j.as_bytes())),
            vec![("X".into(), Some(1_700_000_000))]
        );
    }

    #[test]
    fn codex_ini() {
        let ini = "[SteamAchievements]\nCount=2\n00000=ACH_A\n\n[ACH_A]\nAchieved=1\nCurProgress=0\nMaxProgress=0\nUnlockTime=1700000000\n\n[ACH_B]\nAchieved=0\n";
        assert_eq!(
            names(&parse(Format::Ini, ini.as_bytes())),
            vec![("ACH_A".into(), Some(1_700_000_000))]
        );
    }

    #[test]
    fn progress_completion_counts_as_earned() {
        let ini = "[ACH_P]\nAchieved=0\nCurProgress=10\nMaxProgress=10\n";
        assert_eq!(
            names(&parse(Format::Ini, ini.as_bytes())),
            vec![("ACH_P".into(), None)]
        );
    }

    #[test]
    fn onlinefix_ini() {
        let ini = "[ACH_A]\nachieved=true\ntimestamp=1700000000\n";
        assert_eq!(
            names(&parse(Format::Ini, ini.as_bytes())),
            vec![("ACH_A".into(), Some(1_700_000_000))]
        );
    }

    #[test]
    fn reloaded_hex_ini() {
        // 0x6553f100 = 1700000000, little-endian hex "00f15365".
        let ini = "[ACH_A]\nState=01000000\nTime=00f15365\nCurProgress=00000000\nMaxProgress=00000000\n[ACH_B]\nState=00000000\n";
        assert_eq!(
            names(&parse(Format::Ini, ini.as_bytes())),
            vec![("ACH_A".into(), Some(1_700_000_000))]
        );
    }

    #[test]
    fn hoodlum_ini() {
        let ini =
            "[Achievements]\nAch_One=1\nAch_Two=0\n[AchievementsUnlockTimes]\nAch_One=1700000000\n";
        assert_eq!(
            names(&parse(Format::Ini, ini.as_bytes())),
            vec![("Ach_One".into(), Some(1_700_000_000))]
        );
    }

    #[test]
    fn three_dm_ini() {
        let ini = "[State]\nACH_A=0101\nACH_B=0000\n[Time]\nACH_A=00f15365\n";
        assert_eq!(
            names(&parse(Format::Ini, ini.as_bytes())),
            vec![("ACH_A".into(), Some(1_700_000_000))]
        );
    }

    #[test]
    fn utf16_ini() {
        let text = "[ACH_A]\r\nAchieved=1\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(
            names(&parse(Format::Ini, &bytes)),
            vec![("ACH_A".into(), None)]
        );
    }

    #[test]
    fn sse_stats_bin() {
        let mut bytes = 2i32.to_le_bytes().to_vec();
        let mut rec = |crc: u32, time: i32, value: i32| {
            let mut r = [0u8; 24];
            r[0..4].copy_from_slice(&crc.to_le_bytes());
            r[8..12].copy_from_slice(&time.to_le_bytes());
            r[20..24].copy_from_slice(&value.to_le_bytes());
            bytes.extend_from_slice(&r);
        };
        rec(0xdeadbeef, 1_700_000_000, 1);
        rec(0x12345678, 0, 57); // a stat, not an achievement
        assert_eq!(
            parse(Format::SseBin, &bytes),
            vec![Earned {
                key: Key::Crc(0xdeadbeef),
                unlocked_at: Some(1_700_000_000)
            }]
        );
        // A count that doesn't match the body is rejected outright.
        assert!(parse(Format::SseBin, &bytes[..bytes.len() - 1]).is_empty());
    }

    #[test]
    fn garbage_is_empty() {
        assert!(parse(Format::Json, b"not json").is_empty());
        assert!(parse(Format::Ini, b"").is_empty());
    }
}
