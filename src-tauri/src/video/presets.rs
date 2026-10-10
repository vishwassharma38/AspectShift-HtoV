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
    // Save-time hygiene: reject invalid presets here instead of persisting
    // them and failing later at render time. Reuses the exact value-domain
    // checks the render boundary enforces via `validate_output_job`, so save
    // and render can never disagree. Load (`load_custom_presets`) is
    // intentionally NOT validated: already-persisted files must keep loading.
    validate_custom_preset(&preset)?;

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

/// Save-time validation for user-created presets.
///
/// Reuses the render boundary's own encoding value-domain checks
/// (`validate_encoding_profile`, also enforced via `validate_output_job`
/// before any `ResolvedJob` exists), plus identity hygiene (`id`/`name`
/// non-empty, mirroring `validate_preset` for builtins). Returns the
/// existing `InvalidInput`-style error with a field path.
///
/// Load-time is intentionally NOT validated: `load_custom_presets` must
/// keep tolerating already-persisted files.
pub fn validate_custom_preset(preset: &CustomPreset) -> Result<(), VideoError> {
    if preset.id.trim().is_empty() {
        return Err(VideoError::InvalidInput(
            "preset.id cannot be empty".to_string(),
        ));
    }
    if preset.name.trim().is_empty() {
        return Err(VideoError::InvalidInput(
            "preset.name cannot be empty".to_string(),
        ));
    }
    validate_encoding_profile(&preset.encoding)?;
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

#[cfg(test)]
mod tests {
    use super::{validate_custom_preset, CustomPreset};
    use crate::video::types::{AspectRatio, EncodingProfile};

    fn valid_preset() -> CustomPreset {
        CustomPreset {
            id: "preset-1".to_string(),
            name: "My Preset".to_string(),
            ratio: AspectRatio::Ratio9x16,
            encoding: EncodingProfile::standard(),
        }
    }

    #[test]
    fn valid_custom_preset_passes_save_validation() {
        assert!(validate_custom_preset(&valid_preset()).is_ok());
    }

    #[test]
    fn empty_preset_id_is_rejected_with_field_path() {
        let mut preset = valid_preset();
        preset.id = "   ".to_string();
        let err = validate_custom_preset(&preset).expect_err("empty id must fail");
        assert!(err.to_string().contains("preset.id"), "{err}");
    }

    #[test]
    fn empty_preset_name_is_rejected_with_field_path() {
        let mut preset = valid_preset();
        preset.name = String::new();
        let err = validate_custom_preset(&preset).expect_err("empty name must fail");
        assert!(err.to_string().contains("preset.name"), "{err}");
    }

    #[test]
    fn invalid_preset_encoding_is_rejected() {
        let bad_encodings = [
            EncodingProfile {
                crf: 99,
                ..EncodingProfile::standard()
            },
            EncodingProfile {
                quality_preset: "ultra".to_string(),
                ..EncodingProfile::standard()
            },
            EncodingProfile {
                speed_preset: "ludicrous".to_string(),
                ..EncodingProfile::standard()
            },
            EncodingProfile {
                audio_bitrate: "loud".to_string(),
                ..EncodingProfile::standard()
            },
        ];
        for encoding in bad_encodings {
            let preset = CustomPreset {
                encoding,
                ..valid_preset()
            };
            assert!(
                validate_custom_preset(&preset).is_err(),
                "invalid encoding must fail: {:?}",
                preset.encoding
            );
        }
    }

    #[test]
    fn already_persisted_invalid_preset_still_loads() {
        // Load-time intentionally does NOT validate: a file written before
        // save-time validation (or by hand) must still deserialize so
        // existing user data is never discarded at startup. The invalid
        // value is caught later at render time via `validate_output_job`.
        let raw = r#"[{
            "id": "old-1",
            "name": "Old",
            "ratio": "ratio9x16",
            "encoding": {"crf": 99, "qualityPreset": "ultra",
                         "speedPreset": "ludicrous", "audioBitrate": "loud"}
        }]"#;
        let presets: Vec<CustomPreset> =
            serde_json::from_str(raw).expect("old file must still deserialize");
        assert_eq!(presets.len(), 1);
        assert!(validate_custom_preset(&presets[0]).is_err());
    }

    #[test]
    fn aspect_ratio_targets_keep_crf_with_matching_quality_band_label() {
        // Guards the F-05 label correction: every shipped aspect-ratio target
        // must parse and validate through the production loader, keep its
        // tuned CRF, and carry a `qualityPreset` label that matches the CRF's
        // authoritative band (so UI display and derived labels agree).
        let targets = super::get_aspect_ratio_targets();
        assert_eq!(targets.len(), 5, "all five aspect-ratio targets must load");
        for target in &targets {
            assert_eq!(
                target.encoding.crf, 20,
                "tuned baseline CRF must be preserved for {}",
                target.id
            );
            assert_eq!(
                target.encoding.quality_preset,
                crate::video::encoding::quality_for_crf(target.encoding.crf),
                "stored qualityPreset must match the CRF band for {}",
                target.id
            );
        }
    }
}
