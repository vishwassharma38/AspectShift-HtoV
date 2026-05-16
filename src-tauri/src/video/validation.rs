use crate::video::types::{
    EncodingProfile, OutputFormat, OutputJob, PlatformConfig, PlatformPreset, VideoEffectsSettings,
    VideoError,
};

const SPEED_PRESETS: &[&str] = &[
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];

const QUALITY_PRESETS: &[&str] = &["draft", "standard", "high"];

pub fn validate_encoding_profile(encoding: &EncodingProfile) -> Result<(), VideoError> {
    if encoding.crf > 51 {
        return Err(VideoError::InvalidInput(
            "encoding.crf must be between 0 and 51".to_string(),
        ));
    }

    let quality = encoding.quality_preset.trim().to_ascii_lowercase();
    if !QUALITY_PRESETS.contains(&quality.as_str()) {
        return Err(VideoError::InvalidInput(format!(
            "Unsupported qualityPreset: {}",
            encoding.quality_preset
        )));
    }

    let speed = encoding.speed_preset.trim().to_ascii_lowercase();
    if !SPEED_PRESETS.contains(&speed.as_str()) {
        return Err(VideoError::InvalidInput(format!(
            "Unsupported speedPreset: {}",
            encoding.speed_preset
        )));
    }

    validate_audio_bitrate(&encoding.audio_bitrate)
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
    }

    match effects.output_format.as_ref().unwrap_or(&OutputFormat::Mp4) {
        OutputFormat::Mp4 | OutputFormat::Mov | OutputFormat::Webm => Ok(()),
    }
}

pub fn validate_output_job(job: &OutputJob) -> Result<(), VideoError> {
    if job.id.trim().is_empty() {
        return Err(VideoError::InvalidInput("job.id cannot be empty".to_string()));
    }

    // Traceability Requirement: Ensure source_id is provided
    if job.selection.source_id.trim().is_empty() {
         return Err(VideoError::InvalidInput("job.selection.sourceId must be specified for traceability".to_string()));
    }

    // 1. Encoding Bounds
    validate_encoding_profile(&job.encoding)?;    
    // 2. Video Effects Bounds
    validate_effects(&job.effects)?;
    
    // 3. Platform / Resolution Safety
    if let Some(config) = &job.platform_config {
        if config.target_width == 0 || config.target_height == 0 {
            return Err(VideoError::InvalidInput("Platform dimensions must be non-zero".to_string()));
        }
        if config.target_width > 16384 || config.target_height > 16384 {
             return Err(VideoError::InvalidInput("Platform dimensions exceed maximum resolution".to_string()));
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

fn validate_audio_bitrate(bitrate: &str) -> Result<(), VideoError> {
    let raw = bitrate.trim().to_ascii_lowercase();
    let numeric = raw.strip_suffix('k').ok_or_else(|| {
        VideoError::InvalidInput("encoding.audioBitrate must use 'k' suffix, e.g. 128k".to_string())
    })?;
    let parsed = numeric.parse::<u32>().map_err(|_| {
        VideoError::InvalidInput("encoding.audioBitrate must be numeric, e.g. 128k".to_string())
    })?;
    if !(32..=512).contains(&parsed) {
        return Err(VideoError::InvalidInput(
            "encoding.audioBitrate must be between 32k and 512k".to_string(),
        ));
    }
    Ok(())
}
