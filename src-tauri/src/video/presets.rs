use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;
use crate::video::types::{VideoPreset, AspectRatio, ConversionOptions, PlatformConfig, QualityPreset, VideoError};

pub fn get_builtin_presets() -> Vec<VideoPreset> {
    vec![
        VideoPreset {
            id: "youtube_shorts".to_string(),
            name: "YouTube Shorts".to_string(),
            description: Some("Optimized for YouTube Shorts (9:16)".to_string()),
            ratio: AspectRatio::Ratio9x16,
            options: ConversionOptions {
                quality: QualityPreset::High,
                custom_encoding_enabled: true,
                crf: Some(18),
                preset: Some("slow".to_string()),
                audio_bitrate: Some("192k".to_string()),
                ..Default::default()
            },
            platform_config: Some(PlatformConfig {
                target_width: 1080,
                target_height: 1920,
                enforce_dimensions: true,
            }),
            logo_path: None,
            is_builtin: true,
        },
        VideoPreset {
            id: "instagram_reels".to_string(),
            name: "Instagram Reels".to_string(),
            description: Some("Optimized for Instagram Reels & Stories (9:16)".to_string()),
            ratio: AspectRatio::Ratio9x16,
            options: ConversionOptions {
                quality: QualityPreset::Standard,
                custom_encoding_enabled: true,
                crf: Some(23),
                preset: Some("medium".to_string()),
                audio_bitrate: Some("128k".to_string()),
                ..Default::default()
            },
            platform_config: Some(PlatformConfig {
                target_width: 1080,
                target_height: 1920,
                enforce_dimensions: true,
            }),
            logo_path: None,
            is_builtin: true,
        },
        VideoPreset {
            id: "tiktok".to_string(),
            name: "TikTok".to_string(),
            description: Some("Optimized for TikTok (9:16)".to_string()),
            ratio: AspectRatio::Ratio9x16,
            options: ConversionOptions {
                quality: QualityPreset::Standard,
                custom_encoding_enabled: true,
                crf: Some(23),
                preset: Some("medium".to_string()),
                audio_bitrate: Some("128k".to_string()),
                ..Default::default()
            },
            platform_config: Some(PlatformConfig {
                target_width: 1080,
                target_height: 1920,
                enforce_dimensions: true,
            }),
            logo_path: None,
            is_builtin: true,
        },
        VideoPreset {
            id: "twitter_x".to_string(),
            name: "Twitter / X".to_string(),
            description: Some("Optimized for Twitter / X".to_string()),
            ratio: AspectRatio::Ratio16x9,
            options: ConversionOptions {
                quality: QualityPreset::Standard,
                custom_encoding_enabled: true,
                crf: Some(24),
                preset: Some("medium".to_string()),
                audio_bitrate: Some("128k".to_string()),
                ..Default::default()
            },
            platform_config: Some(PlatformConfig {
                target_width: 1280,
                target_height: 720,
                enforce_dimensions: false,
            }),
            logo_path: None,
            is_builtin: true,
        },
        VideoPreset {
            id: "reddit".to_string(),
            name: "Reddit".to_string(),
            description: Some("Optimized for Reddit".to_string()),
            ratio: AspectRatio::Ratio4x5,
            options: ConversionOptions {
                quality: QualityPreset::Standard,
                custom_encoding_enabled: true,
                crf: Some(25),
                preset: Some("medium".to_string()),
                audio_bitrate: Some("128k".to_string()),
                ..Default::default()
            },
            platform_config: Some(PlatformConfig {
                target_width: 1080,
                target_height: 1350,
                enforce_dimensions: false,
            }),
            logo_path: None,
            is_builtin: true,
        },
    ]
}

fn get_presets_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let app_data = app.path().app_data_dir().map_err(|e| VideoError::TauriError(e))?;
    Ok(app_data.join("presets.json"))
}

pub fn load_custom_presets(app: &AppHandle) -> Result<Vec<VideoPreset>, VideoError> {
    let path = get_presets_path(app)?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(path)?;
    let presets: Vec<VideoPreset> = serde_json::from_str(&content)?;
    Ok(presets)
}

pub fn save_custom_preset(app: &AppHandle, mut preset: VideoPreset) -> Result<(), VideoError> {
    let mut presets = load_custom_presets(app)?;
    
    preset.is_builtin = false;
    // Update existing or add new
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
pub fn get_all_presets(app: AppHandle) -> Result<Vec<VideoPreset>, String> {
    let mut all_presets = get_builtin_presets();
    let custom_presets = load_custom_presets(&app).map_err(|e| e.to_string())?;
    all_presets.extend(custom_presets);
    Ok(all_presets)
}

#[tauri::command]
pub fn save_preset(app: AppHandle, preset: VideoPreset) -> Result<(), String> {
    save_custom_preset(&app, preset).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_preset(app: AppHandle, id: String) -> Result<(), String> {
    delete_custom_preset(&app, id).map_err(|e| e.to_string())
}
