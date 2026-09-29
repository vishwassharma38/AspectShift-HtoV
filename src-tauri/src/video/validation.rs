use crate::video::encoding::{validate_baseline_profile, validate_encoding_overrides};
use crate::video::types::{
    EncodingProfile, OutputFormat, OutputJob, PlatformConfig, PlatformPreset,
    SubtitleOverlaySettings, TextFontStyle, TextLayerSettings, VideoEffectsSettings, VideoError,
};
use std::collections::HashSet;

// Canonical encoding semantics (quality levels, speed presets, bitrate rules)
// live in `video::encoding`; this module delegates so the two cannot drift
// apart.

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn validate_encoding_profile(encoding: &EncodingProfile) -> Result<(), VideoError> {
    validate_baseline_profile(encoding)
}

pub fn validate_preset(preset: &PlatformPreset) -> Result<(), VideoError> {
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
    validate_platform_ratio(&preset.ratio, preset.platform_config.as_ref())
}

pub fn validate_effects(effects: &VideoEffectsSettings) -> Result<(), VideoError> {
    if effects.blur.unwrap_or(false) && effects.white_background.unwrap_or(false) {
        return Err(VideoError::InvalidInput(
            "effects.blur and effects.whiteBackground cannot both be enabled".to_string(),
        ));
    }

    if let Some(blur_sigma) = effects.blur_sigma {
        if !blur_sigma.is_finite() || !(0.0..=100.0).contains(&blur_sigma) {
            return Err(VideoError::InvalidInput(
                "effects.blurSigma must be between 0.0 and 100.0".to_string(),
            ));
        }
    }

    if let Some(transform) = &effects.transform {
        if !matches!(transform.rotate, 0 | 90 | 180 | 270) {
            return Err(VideoError::InvalidInput(
                "effects.transform.rotate must be one of: 0, 90, 180, 270".to_string(),
            ));
        }
    }

    if let Some(logo) = &effects.logo {
        if !(0.0..=1.0).contains(&logo.opacity) {
            return Err(VideoError::InvalidInput(
                "effects.logo.opacity must be between 0.0 and 1.0".to_string(),
            ));
        }
        if !(0.01..=1.0).contains(&logo.scale) {
            return Err(VideoError::InvalidInput(
                "effects.logo.scale must be between 0.01 and 1.0".to_string(),
            ));
        }
        if logo.gap > 2000 {
            return Err(VideoError::InvalidInput(
                "effects.logo.gap is too large".to_string(),
            ));
        }
        // Phase 4: free-positioned logo coordinates accept any finite value.
        // The video frame clips visibility instead of bounding geometry.
        if !logo.x.is_finite() {
            return Err(VideoError::InvalidInput(
                "effects.logo.x must be a finite number".to_string(),
            ));
        }
        if !logo.y.is_finite() {
            return Err(VideoError::InvalidInput(
                "effects.logo.y must be a finite number".to_string(),
            ));
        }
    }

    if effects.text_overlay.layers.len() > 512 {
        return Err(VideoError::InvalidInput(
            "effects.textOverlay.layers cannot exceed 512 layers".to_string(),
        ));
    }
    let mut layer_ids = HashSet::new();
    for (index, text) in effects.text_overlay.layers.iter().enumerate() {
        validate_text_layer(text, index)?;
        if text.id.trim().is_empty() {
            return Err(VideoError::InvalidInput(format!(
                "effects.textOverlay.layers[{index}].id cannot be empty"
            )));
        }
        if !layer_ids.insert(text.id.clone()) {
            return Err(VideoError::InvalidInput(format!(
                "effects.textOverlay.layers[{index}].id must be unique"
            )));
        }
    }

    validate_subtitle_overlay(&effects.subtitle_overlay)?;

    match effects.output_format.as_ref().unwrap_or(&OutputFormat::Mp4) {
        OutputFormat::Mp4 | OutputFormat::Mov | OutputFormat::Webm => Ok(()),
    }
}

fn validate_subtitle_overlay(subtitle: &SubtitleOverlaySettings) -> Result<(), VideoError> {
    let prefix = "effects.subtitleOverlay";
    if let Some(font_size) = subtitle.font_size {
        if !(12..=240).contains(&font_size) {
            return Err(VideoError::InvalidInput(format!(
                "{prefix}.fontSize must be between 12 and 240"
            )));
        }
    }
    if !subtitle.opacity.is_finite() || !(0.0..=1.0).contains(&subtitle.opacity) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.opacity must be between 0.0 and 1.0"
        )));
    }
    // Phase 4: free-positioned subtitle coordinates accept any finite value.
    if !subtitle.x.is_finite() {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.x must be a finite number"
        )));
    }
    if !subtitle.y.is_finite() {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.y must be a finite number"
        )));
    }
    if !is_hex_color(&subtitle.color) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.color must use #RRGGBB format"
        )));
    }
    if !is_hex_color(&subtitle.outline_color) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.outlineColor must use #RRGGBB format"
        )));
    }
    if let Some(outline_width) = subtitle.outline_width {
        if !(0..=20).contains(&outline_width) {
            return Err(VideoError::InvalidInput(format!(
                "{prefix}.outlineWidth must be between 0 and 20"
            )));
        }
    }
    match &subtitle.font_style {
        TextFontStyle::Clean
        | TextFontStyle::Minimal
        | TextFontStyle::Caption
        | TextFontStyle::Meme
        | TextFontStyle::Creator
        | TextFontStyle::Gaming
        | TextFontStyle::Cyberpunk
        | TextFontStyle::Cinematic
        | TextFontStyle::Retro
        | TextFontStyle::Handwritten => {}
    }
    Ok(())
}

fn validate_text_layer(text: &TextLayerSettings, index: usize) -> Result<(), VideoError> {
    let prefix = format!("effects.textOverlay.layers[{index}]");
    if text.text.chars().count() > 500 {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.text cannot exceed 500 characters"
        )));
    }
    if !(12..=240).contains(&text.font_size) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.fontSize must be between 12 and 240"
        )));
    }
    if !text.opacity.is_finite() || !(0.0..=1.0).contains(&text.opacity) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.opacity must be between 0.0 and 1.0"
        )));
    }
    // Phase 4: free-positioned text coordinates accept any finite value.
    if !text.x.is_finite() {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.x must be a finite number"
        )));
    }
    if !text.y.is_finite() {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.y must be a finite number"
        )));
    }
    if !is_hex_color(&text.color) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.color must use #RRGGBB format"
        )));
    }
    if !is_hex_color(&text.outline_color) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.outlineColor must use #RRGGBB format"
        )));
    }
    if !(0..=20).contains(&text.outline_width) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.outlineWidth must be between 0 and 20"
        )));
    }
    match &text.font_style {
        TextFontStyle::Clean
        | TextFontStyle::Minimal
        | TextFontStyle::Caption
        | TextFontStyle::Meme
        | TextFontStyle::Creator
        | TextFontStyle::Gaming
        | TextFontStyle::Cyberpunk
        | TextFontStyle::Cinematic
        | TextFontStyle::Retro
        | TextFontStyle::Handwritten => {}
    }
    Ok(())
}

pub fn validate_output_job(job: &OutputJob) -> Result<(), VideoError> {
    if job.id.trim().is_empty() {
        return Err(VideoError::InvalidInput(
            "job.id cannot be empty".to_string(),
        ));
    }

    // Traceability Requirement: Ensure source_id is provided
    if job.selection.source_id.trim().is_empty() {
        return Err(VideoError::InvalidInput(
            "job.selection.sourceId must be specified for traceability".to_string(),
        ));
    }

    // 1. Encoding Bounds: canonical baseline plus transient overrides.
    // Both must validate so an invalid render request fails before any
    // ResolvedJob can be constructed (resolution itself re-validates).
    validate_encoding_profile(&job.encoding)?;
    validate_encoding_overrides(&job.encoding_overrides)?;
    // 2. Video Effects Bounds
    validate_effects(&job.effects)?;

    // 3. Platform / Resolution Safety
    if let Some(config) = &job.platform_config {
        if config.target_width == 0 || config.target_height == 0 {
            return Err(VideoError::InvalidInput(
                "Platform dimensions must be non-zero".to_string(),
            ));
        }
        if config.target_width > 16384 || config.target_height > 16384 {
            return Err(VideoError::InvalidInput(
                "Platform dimensions exceed maximum resolution".to_string(),
            ));
        }
    }

    // 4. Aspect Ratio Consistency
    validate_platform_ratio(&job.ratio, job.platform_config.as_ref())
}

fn validate_platform_ratio(
    ratio: &crate::video::types::AspectRatio,
    platform_config: Option<&PlatformConfig>,
) -> Result<(), VideoError> {
    if let Some(config) = platform_config {
        if config.target_width == 0 || config.target_height == 0 {
            return Err(VideoError::InvalidInput(
                "Platform dimensions must be non-zero".to_string(),
            ));
        }

        if config.enforce_dimensions {
            let config_ratio = config.target_width as f32 / config.target_height as f32;
            let target_ratio = ratio.get_ratio();
            if (config_ratio - target_ratio).abs() > 0.01 {
                return Err(VideoError::InvalidInput(format!(
                    "Ratio conflict: target ratio {} does not match enforced platform dimensions {}x{}",
                    ratio.get_tag(),
                    config.target_width,
                    config.target_height
                )));
            }
        }
    }
    Ok(())
}



#[cfg(test)]
mod tests {
    use super::validate_effects;
    use crate::video::types::VideoEffectsSettings;

    fn default_effects() -> VideoEffectsSettings {
        serde_json::from_str("{}").expect("default effects should deserialize")
    }

    #[test]
    fn default_text_overlay_is_valid() {
        assert!(validate_effects(&default_effects()).is_ok());
    }

    #[test]
    fn enabled_text_overlay_allows_empty_text_for_skipped_render_layers() {
        let mut effects = default_effects();
        effects
            .text_overlay
            .layers
            .push(crate::video::types::TextLayerSettings {
                id: "layer-1".to_string(),
                enabled: true,
                text: "   ".to_string(),
                ..crate::video::types::TextLayerSettings::default()
            });
        assert!(validate_effects(&effects).is_ok());
    }

    #[test]
    fn text_overlay_accepts_off_canvas_but_rejects_non_finite() {
        // Phase 4: the frame clips visibility instead of bounding geometry,
        // so finite off-canvas coordinates validate; only non-finite values
        // and unrelated bounds (e.g. fontSize) are rejected.
        let mut effects = default_effects();
        effects
            .text_overlay
            .layers
            .push(crate::video::types::TextLayerSettings {
                id: "layer-1".to_string(),
                x: 1.1,
                y: -0.1,
                ..crate::video::types::TextLayerSettings::default()
            });
        assert!(
            validate_effects(&effects).is_ok(),
            "finite off-canvas x/y must validate"
        );

        effects.text_overlay.layers[0].x = f32::NAN;
        let error = validate_effects(&effects).expect_err("NaN x must fail");
        assert!(error.to_string().contains("textOverlay.layers[0].x"));

        effects.text_overlay.layers[0].x = 0.5;
        effects.text_overlay.layers[0].font_size = 241;
        let error = validate_effects(&effects).expect_err("invalid size must fail");
        assert!(error.to_string().contains("fontSize"));
    }

    #[test]
    fn subtitle_overlay_accepts_off_canvas_but_rejects_non_finite() {
        // Phase 4: same finite-only rule for free-positioned subtitles.
        let mut effects = default_effects();
        effects.subtitle_overlay.manual_position = true;
        effects.subtitle_overlay.x = -0.1;
        effects.subtitle_overlay.y = 1.1;
        assert!(
            validate_effects(&effects).is_ok(),
            "finite off-canvas subtitle x/y must validate"
        );

        effects.subtitle_overlay.x = f32::INFINITY;
        let error = validate_effects(&effects).expect_err("infinite x must fail");
        assert!(error.to_string().contains("subtitleOverlay.x"));

        effects.subtitle_overlay.x = 0.5;
        effects.subtitle_overlay.font_size = Some(241);
        let error = validate_effects(&effects).expect_err("invalid subtitle size must fail");
        assert!(error.to_string().contains("subtitleOverlay.fontSize"));
    }

    #[test]
    fn logo_accepts_off_canvas_but_rejects_non_finite() {
        // Phase 4: same finite-only rule for manual logo coordinates.
        let mut effects = default_effects();
        effects.logo = Some(crate::video::types::LogoOptions {
            enabled: true,
            path: Some("logo.png".to_string()),
            manual_position: true,
            x: -0.2,
            y: 1.2,
            ..crate::video::types::LogoOptions::default()
        });
        assert!(
            validate_effects(&effects).is_ok(),
            "finite off-canvas logo x/y must validate"
        );

        if let Some(logo) = effects.logo.as_mut() {
            logo.x = f32::NAN;
        }
        let error = validate_effects(&effects).expect_err("NaN logo x must fail");
        assert!(error.to_string().contains("effects.logo.x"));
    }

    #[test]
    fn text_overlay_rejects_duplicate_layer_ids() {
        let mut effects = default_effects();
        effects.text_overlay.layers = vec![
            crate::video::types::TextLayerSettings {
                id: "same".to_string(),
                ..crate::video::types::TextLayerSettings::default()
            },
            crate::video::types::TextLayerSettings {
                id: "same".to_string(),
                ..crate::video::types::TextLayerSettings::default()
            },
        ];
        let error = validate_effects(&effects).expect_err("duplicate IDs must fail");
        assert!(error.to_string().contains("must be unique"));
    }

    #[test]
    fn text_overlay_accepts_all_bundled_font_styles() {
        for style in [
            "clean",
            "minimal",
            "caption",
            "meme",
            "creator",
            "gaming",
            "cyberpunk",
            "cinematic",
            "retro",
            "handwritten",
        ] {
            let json = format!(
                r##"{{
                    "blur": false,
                    "textOverlay": {{
                        "panelOpen": true,
                        "layers": [{{
                            "id": "layer-{style}",
                            "enabled": true,
                            "text": "Hello",
                            "fontStyle": "{style}",
                            "fontSize": 48,
                            "color": "#ffffff",
                            "opacity": 1.0,
                            "x": 0.5,
                            "y": 0.5,
                            "outlineEnabled": true,
                            "outlineColor": "#000000",
                            "outlineWidth": 3
                        }}],
                        "selectedLayerIds": ["layer-{style}"]
                    }}
                }}"##
            );
            let effects: VideoEffectsSettings =
                serde_json::from_str(&json).expect("style should deserialize");
            assert!(
                validate_effects(&effects).is_ok(),
                "{style} should validate"
            );
        }
    }
}
