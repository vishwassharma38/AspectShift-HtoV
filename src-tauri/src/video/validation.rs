use crate::video::encoding::{validate_baseline_profile, validate_encoding_overrides};
use crate::video::types::{
    EncodingProfile, ImageOverlay, OutputFormat, OutputJob, PlatformConfig, PlatformPreset,
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
    if let Some(config) = preset.platform_config.as_ref() {
        validate_platform_rate_control(config)?;
    }
    validate_platform_ratio(&preset.ratio, preset.platform_config.as_ref())
}

pub fn validate_effects(effects: &VideoEffectsSettings) -> Result<(), VideoError> {
    if effects.blur.unwrap_or(false) && effects.white_background.unwrap_or(false) {
        return Err(VideoError::InvalidInput(
            "effects.blur and effects.whiteBackground cannot both be enabled".to_string(),
        ));
    }

    if let Some(color) = &effects.background_color {
        if !is_hex_color(color) {
            return Err(VideoError::InvalidInput(
                "effects.backgroundColor must use #RRGGBB format".to_string(),
            ));
        }
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

    if effects.image_overlay.overlays.len() > 128 {
        return Err(VideoError::InvalidInput(
            "effects.imageOverlay.overlays cannot exceed 128 overlays".to_string(),
        ));
    }
    {
        let mut overlay_ids = HashSet::new();
        for (index, overlay) in effects.image_overlay.overlays.iter().enumerate() {
            validate_image_overlay(overlay, index)?;
            if overlay.id.trim().is_empty() {
                return Err(VideoError::InvalidInput(format!(
                    "effects.imageOverlay.overlays[{index}].id cannot be empty"
                )));
            }
            if !overlay_ids.insert(overlay.id.clone()) {
                return Err(VideoError::InvalidInput(format!(
                    "effects.imageOverlay.overlays[{index}].id must be unique"
                )));
            }
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
    if !text.rotation.is_finite() || !(-720.0..=720.0).contains(&text.rotation) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.rotation must be between -720.0 and 720.0"
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

fn validate_image_overlay(overlay: &ImageOverlay, index: usize) -> Result<(), VideoError> {
    let prefix = format!("effects.imageOverlay.overlays[{index}]");
    if overlay.path.trim().is_empty() {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.path cannot be empty"
        )));
    }
    if !overlay.opacity.is_finite() || !(0.0..=1.0).contains(&overlay.opacity) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.opacity must be between 0.0 and 1.0"
        )));
    }
    // Canonical, unbounded geometry: any finite x/y is valid (negative, >1,
    // arbitrarily far outside). The video frame clips visibility.
    if !overlay.x.is_finite() {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.x must be a finite number"
        )));
    }
    if !overlay.y.is_finite() {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.y must be a finite number"
        )));
    }
    // Bounding-box manipulation is the source of scale changes. Internal
    // scale is a width fraction of the video width; allow larger-than-frame
    // values so resized images can exceed the frame.
    if !overlay.scale.is_finite() || !(0.01..=10.0).contains(&overlay.scale) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.scale must be between 0.01 and 10.0"
        )));
    }
    if !overlay.rotation.is_finite() || !(-720.0..=720.0).contains(&overlay.rotation) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.rotation must be between -720.0 and 720.0"
        )));
    }
    // Crop is independent from transform: fractions of the source image.
    let crop = &overlay.crop;
    if !crop.x.is_finite()
        || !crop.y.is_finite()
        || !crop.width.is_finite()
        || !crop.height.is_finite()
    {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.crop must use finite numbers"
        )));
    }
    if !(0.0..=1.0).contains(&crop.x) || !(0.0..=1.0).contains(&crop.y) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.crop x/y must be between 0.0 and 1.0"
        )));
    }
    if !(0.01..=1.0).contains(&crop.width) || !(0.01..=1.0).contains(&crop.height) {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.crop width/height must be between 0.01 and 1.0"
        )));
    }
    if crop.x + crop.width > 1.001 || crop.y + crop.height > 1.001 {
        return Err(VideoError::InvalidInput(format!(
            "{prefix}.crop region must stay within the source image"
        )));
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
        validate_platform_rate_control(config)?;
    }

    // 4. Aspect Ratio Consistency
    validate_platform_ratio(&job.ratio, job.platform_config.as_ref())
}

fn validate_platform_rate_control(config: &PlatformConfig) -> Result<(), VideoError> {
    if let Some(max_frame_rate) = config.max_frame_rate {
        if !(1..=240).contains(&max_frame_rate) {
            return Err(VideoError::InvalidInput(
                "platformConfig.maxFrameRate must be between 1 and 240".to_string(),
            ));
        }
    }

    match (&config.video_max_rate, &config.video_buffer_size) {
        (None, None) => Ok(()),
        (Some(max_rate), Some(buffer_size)) => {
            validate_ffmpeg_rate_value(max_rate, "platformConfig.videoMaxRate")?;
            validate_ffmpeg_rate_value(buffer_size, "platformConfig.videoBufferSize")
        }
        _ => Err(VideoError::InvalidInput(
            "platformConfig.videoMaxRate and videoBufferSize must be provided together".to_string(),
        )),
    }
}

fn validate_ffmpeg_rate_value(value: &str, field: &str) -> Result<(), VideoError> {
    let trimmed = value.trim();
    let numeric = trimmed
        .strip_suffix('M')
        .or_else(|| trimmed.strip_suffix('m'))
        .or_else(|| trimmed.strip_suffix('K'))
        .or_else(|| trimmed.strip_suffix('k'))
        .unwrap_or(trimmed);
    let parsed = numeric.parse::<u32>().map_err(|_| {
        VideoError::InvalidInput(format!("{field} must be a positive integer with optional K/M suffix"))
    })?;
    if parsed == 0 || trimmed.is_empty() {
        return Err(VideoError::InvalidInput(format!(
            "{field} must be greater than zero"
        )));
    }
    Ok(())
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
    use super::{validate_effects, validate_platform_rate_control};
    use crate::video::types::{PlatformConfig, VideoEffectsSettings};

    fn platform_config() -> PlatformConfig {
        PlatformConfig {
            target_width: 1080,
            target_height: 1920,
            enforce_dimensions: true,
            max_frame_rate: None,
            video_max_rate: None,
            video_buffer_size: None,
        }
    }

    #[test]
    fn platform_rate_control_is_optional() {
        assert!(validate_platform_rate_control(&platform_config()).is_ok());
    }

    #[test]
    fn platform_max_frame_rate_accepts_reasonable_caps_and_rejects_invalid_values() {
        let mut config = platform_config();
        config.max_frame_rate = Some(60);
        assert!(validate_platform_rate_control(&config).is_ok());

        config.max_frame_rate = Some(0);
        assert!(validate_platform_rate_control(&config).is_err());

        config.max_frame_rate = Some(241);
        assert!(validate_platform_rate_control(&config).is_err());
    }

    #[test]
    fn platform_rate_control_accepts_paired_positive_ffmpeg_values() {
        let mut config = platform_config();
        config.video_max_rate = Some("25M".to_string());
        config.video_buffer_size = Some("25M".to_string());
        assert!(validate_platform_rate_control(&config).is_ok());
    }

    #[test]
    fn platform_rate_control_rejects_missing_pair_member() {
        let mut config = platform_config();
        config.video_max_rate = Some("8M".to_string());
        let error = validate_platform_rate_control(&config)
            .expect_err("maxrate without bufsize must fail");
        assert!(error.to_string().contains("must be provided together"));
    }

    #[test]
    fn platform_rate_control_rejects_invalid_or_zero_values() {
        let mut config = platform_config();
        config.video_max_rate = Some("8MB".to_string());
        config.video_buffer_size = Some("16M".to_string());
        assert!(validate_platform_rate_control(&config).is_err());

        config.video_max_rate = Some("8M".to_string());
        config.video_buffer_size = Some("0M".to_string());
        assert!(validate_platform_rate_control(&config).is_err());
    }

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
    fn text_overlay_rotation_validates_like_image_rotation() {
        let mut effects = default_effects();
        effects
            .text_overlay
            .layers
            .push(crate::video::types::TextLayerSettings {
                id: "layer-1".to_string(),
                rotation: 45.0,
                ..crate::video::types::TextLayerSettings::default()
            });
        assert!(
            validate_effects(&effects).is_ok(),
            "finite rotation must validate"
        );

        effects.text_overlay.layers[0].rotation = 721.0;
        let error = validate_effects(&effects).expect_err("excess rotation must fail");
        assert!(error.to_string().contains("rotation"));

        effects.text_overlay.layers[0].rotation = f32::NAN;
        let error = validate_effects(&effects).expect_err("NaN rotation must fail");
        assert!(error.to_string().contains("rotation"));
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
    fn image_overlay_accepts_off_canvas_but_rejects_non_finite() {
        // Canonical, unbounded geometry: finite off-canvas centers validate;
        // only non-finite values and unrelated bounds fail.
        let mut effects = default_effects();
        effects.image_overlay.overlays = vec![crate::video::types::ImageOverlay {
            id: "img-1".to_string(),
            path: "a.png".to_string(),
            x: -0.2,
            y: 1.2,
            ..crate::video::types::ImageOverlay::default()
        }];
        assert!(
            validate_effects(&effects).is_ok(),
            "finite off-canvas image x/y must validate"
        );

        effects.image_overlay.overlays[0].x = f32::NAN;
        let error = validate_effects(&effects).expect_err("NaN image x must fail");
        assert!(error.to_string().contains("imageOverlay.overlays[0].x"));
    }

    #[test]
    fn image_overlay_rejects_duplicate_ids_and_bad_crop() {
        let mut effects = default_effects();
        effects.image_overlay.overlays = vec![
            crate::video::types::ImageOverlay {
                id: "same".to_string(),
                path: "a.png".to_string(),
                ..crate::video::types::ImageOverlay::default()
            },
            crate::video::types::ImageOverlay {
                id: "same".to_string(),
                path: "b.png".to_string(),
                ..crate::video::types::ImageOverlay::default()
            },
        ];
        let error = validate_effects(&effects).expect_err("duplicate IDs must fail");
        assert!(error.to_string().contains("must be unique"));

        effects.image_overlay.overlays[1].id = "other".to_string();
        effects.image_overlay.overlays[0].crop.width = 0.0;
        let error = validate_effects(&effects).expect_err("bad crop must fail");
        assert!(error.to_string().contains("crop"));
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
