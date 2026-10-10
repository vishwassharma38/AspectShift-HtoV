use crate::video::types::{AppConfig, VideoEffectsSettings, VideoError};
use crate::video::validation::validate_effects;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

fn get_config_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let runtime = crate::runtime_paths::RuntimePaths::from_app(app)?;
    Ok(runtime.root().join("settings.json"))
}

fn get_legacy_config_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let app_data = app.path().app_data_dir().map_err(VideoError::TauriError)?;
    Ok(app_data.join("settings.json"))
}

pub fn load_app_config(app: &AppHandle) -> Result<AppConfig, VideoError> {
    let path = get_config_path(app)?;
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        let config: AppConfig = serde_json::from_str(&content).map_err(VideoError::JsonError)?;
        return Ok(config);
    }

    let legacy = get_legacy_config_path(app)?;
    if legacy.exists() {
        let content = fs::read_to_string(&legacy)?;
        let config: AppConfig = serde_json::from_str(&content).map_err(VideoError::JsonError)?;
        return Ok(config);
    }

    Ok(AppConfig::default())
}

pub fn save_app_config(app: &AppHandle, config: AppConfig) -> Result<(), VideoError> {
    // Save-time hygiene for backend-owned value domains only. Rejects
    // clearly invalid values instead of persisting them; free-form UI prefs
    // (paths, ids) are untouched. Load (`load_app_config`) is intentionally
    // NOT validated: already-persisted files must keep loading.
    validate_app_config_for_save(&config)?;

    let path = get_config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&config)?;
    fs::write(path, content)?;
    Ok(())
}

/// Save-time value-domain checks for persisted preferences.
///
/// Covers backend-owned domains only:
/// - `preview_volume`: the app contract is 0–100 (the UI clamps on load);
///   the `u8` carrier allows up to 255, so values above 100 are rejected
///   rather than persisted. Rejected (not clamped): a volume above 100 can
///   never arise from the normal UI flow, so rejection surfaces a real bug
///   instead of hiding it.
/// - `blur_sigma`: same 0.0–100.0 finite domain `validate_effects`
///   enforces at render time.
/// - overlay default sub-objects + `blur`/`white_background`/`background_color`:
///   validated by
///   reusing the render boundary's own `validate_effects` on a synthetic
///   settings value (no parallel logic), so save and render agree.
///
/// Free-form prefs (`last_input_dir`, `last_output_dir`, `last_preset_id`,
/// `selected_ratio_ids`, `selected_preset_ids`, `enable_subfolders`) have
/// no backend-owned domain and are not checked.
pub fn validate_app_config_for_save(config: &AppConfig) -> Result<(), VideoError> {
    if let Some(volume) = config.preview_volume {
        if volume > 100 {
            return Err(VideoError::InvalidInput(
                "config.previewVolume must be between 0 and 100".to_string(),
            ));
        }
    }
    if let Some(sigma) = config.blur_sigma {
        if !sigma.is_finite() || !(0.0..=100.0).contains(&sigma) {
            return Err(VideoError::InvalidInput(
                "config.blurSigma must be between 0.0 and 100.0".to_string(),
            ));
        }
    }
    let effects = VideoEffectsSettings {
        blur: config.blur,
        white_background: config.white_background,
        background_color: config.background_color.clone(),
        overlays: None,
        subtitles: None,
        color_filter: None,
        blur_sigma: config.blur_sigma,
        remove_audio: None,
        export_subtitles: None,
        burn_subtitles: None,
        skip_existing: None,
        output_format: None,
        image_overlay: config.image_overlay.clone().unwrap_or_default(),
        text_overlay: config.text_overlay.clone().unwrap_or_default(),
        subtitle_overlay: config.subtitle_overlay.clone().unwrap_or_default(),
        transform: None,
    };
    validate_effects(&effects)?;
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

#[cfg(test)]
mod tests {
    use super::{validate_app_config_for_save, AppConfig};

    #[test]
    fn default_config_passes_save_validation() {
        assert!(validate_app_config_for_save(&AppConfig::default()).is_ok());
    }

    #[test]
    fn preview_volume_above_100_is_rejected_with_field_path() {
        let config = AppConfig {
            preview_volume: Some(200),
            ..AppConfig::default()
        };
        let err = validate_app_config_for_save(&config).expect_err("volume 200 must fail");
        assert!(err.to_string().contains("previewVolume"), "{err}");

        let config = AppConfig {
            preview_volume: Some(100),
            ..AppConfig::default()
        };
        assert!(validate_app_config_for_save(&config).is_ok());
    }

    #[test]
    fn out_of_range_blur_sigma_is_rejected_with_field_path() {
        for bad in [f32::NAN, f32::INFINITY, -1.0, 150.0] {
            let config = AppConfig {
                blur_sigma: Some(bad),
                ..AppConfig::default()
            };
            let err = validate_app_config_for_save(&config).expect_err("bad blurSigma must fail");
            assert!(err.to_string().contains("blurSigma"), "{err}");
        }
        let config = AppConfig {
            blur_sigma: Some(20.0),
            ..AppConfig::default()
        };
        assert!(validate_app_config_for_save(&config).is_ok());
    }

    #[test]
    fn conflicting_background_flags_are_rejected() {
        let config = AppConfig {
            blur: Some(true),
            white_background: Some(true),
            ..AppConfig::default()
        };
        assert!(validate_app_config_for_save(&config).is_err());
    }

    #[test]
    fn already_persisted_out_of_range_config_still_loads() {
        // Load-time intentionally does NOT validate: an old settings.json
        // with out-of-range values must still deserialize (the UI clamps on
        // load), so existing user data is never discarded at startup.
        let raw = r#"{
            "lastInputDir": null, "lastOutputDir": null,
            "lastPresetId": null, "selectedRatioIds": [],
            "selectedPresetIds": [], "imageOverlay": null,
            "textOverlay": null, "subtitleOverlay": null,
            "blur": null, "whiteBackground": null,
            "blurSigma": 999.0, "enableSubfolders": null,
            "previewVolume": 200
        }"#;
        let config: AppConfig =
            serde_json::from_str(raw).expect("old file must still deserialize");
        assert_eq!(config.preview_volume, Some(200));
        assert!(validate_app_config_for_save(&config).is_err());
    }
}
