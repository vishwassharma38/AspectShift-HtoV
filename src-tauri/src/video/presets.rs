use crate::video::types::{
    AspectRatio, AspectRatioTarget, EncodingProfile, PlatformConfig, PlatformPreset, VideoError,
    VideoPreset,
};
use crate::video::validation::validate_preset;
use serde_json::Value;
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

fn encoding(
    crf: u8,
    quality_preset: &str,
    speed_preset: &str,
    audio_bitrate: &str,
) -> EncodingProfile {
    EncodingProfile {
        crf,
        quality_preset: quality_preset.to_string(),
        speed_preset: speed_preset.to_string(),
        audio_bitrate: audio_bitrate.to_string(),
    }
}

pub fn get_builtin_presets() -> Vec<VideoPreset> {
    vec![
        PlatformPreset {
            id: "youtube".to_string(),
            name: "YouTube".to_string(),
            description: Some("Standard YouTube video (16:9)".to_string()),
            ratio: AspectRatio::Ratio16x9,
            encoding: encoding(18, "high", "slow", "192k"),
            platform_config: Some(PlatformConfig {
                target_width: 1920,
                target_height: 1080,
                enforce_dimensions: true,
            }),
            logo_path: None,
            is_builtin: true,
        },
        PlatformPreset {
            id: "youtube_shorts".to_string(),
            name: "YouTube Shorts".to_string(),
            description: Some("Optimized for YouTube Shorts (9:16)".to_string()),
            ratio: AspectRatio::Ratio9x16,
            encoding: encoding(18, "high", "slow", "192k"),
            platform_config: Some(PlatformConfig {
                target_width: 1080,
                target_height: 1920,
                enforce_dimensions: true,
            }),
            logo_path: None,
            is_builtin: true,
        },
        PlatformPreset {
            id: "instagram".to_string(),
            name: "Instagram Square".to_string(),
            description: Some("Standard Instagram Square (1:1)".to_string()),
            ratio: AspectRatio::Ratio1x1,
            encoding: encoding(23, "standard", "medium", "128k"),
            platform_config: Some(PlatformConfig {
                target_width: 1080,
                target_height: 1080,
                enforce_dimensions: true,
            }),
            logo_path: None,
            is_builtin: true,
        },
        PlatformPreset {
            id: "instagram_reels".to_string(),
            name: "Instagram Reels".to_string(),
            description: Some("Optimized for Instagram Reels & Stories (9:16)".to_string()),
            ratio: AspectRatio::Ratio9x16,
            encoding: encoding(23, "standard", "medium", "128k"),
            platform_config: Some(PlatformConfig {
                target_width: 1080,
                target_height: 1920,
                enforce_dimensions: true,
            }),
            logo_path: None,
            is_builtin: true,
        },
        PlatformPreset {
            id: "tiktok".to_string(),
            name: "TikTok".to_string(),
            description: Some("Optimized for TikTok (9:16)".to_string()),
            ratio: AspectRatio::Ratio9x16,
            encoding: encoding(23, "standard", "medium", "128k"),
            platform_config: Some(PlatformConfig {
                target_width: 1080,
                target_height: 1920,
                enforce_dimensions: true,
            }),
            logo_path: None,
            is_builtin: true,
        },
        PlatformPreset {
            id: "twitter_x".to_string(),
            name: "Twitter / X".to_string(),
            description: Some("Optimized for Twitter / X".to_string()),
            ratio: AspectRatio::Ratio16x9,
            encoding: encoding(24, "standard", "medium", "128k"),
            platform_config: Some(PlatformConfig {
                target_width: 1280,
                target_height: 720,
                enforce_dimensions: false,
            }),
            logo_path: None,
            is_builtin: true,
        },
        PlatformPreset {
            id: "reddit".to_string(),
            name: "Reddit".to_string(),
            description: Some("Optimized for Reddit".to_string()),
            ratio: AspectRatio::Ratio4x5,
            encoding: encoding(25, "standard", "medium", "128k"),
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
    let app_data = app.path().app_data_dir().map_err(VideoError::TauriError)?;
    Ok(app_data.join("presets.json"))
}

fn required_string(value: &Value, key: &str) -> Result<String, VideoError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| VideoError::InvalidInput(format!("Legacy preset missing field: {}", key)))
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn optional_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn legacy_quality_encoding(quality: &str) -> Result<EncodingProfile, VideoError> {
    match quality {
        "draft" => Ok(encoding(28, "draft", "veryfast", "128k")),
        "standard" => Ok(encoding(23, "standard", "medium", "128k")),
        "high" => Ok(encoding(18, "high", "slow", "192k")),
        _ => Err(VideoError::InvalidInput(format!(
            "Unsupported legacy quality preset: {}",
            quality
        ))),
    }
}

fn migrate_legacy_encoding(options: &Value) -> Result<EncodingProfile, VideoError> {
    let custom_encoding_enabled = options
        .get("custom_encoding_enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            VideoError::InvalidInput(
                "Legacy preset options missing field: custom_encoding_enabled".to_string(),
            )
        })?;

    if custom_encoding_enabled {
        let crf = options.get("crf").and_then(Value::as_u64).ok_or_else(|| {
            VideoError::InvalidInput("Legacy preset options missing field: crf".to_string())
        })?;
        let speed_preset = required_string(options, "preset")?;
        let audio_bitrate = required_string(options, "audio_bitrate")?;
        let quality_preset = required_string(options, "quality")?;
        return Ok(EncodingProfile {
            crf: u8::try_from(crf).map_err(|_| {
                VideoError::InvalidInput("Legacy preset crf is out of range".to_string())
            })?,
            quality_preset,
            speed_preset,
            audio_bitrate,
        });
    }

    let quality = required_string(options, "quality")?;
    legacy_quality_encoding(&quality)
}

fn migrate_platform_config(value: &Value) -> Result<Option<PlatformConfig>, VideoError> {
    let Some(config) = value
        .get("platform_config")
        .or_else(|| value.get("platformConfig"))
    else {
        return Ok(None);
    };

    if config.is_null() {
        return Ok(None);
    }

    let target_width = config
        .get("target_width")
        .or_else(|| config.get("targetWidth"))
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            VideoError::InvalidInput("Legacy platform_config missing target_width".to_string())
        })?;
    let target_height = config
        .get("target_height")
        .or_else(|| config.get("targetHeight"))
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            VideoError::InvalidInput("Legacy platform_config missing target_height".to_string())
        })?;
    let enforce_dimensions = config
        .get("enforce_dimensions")
        .or_else(|| config.get("enforceDimensions"))
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            VideoError::InvalidInput(
                "Legacy platform_config missing enforce_dimensions".to_string(),
            )
        })?;

    Ok(Some(PlatformConfig {
        target_width: u32::try_from(target_width).map_err(|_| {
            VideoError::InvalidInput(
                "Legacy platform_config target_width is out of range".to_string(),
            )
        })?,
        target_height: u32::try_from(target_height).map_err(|_| {
            VideoError::InvalidInput(
                "Legacy platform_config target_height is out of range".to_string(),
            )
        })?,
        enforce_dimensions,
    }))
}

fn deserialize_or_migrate_preset(value: Value) -> Result<VideoPreset, VideoError> {
    if value.get("encoding").is_some() {
        return serde_json::from_value(value).map_err(VideoError::JsonError);
    }

    let options = value.get("options").ok_or_else(|| {
        VideoError::InvalidInput("Preset missing encoding and legacy options fields".to_string())
    })?;

    let ratio = value
        .get("ratio")
        .cloned()
        .ok_or_else(|| VideoError::InvalidInput("Legacy preset missing field: ratio".to_string()))
        .and_then(|raw| serde_json::from_value(raw).map_err(VideoError::JsonError))?;

    Ok(PlatformPreset {
        id: required_string(&value, "id")?,
        name: required_string(&value, "name")?,
        description: optional_string(&value, "description"),
        ratio,
        encoding: migrate_legacy_encoding(options)?,
        logo_path: optional_string(&value, "logo_path")
            .or_else(|| optional_string(&value, "logoPath")),
        platform_config: migrate_platform_config(&value)?,
        is_builtin: optional_bool(&value, "is_builtin")
            .or_else(|| optional_bool(&value, "isBuiltin"))
            .unwrap_or(false),
    })
}

pub fn load_custom_presets(app: &AppHandle) -> Result<Vec<VideoPreset>, VideoError> {
    let path = get_presets_path(app)?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(&path)?;
    let raw_presets: Vec<Value> = serde_json::from_str(&content)?;
    let mut presets = Vec::with_capacity(raw_presets.len());
    let mut migrated = false;
    for raw_preset in raw_presets {
        if raw_preset.get("encoding").is_none() {
            migrated = true;
        }
        presets.push(deserialize_or_migrate_preset(raw_preset)?);
    }
    for preset in &presets {
        validate_preset(preset)?;
    }
    if migrated {
        let content = serde_json::to_string_pretty(&presets)?;
        fs::write(path, content)?;
    }
    Ok(presets)
}

pub fn get_all_presets_internal(app: &AppHandle) -> Result<Vec<VideoPreset>, VideoError> {
    let mut all_presets = get_builtin_presets();
    let custom_presets = load_custom_presets(app)?;
    all_presets.extend(custom_presets);
    for preset in &all_presets {
        validate_preset(preset)?;
    }
    Ok(all_presets)
}

pub fn save_custom_preset(app: &AppHandle, mut preset: VideoPreset) -> Result<(), VideoError> {
    validate_preset(&preset)?;
    let mut presets = load_custom_presets(app)?;

    preset.is_builtin = false;
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
    get_all_presets_internal(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_all_aspect_ratio_targets() -> Vec<AspectRatioTarget> {
    get_aspect_ratio_targets()
}

#[tauri::command]
pub fn save_preset(app: AppHandle, preset: VideoPreset) -> Result<(), String> {
    save_custom_preset(&app, preset).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_preset(app: AppHandle, id: String) -> Result<(), String> {
    delete_custom_preset(&app, id).map_err(|e| e.to_string())
}
