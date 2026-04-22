use crate::video::types::{
    AspectRatio, ConversionOptions, LogoPreset, PlatformConfig, QualityPreset,
};

#[derive(Debug, Clone)]
pub struct FfmpegPreset {
    pub ratio: AspectRatio,
    pub blur_background: bool,
    pub blur_sigma: f32,
    pub quality: QualityPreset,
    pub remove_audio: bool,
    pub burn_subtitles: bool,
    pub logo: Option<LogoPreset>,
    pub custom_encoding_enabled: bool,
    pub crf: Option<u8>,
    pub preset: Option<String>,
    pub audio_bitrate: Option<String>,
    pub platform_config: Option<PlatformConfig>,
}

pub fn legacy_to_preset(
    ratio: AspectRatio,
    options: ConversionOptions,
    logo_path: Option<String>,
    platform_config: Option<PlatformConfig>,
) -> FfmpegPreset {
    let logo = if let (Some(path), Some(logo_opts)) = (logo_path, options.logo) {
        Some(LogoPreset {
            path,
            position: logo_opts.position,
            opacity: logo_opts.opacity,
            gap: logo_opts.gap,
            scale: logo_opts.scale,
        })
    } else {
        None
    };

    FfmpegPreset {
        ratio,
        blur_background: options.blur_background,
        blur_sigma: options.blur_sigma,
        quality: options.quality,
        remove_audio: options.remove_audio,
        burn_subtitles: options.burn_subtitles,
        logo,
        crf: options.crf,
        preset: options.preset,
        audio_bitrate: options.audio_bitrate,
        platform_config,
        custom_encoding_enabled: options.custom_encoding_enabled,
    }
}
