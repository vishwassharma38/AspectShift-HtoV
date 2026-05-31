use crate::video::types::{AspectRatioTarget, CustomPreset, PlatformPreset, VideoError};
use crate::video::validation::{validate_encoding_profile, validate_preset};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

pub fn get_builtin_presets() -> Vec<PlatformPreset> {
    const RAW: &str = include_str!("../../resources/presets/platform_specific_presets.json");
    let presets: Vec<PlatformPreset> = serde_json::from_str(RAW)
        .expect("platform_specific_presets.json is malformed - fix the file");

    for preset in &presets {
        validate_preset(preset).expect("platform_specific_presets.json contains an invalid preset");
    }

    presets
}

pub fn get_aspect_ratio_targets() -> Vec<AspectRatioTarget> {
    const RAW: &str = include_str!("../../resources/presets/aspect_ratio_presets.json");
    let targets: Vec<AspectRatioTarget> =
        serde_json::from_str(RAW).expect("aspect_ratio_presets.json is malformed - fix the file");

    let mut ids = HashSet::new();
    for target in &targets {
        assert!(
            !target.id.trim().is_empty(),
            "aspect_ratio_presets.json contains a target with an empty id"
        );
        assert!(
            ids.insert(target.id.clone()),
            "aspect_ratio_presets.json contains a duplicate target id: {}",
            target.id
        );
        validate_encoding_profile(&target.encoding)
            .expect("aspect_ratio_presets.json contains invalid encoding settings");
    }

    targets
}

fn get_presets_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let runtime = crate::runtime_paths::RuntimePaths::from_app(app)?;
    Ok(runtime.root().join("presets.json"))
}

fn get_legacy_presets_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let app_data = app.path().app_data_dir().map_err(VideoError::TauriError)?;
    Ok(app_data.join("presets.json"))
}

pub fn load_custom_presets(app: &AppHandle) -> Result<Vec<CustomPreset>, VideoError> {
    let path = get_presets_path(app)?;
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        let presets: Vec<CustomPreset> =
            serde_json::from_str(&content).map_err(VideoError::JsonError)?;
        return Ok(presets);
    }

    let legacy = get_legacy_presets_path(app)?;
    if legacy.exists() {
        let content = fs::read_to_string(&legacy)?;
        let presets: Vec<CustomPreset> =
            serde_json::from_str(&content).map_err(VideoError::JsonError)?;
        return Ok(presets);
    }

    Ok(vec![])
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
