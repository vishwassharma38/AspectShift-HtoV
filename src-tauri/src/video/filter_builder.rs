use crate::video::preset_adapter::Preset;
use crate::video::types::{OrientationInfo, LogoPosition};

pub fn build_filter_graph(
    preset: &Preset,
    orientation: &OrientationInfo
) -> String {
    let target_ratio = preset.ratio.get_ratio();
    let max_height = 1920;
    let th = orientation.display_height.min(max_height);
    let th = (th as f32 / 2.0).round() as u32 * 2;
    let tw = ((th as f32 * target_ratio) / 2.0).round() as u32 * 2;

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
