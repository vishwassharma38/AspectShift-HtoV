use crate::video::preset_adapter::FfmpegPreset;
use crate::video::types::{OrientationInfo, LogoPosition, AspectRatio};

pub fn build_filter_graph(
    preset: &FfmpegPreset,
    orientation: &OrientationInfo
) -> String {
    let max_height = 1920;
    
    // 1. Determine base dimensions from PRESET and ENFORCEMENT
    let (mut tw, mut th) = if let Some(config) = &preset.platform_config {
        if config.enforce_dimensions {
            // Preset enforces specific platform dimensions
            (config.target_width, config.target_height)
        } else {
            // Preset has platform config but DOES NOT enforce dimensions
            // Fall back to dynamic scaling based on ratio
            let target_ratio = preset.ratio.get_ratio();
            let h = orientation.display_height.min(max_height);
            let rounded_h = (h as f32 / 2.0).round() as u32 * 2;
            let w = (rounded_h as f32 * target_ratio) as u32;
            (w, rounded_h)
        }
    } else {
        // No platform config, use dynamic scaling based on target ratio
        let target_ratio = preset.ratio.get_ratio();
        let h = orientation.display_height.min(max_height);
        let rounded_h = (h as f32 / 2.0).round() as u32 * 2;
        let w = (rounded_h as f32 * target_ratio) as u32;
        (w, rounded_h)
    };

    // 2. Ensure ALL dimensions are even (FFmpeg requirement)
    tw = (tw as f32 / 2.0).round() as u32 * 2;
    th = (th as f32 / 2.0).round() as u32 * 2;

    let mut filter_stages = Vec::new();
    let uses_complex_graph = preset.blur_background || preset.logo.is_some();

    // Stage 1: Base Video Processing (Crop/Blur)
    if preset.blur_background {
        filter_stages.push(format!(
            "[0:v]split[bg][fg];\
             [bg]scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th},gblur=sigma={sigma}[bg_blurred];\
             [fg]scale=w={tw}:h={th}:force_original_aspect_ratio=decrease[fg_scaled];\
             [bg_blurred][fg_scaled]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2[v]",
            tw = tw, th = th, sigma = preset.blur_sigma
        ));
    } else if uses_complex_graph {
        filter_stages.push(format!(
            "[0:v]scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th}[v]",
            tw = tw, th = th
        ));
    } else {
        filter_stages.push(format!(
            "scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th}",
            tw = tw, th = th
        ));
    }

    // Stage 2: Logo Overlay (Optional)
    if let Some(logo) = &preset.logo {
        let (x, y) = match logo.position {
            LogoPosition::TopLeft => (logo.gap.to_string(), logo.gap.to_string()),
            LogoPosition::TopRight => (format!("main_w-overlay_w-{}", logo.gap), logo.gap.to_string()),
            LogoPosition::BottomLeft => (logo.gap.to_string(), format!("main_h-overlay_h-{}", logo.gap)),
            LogoPosition::BottomRight => (format!("main_w-overlay_w-{}", logo.gap), format!("main_h-overlay_h-{}", logo.gap)),
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

pub fn validate_preset_consistency(preset: &FfmpegPreset) -> Result<(), String> {
    if let Some(config) = &preset.platform_config {
        if config.enforce_dimensions {
            let config_ratio = config.target_width as f32 / config.target_height as f32;
            let preset_ratio = preset.ratio.get_ratio();
            
            // Allow for small floating point differences
            if (config_ratio - preset_ratio).abs() > 0.01 {
                return Err(format!(
                    "Ratio conflict: Preset ratio is {}, but platform requires {}x{} ({:.2})",
                    preset.ratio.get_tag(),
                    config.target_width,
                    config.target_height,
                    config_ratio
                ));
            }
        }
    }
    Ok(())
}
