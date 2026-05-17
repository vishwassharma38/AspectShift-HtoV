use crate::video::preset_adapter::RenderPlan;
use crate::video::types::OrientationInfo;
use crate::subtitles::positioning::{calculate_layout_metrics, SubtitleLayoutMetrics};

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRenderLayout {
    pub target_width: u32,
    pub target_height: u32,
    pub background_fit: PreviewFitMode,
    pub foreground_fit: PreviewFitMode,
    pub foreground_frame_width: u32,
    pub foreground_frame_height: u32,
    pub blur_sigma: f32,
    pub logo_width: Option<u32>,
    pub logo_gap: Option<u32>,
    pub subtitle: SubtitleLayoutMetrics,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PreviewFitMode {
    Cover,
    Contain,
}

pub fn calculate_render_layout(
    plan: &RenderPlan,
    orientation: &OrientationInfo,
    target_aspect_ratio_override: Option<f32>,
) -> PreviewRenderLayout {
    let max_height = 1920;

    let (mut effective_display_width, mut effective_display_height) =
        (orientation.display_width, orientation.display_height);

    if let Some(transform) = &plan.effects.transform {
        let rotate = ((transform.rotate % 360) + 360) % 360;
        if rotate == 90 || rotate == 270 {
            std::mem::swap(&mut effective_display_width, &mut effective_display_height);
        }
    }

    let resolved_target_ratio = target_aspect_ratio_override.unwrap_or_else(|| plan.ratio.get_ratio());

    let (mut target_width, mut target_height) = if let Some(config) = &plan.platform_config {
        if config.enforce_dimensions {
            (config.target_width, config.target_height)
        } else {
            let h = effective_display_height.min(max_height);
            let rounded_h = (h as f32 / 2.0).round() as u32 * 2;
            let w = (rounded_h as f32 * resolved_target_ratio) as u32;
            (w, rounded_h)
        }
    } else {
        let h = effective_display_height.min(max_height);
        let rounded_h = (h as f32 / 2.0).round() as u32 * 2;
        let w = (rounded_h as f32 * resolved_target_ratio) as u32;
        (w, rounded_h)
    };

    target_width = (target_width as f32 / 2.0).round() as u32 * 2;
    target_height = (target_height as f32 / 2.0).round() as u32 * 2;

    let is_target_vertical_9x16 = (resolved_target_ratio - (9.0 / 16.0)).abs() < 0.01;
    let blur_enabled = plan.effects.blur_enabled();
    let use_portrait_foreground_crop = blur_enabled
        && is_target_vertical_9x16
        && effective_display_width > effective_display_height;

    let (foreground_fit, foreground_frame_width, foreground_frame_height) =
        if blur_enabled && use_portrait_foreground_crop {
            let fg_w = target_width;
            let fg_h = ((fg_w as f32 * 1.25 / 2.0).round() as u32) * 2;
            (PreviewFitMode::Cover, fg_w, fg_h)
        } else if blur_enabled {
            (PreviewFitMode::Contain, target_width, target_height)
        } else {
            (PreviewFitMode::Cover, target_width, target_height)
        };

    let logo_width = plan
        .logo
        .as_ref()
        .map(|logo| (target_width as f32 * logo.scale).round() as u32);
    let logo_gap = plan.logo.as_ref().map(|logo| logo.gap);
    let subtitle = calculate_layout_metrics(
        target_width,
        target_height,
        foreground_frame_height,
        blur_enabled,
    );

    PreviewRenderLayout {
        target_width,
        target_height,
        background_fit: PreviewFitMode::Cover,
        foreground_fit,
        foreground_frame_width,
        foreground_frame_height,
        blur_sigma: plan.effects.blur_sigma_value(),
        logo_width,
        logo_gap,
        subtitle,
    }
}
