use crate::video::types::{AspectRatio, EncodingProfile, ImagePreset, PlatformConfig,
                           VideoEffectsSettings, VideoError};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RenderPlan {
    pub ratio: AspectRatio,
    pub encoding: EncodingProfile,
    pub effects: VideoEffectsSettings,
    pub platform_config: Option<PlatformConfig>,
    pub images: Vec<ImagePreset>,
}

fn resolve_images(effects: &VideoEffectsSettings) -> Vec<ImagePreset> {
    effects
        .image_overlay
        .overlays
        .iter()
        .filter(|overlay| !overlay.path.trim().is_empty())
        .filter(|overlay| Path::new(&overlay.path).exists())
        .map(|overlay| ImagePreset {
            path: overlay.path.clone(),
            x: overlay.x,
            y: overlay.y,
            scale: overlay.scale,
            rotation: overlay.rotation,
            opacity: overlay.opacity,
            flip_h: overlay.flip_horizontal,
            flip_v: overlay.flip_vertical,
            crop: overlay.crop.clone(),
            is_gif: ImagePreset::is_gif_path(&overlay.path),
        })
        .collect()
}

pub fn create_render_plan_resolved(
    job: &crate::video::types::ResolvedJob,
) -> Result<RenderPlan, VideoError> {
    let images = resolve_images(&job.effects);
    Ok(RenderPlan {
        ratio: job.ratio.clone(),
        encoding: job.encoding.clone(),
        effects: job.effects.clone(),
        platform_config: job.platform_config.clone(),
        images,
    })
}
