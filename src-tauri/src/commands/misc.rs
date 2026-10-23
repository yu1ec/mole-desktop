use serde::Serialize;
use tauri::State;

use crate::models::{HistoryEntry, Settings};
use crate::state::AppState;
use crate::store::{clear_history as clear_store, load_history, load_settings, save_settings as save_store};
use crate::util::{home, os_open};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub platform: String,
    pub home: String,
    pub version: String,
    pub data_dir: String,
}

#[tauri::command]
pub fn app_info(state: State<AppState>) -> AppInfo {
    AppInfo {
        platform: if cfg!(target_os = "macos") {
            "macos".into()
        } else if cfg!(windows) {
            "windows".into()
        } else {
            "linux".into()
        },
        home: home().to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: state.data_dir.to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn get_history(state: State<AppState>) -> Vec<HistoryEntry> {
    load_history(&state.data_dir)
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) {
    clear_store(&state.data_dir);
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    load_settings(&state.data_dir)
}

#[tauri::command]
pub fn save_settings(settings: Settings, state: State<AppState>) -> Result<(), String> {
    save_store(&state.data_dir, &settings)
}

#[tauri::command]
pub fn open_data_dir(state: State<AppState>) -> Result<(), String> {
    os_open(&state.data_dir)
}

#[tauri::command]
pub fn open_external(path: String) -> Result<(), String> {
    os_open(&crate::util::expand_tilde(&path))
}
