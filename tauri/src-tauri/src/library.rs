use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]  // tolerate missing fields from older library.json files
pub struct GameEntry {
    pub id: String,
    pub game_name: String,
    pub exe_path: String,
    pub cover_image_path: Option<String>,
    // add more fields as you need them
}

impl Default for GameEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            game_name: String::new(),
            exe_path: String::new(),
            cover_image_path: None,
        }
    }
}

fn library_path() -> PathBuf {
    dirs::data_local_dir().unwrap().join("Spool").join("library.json")
}

#[tauri::command]
pub fn list_games() -> Result<Vec<GameEntry>, String> {
    let path = library_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}