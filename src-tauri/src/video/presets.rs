use crate::video::types::{
    AspectRatio, AspectRatioTarget, CustomPreset, EncodingProfile, PlatformPreset, VideoError,
};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

pub fn get_aspect_ratio_targets() -> Vec<AspectRatioTarget> {
    vec![
        AspectRatioTarget {
            id: "ratio9x16".to_string(),
            ratio: AspectRatio::Ratio9x16,
            encoding: EncodingProfile::standard(),
        },
        AspectRatioTarget {
            id: "ratio1x1".to_string(),
            ratio: AspectRatio::Ratio1x1,
            encoding: EncodingProfile::standard(),
        },
        AspectRatioTarget {
            id: "ratio4x5".to_string(),
            ratio: AspectRatio::Ratio4x5,
            encoding: EncodingProfile::standard(),
        },
        AspectRatioTarget {
            id: "ratio2x3".to_string(),
            ratio: AspectRatio::Ratio2x3,
            encoding: EncodingProfile::standard(),
        },
        AspectRatioTarget {
            id: "ratio16x9".to_string(),
            ratio: AspectRatio::Ratio16x9,
            encoding: EncodingProfile::standard(),
        },
    ]
}

pub fn get_builtin_presets() -> Vec<PlatformPreset> {
    const RAW: &str = include_str!("../../resources/builtin_presets.json");
    serde_json::from_str(RAW).expect("builtin_presets.json is malformed — fix the file")
}

fn get_presets_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let app_data = app.path().app_data_dir().map_err(VideoError::TauriError)?;
    Ok(app_data.join("presets.json"))
}

pub fn load_custom_presets(app: &AppHandle) -> Result<Vec<CustomPreset>, VideoError> {
    let path = get_presets_path(app)?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(&path)?;
    let presets: Vec<CustomPreset> = serde_json::from_str(&content).map_err(VideoError::JsonError)?;
    Ok(presets)
}

pub fn save_custom_preset(app: &AppHandle, preset: CustomPreset) -> Result<(), VideoError> {
    let mut presets = load_custom_presets(app)?;

    if let Some(index) = presets.iter().position(|p| p.id == preset.id) {
        presets[index] = preset;
    } else {
        presets.push(preset);
    }

    let path = get_presets_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&presets)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn delete_custom_preset(app: &AppHandle, id: String) -> Result<(), VideoError> {
    let mut presets = load_custom_presets(app)?;
    presets.retain(|p| p.id != id);

    let path = get_presets_path(app)?;
    let content = serde_json::to_string_pretty(&presets)?;
    fs::write(path, content)?;
    Ok(())
}

#[tauri::command]
pub fn get_builtin_platform_presets() -> Vec<PlatformPreset> {
    get_builtin_presets()
}

#[tauri::command]
pub fn get_all_aspect_ratio_targets() -> Vec<AspectRatioTarget> {
    get_aspect_ratio_targets()
}

#[tauri::command]
pub fn save_preset(app: AppHandle, preset: CustomPreset) -> Result<(), String> {
    save_custom_preset(&app, preset).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_preset(app: AppHandle, id: String) -> Result<(), String> {
    delete_custom_preset(&app, id).map_err(|e| e.to_string())
}
