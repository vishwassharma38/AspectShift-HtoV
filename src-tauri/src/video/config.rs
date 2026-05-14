use crate::video::types::{AppConfig, VideoError};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

fn get_config_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let app_data = app.path().app_data_dir().map_err(VideoError::TauriError)?;
    Ok(app_data.join("settings.json"))
}

pub fn load_app_config(app: &AppHandle) -> Result<AppConfig, VideoError> {
    let path = get_config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let content = fs::read_to_string(&path)?;
    let config: AppConfig = serde_json::from_str(&content).map_err(VideoError::JsonError)?;
    Ok(config)
}

pub fn save_app_config(app: &AppHandle, config: AppConfig) -> Result<(), VideoError> {
    let path = get_config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&config)?;
    fs::write(path, content)?;
    Ok(())
}

#[tauri::command]
pub fn get_config(app: AppHandle) -> Result<AppConfig, String> {
    load_app_config(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    save_app_config(&app, config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reset_config(app: AppHandle) -> Result<AppConfig, String> {
    let config = AppConfig::default();
    save_app_config(&app, config.clone()).map_err(|e| e.to_string())?;
    Ok(config)
}
