use crate::video::types::{
    LogoPreset, OutputJob, PlatformConfig, VideoEffectsSettings, VideoError,
};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RenderPlan {
    pub job: OutputJob,
    pub logo: Option<LogoPreset>,
    pub platform_config: Option<PlatformConfig>,
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
    } else if let Some(path) = &effects.watermark {
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

pub fn create_render_plan(job: OutputJob, input: &str) -> Result<RenderPlan, VideoError> {
    let logo = resolve_logo(&job.effects, input);
    Ok(RenderPlan {
        platform_config: job.platform_config.clone(),
        job,
        logo,
    })
}
