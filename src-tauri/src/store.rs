use std::fs;
use std::path::{Path, PathBuf};

use crate::models::{HistoryEntry, Settings};

pub fn history_path(data_dir: &Path) -> PathBuf {
    data_dir.join("history.json")
}

pub fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

pub fn load_history(data_dir: &Path) -> Vec<HistoryEntry> {
    fs::read_to_string(history_path(data_dir))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<HistoryEntry>>(&text).ok())
        .unwrap_or_default()
}

pub fn push_history(data_dir: &Path, mut entry: HistoryEntry) -> Vec<HistoryEntry> {
    let mut history = load_history(data_dir);
    entry.id = if entry.id.is_empty() {
        format!("{}-{}", crate::util::now_ts(), history.len())
    } else {
        entry.id
    };
    history.insert(0, entry);
    history.truncate(200);
    if let Some(parent) = history_path(data_dir).parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&history) {
        let _ = fs::write(history_path(data_dir), text);
    }
    history
}

pub fn clear_history(data_dir: &Path) {
    let _ = fs::remove_file(history_path(data_dir));
}

pub fn load_settings(data_dir: &Path) -> Settings {
    fs::read_to_string(settings_path(data_dir))
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(data_dir: &Path, settings: &Settings) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|err| err.to_string())?;
    let text = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    fs::write(settings_path(data_dir), text).map_err(|err| err.to_string())
}
