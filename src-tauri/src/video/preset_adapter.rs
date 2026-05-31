use crate::video::types::{
    AspectRatio, EncodingProfile, LogoPreset, PlatformConfig, VideoEffectsSettings, VideoError,
};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RenderPlan {
    pub ratio: AspectRatio,
    pub encoding: EncodingProfile,
    pub effects: VideoEffectsSettings,
    pub platform_config: Option<PlatformConfig>,
    pub logo: Option<LogoPreset>,
}

fn resolve_logo(effects: &VideoEffectsSettings, input: &str) -> Option<LogoPreset> {
    let logo_opts = effects.logo.as_ref()?;
    if !logo_opts.enabled {
        return None;
    }

    let path = if let Some(path) = &logo_opts.path {
        if Path::new(path).exists() {
            Some(path.clone())
        } else {
            None
        }
    } else {
        let input_path = Path::new(input);
        let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
        let logo_file = parent.join("logo.png");
        if logo_file.exists() {
            Some(logo_file.to_string_lossy().to_string())
        } else {
            None
        }
    }?;

    Some(LogoPreset {
        path,
        position: logo_opts.position.clone(),
        opacity: logo_opts.opacity,
        gap: logo_opts.gap,
        scale: logo_opts.scale,
    })
}

pub fn create_render_plan_resolved(
    job: &crate::video::types::ResolvedJob,
) -> Result<RenderPlan, VideoError> {
    let logo = resolve_logo(&job.effects, &job.input_path);
    Ok(RenderPlan {
        ratio: job.ratio.clone(),
        encoding: job.encoding.clone(),
        effects: job.effects.clone(),
        platform_config: job.platform_config.clone(),
        logo,
    })
}
