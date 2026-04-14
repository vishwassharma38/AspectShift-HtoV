use crate::video::types::{AspectRatio, ConversionOptions, QualityPreset};

#[derive(Debug, Clone)]
pub struct Preset {
    pub ratio: AspectRatio,
    pub blur_background: bool,
    pub blur_sigma: f32,
    pub quality: QualityPreset,
    pub remove_audio: bool,
}

pub fn legacy_to_preset(
    ratio: AspectRatio,
    options: ConversionOptions
) -> Preset {
    Preset {
        ratio,
        blur_background: options.blur_background,
        blur_sigma: options.blur_sigma,
        quality: options.quality,
        remove_audio: options.remove_audio,
    }
}
