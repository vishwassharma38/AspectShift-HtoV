use crate::video::preset_adapter::RenderPlan;
use crate::video::render_layout::{calculate_render_layout, PreviewFitMode};
use crate::video::types::{LogoPosition, OrientationInfo, VideoTransform};

fn get_transform_filters(transform: &VideoTransform) -> (String, bool) {
    let mut filters = Vec::new();
    let mut swaps_dimensions = false;

    match transform.rotate {
        90 => {
            filters.push("transpose=1".to_string());
            swaps_dimensions = !swaps_dimensions;
        }
        180 => {
            filters.push("hflip".to_string());
            filters.push("vflip".to_string());
        }
        270 => {
            filters.push("transpose=2".to_string());
            swaps_dimensions = !swaps_dimensions;
        }
        _ => {}
    }

    if transform.flip_h {
        filters.push("hflip".to_string());
    }
    if transform.flip_v {
        filters.push("vflip".to_string());
    }

    (filters.join(","), swaps_dimensions)
}

pub fn build_filter_graph(plan: &RenderPlan, orientation: &OrientationInfo) -> String {
    // 0. Handle transformations first
    let layout = calculate_render_layout(plan, orientation, None);
    let mut transform_filter = String::new();

    if let Some(transform) = &plan.effects.transform {
        let (filters, _) = get_transform_filters(transform);
        transform_filter = filters;
    }
    let tw = layout.target_width;
    let th = layout.target_height;

    let mut filter_stages = Vec::new();
    let has_transform = !transform_filter.is_empty();
    let uses_complex_graph = plan.effects.blur_enabled() || plan.logo.is_some() || has_transform;

    // Determine foreground scaling strategy
    let fg_filter = match layout.foreground_fit {
        PreviewFitMode::Cover => format!(
            "scale=w={fw}:h={fh}:force_original_aspect_ratio=increase,crop={fw}:{fh}",
            fw = layout.foreground_frame_width,
            fh = layout.foreground_frame_height
        ),
        PreviewFitMode::Contain => format!(
            "scale=w={fw}:h={fh}:force_original_aspect_ratio=decrease",
            fw = layout.foreground_frame_width,
            fh = layout.foreground_frame_height
        ),
    };

    // Stage 1: Base Video Processing (Transform/Crop/Blur)
    if has_transform {
        if plan.effects.blur_enabled() {
            filter_stages.push(format!(
                "[0:v]{transform}[v_transformed];\
                 [v_transformed]split[bg][fg];\
                 [bg]scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th},gblur=sigma={sigma}[bg_blurred];\
                 [fg]{fg_filter}[fg_scaled];\
                 [bg_blurred][fg_scaled]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2[v]",
                transform = transform_filter,
                tw = tw,
                th = th,
                sigma = plan.effects.blur_sigma_value(),
                fg_filter = fg_filter
            ));
        } else if uses_complex_graph {
            filter_stages.push(format!(
                "[0:v]{transform},scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th}[v]",
                transform = transform_filter, tw = tw, th = th
            ));
        } else {
            // Should not happen as uses_complex_graph is true if has_transform
            filter_stages.push(format!(
                "{transform},scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th}",
                transform = transform_filter, tw = tw, th = th
            ));
        }
    } else if plan.effects.blur_enabled() {
        filter_stages.push(format!(
            "[0:v]split[bg][fg];\
             [bg]scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th},gblur=sigma={sigma}[bg_blurred];\
             [fg]{fg_filter}[fg_scaled];\
             [bg_blurred][fg_scaled]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2[v]",
            tw = tw,
            th = th,
            sigma = plan.effects.blur_sigma_value(),
            fg_filter = fg_filter
        ));
    } else if uses_complex_graph {
        filter_stages.push(format!(
            "[0:v]scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th}[v]",
            tw = tw,
            th = th
        ));
    } else {
        filter_stages.push(format!(
            "scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th}",
            tw = tw,
            th = th
        ));
    }

    // Stage 2: Logo Overlay (Optional)
    if let Some(logo) = &plan.logo {
        let (x, y) = match logo.position {
            LogoPosition::TopLeft => (logo.gap.to_string(), logo.gap.to_string()),
            LogoPosition::TopRight => (
                format!("main_w-overlay_w-{}", logo.gap),
                logo.gap.to_string(),
            ),
            LogoPosition::BottomLeft => (
                logo.gap.to_string(),
                format!("main_h-overlay_h-{}", logo.gap),
            ),
            LogoPosition::BottomRight => (
                format!("main_w-overlay_w-{}", logo.gap),
                format!("main_h-overlay_h-{}", logo.gap),
            ),
        };

        let logo_scale_w = (tw as f32 * logo.scale).round() as u32;

        filter_stages.push(format!(
            "[1:v]scale=w={lw}:h=-1,format=rgba,colorchannelmixer=aa={opacity}[logo_processed];\
             [v][logo_processed]overlay=x={x}:y={y}[v]",
            lw = logo_scale_w,
            opacity = logo.opacity,
            x = x,
            y = y
        ));
    }

    filter_stages.join(";")
}

pub fn validate_preset_consistency(plan: &RenderPlan) -> Result<(), String> {
    if let Some(config) = &plan.platform_config {
        if config.enforce_dimensions {
            let config_ratio = config.target_width as f32 / config.target_height as f32;
            let preset_ratio = plan.ratio.get_ratio();

            // Allow for small floating point differences
            if (config_ratio - preset_ratio).abs() > 0.01 {
                return Err(format!(
                    "Ratio conflict: Preset ratio is {}, but platform requires {}x{} ({:.2})",
                    plan.ratio.get_tag(),
                    config.target_width,
                    config.target_height,
                    config_ratio
                ));
            }
        }
    }
    Ok(())
}
