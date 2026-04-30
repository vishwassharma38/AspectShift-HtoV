use crate::video::types::{
    AspectRatio, ConversionOptions, OutputFormat, PlatformConfig, VideoError,
};

#[derive(Debug, Clone)]
pub struct FinalConfig {
    pub ratio: AspectRatio,
    pub options: ConversionOptions,
    pub platform_config: Option<PlatformConfig>,
}

#[derive(Debug, Clone)]
pub struct ValidatedConfig {
    pub ratio: AspectRatio,
    pub options: ConversionOptions,
    pub platform_config: Option<PlatformConfig>,
}

pub fn validate_config(config: FinalConfig) -> Result<ValidatedConfig, VideoError> {
    validate_platform_ratio(&config.ratio, config.platform_config.as_ref())?;
    validate_blur_sigma(config.options.blur_sigma)?;
    validate_custom_encoding(
        config.options.custom_encoding_enabled,
        config.options.crf,
        config.options.preset.as_deref(),
    )?;
    validate_audio_bitrate(config.options.remove_audio, config.options.audio_bitrate.as_deref())?;
    validate_transform(config.options.transform.as_ref())?;
    validate_logo(config.options.logo.as_ref())?;
    validate_format_compatibility(&config.options.output_format, config.options.remove_audio)?;

    Ok(ValidatedConfig {
        ratio: config.ratio,
        options: config.options,
        platform_config: config.platform_config,
    })
}

fn validate_platform_ratio(
    ratio: &AspectRatio,
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

fn validate_blur_sigma(blur_sigma: f32) -> Result<(), VideoError> {
    if !blur_sigma.is_finite() || !(0.0..=100.0).contains(&blur_sigma) {
        return Err(VideoError::InvalidInput(
            "blur_sigma must be between 0.0 and 100.0".to_string(),
        ));
    }
    Ok(())
}

fn validate_custom_encoding(
    custom_encoding_enabled: bool,
    crf: Option<u8>,
    preset: Option<&str>,
) -> Result<(), VideoError> {
    if custom_encoding_enabled {
        if let Some(v) = crf {
            if v > 51 {
                return Err(VideoError::InvalidInput(
                    "crf must be between 0 and 51".to_string(),
                ));
            }
        }

        if let Some(p) = preset {
            let normalized = p.trim().to_ascii_lowercase();
            let allowed = [
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
            if !allowed.contains(&normalized.as_str()) {
                return Err(VideoError::InvalidInput(format!(
                    "Unsupported encoding preset: {}",
                    p
                )));
            }
        }
    }
    Ok(())
}

fn validate_audio_bitrate(remove_audio: bool, bitrate: Option<&str>) -> Result<(), VideoError> {
    if remove_audio {
        return Ok(());
    }

    let raw = bitrate.unwrap_or("128k").trim().to_ascii_lowercase();
    let numeric = raw.strip_suffix('k').ok_or_else(|| {
        VideoError::InvalidInput("audio_bitrate must use 'k' suffix, e.g. 128k".to_string())
    })?;
    let parsed = numeric.parse::<u32>().map_err(|_| {
        VideoError::InvalidInput("audio_bitrate must be numeric, e.g. 128k".to_string())
    })?;
    if !(32..=512).contains(&parsed) {
        return Err(VideoError::InvalidInput(
            "audio_bitrate must be between 32k and 512k".to_string(),
        ));
    }
    Ok(())
}

fn validate_transform(transform: Option<&crate::video::types::VideoTransform>) -> Result<(), VideoError> {
    if let Some(t) = transform {
        if !matches!(t.rotate, 0 | 90 | 180 | 270) {
            return Err(VideoError::InvalidInput(
                "transform.rotate must be one of: 0, 90, 180, 270".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_logo(logo: Option<&crate::video::types::LogoOptions>) -> Result<(), VideoError> {
    if let Some(l) = logo {
        if !(0.0..=1.0).contains(&l.opacity) {
            return Err(VideoError::InvalidInput(
                "logo.opacity must be between 0.0 and 1.0".to_string(),
            ));
        }
        if !(0.01..=1.0).contains(&l.scale) {
            return Err(VideoError::InvalidInput(
                "logo.scale must be between 0.01 and 1.0".to_string(),
            ));
        }
        if l.gap > 2000 {
            return Err(VideoError::InvalidInput(
                "logo.gap is too large".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_format_compatibility(
    output_format: &OutputFormat,
    _remove_audio: bool,
) -> Result<(), VideoError> {
    match output_format {
        OutputFormat::Mp4 | OutputFormat::Mov | OutputFormat::Webm => Ok(()),
    }
}
