use crate::video::preset_adapter::Preset;
use crate::video::types::OrientationInfo;

pub fn build_filter_graph(
    preset: &Preset,
    orientation: &OrientationInfo
) -> String {
    let target_ratio = preset.ratio.get_ratio();
    let max_height = 1920;
    let th = orientation.display_height.min(max_height);
    let th = (th as f32 / 2.0).round() as u32 * 2;
    let tw = ((th as f32 * target_ratio) / 2.0).round() as u32 * 2;

    if preset.blur_background {
        format!(
            "[0:v]split[bg][fg];\
             [bg]scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th},gblur=sigma={sigma}[bg_blurred];\
             [fg]scale=w={tw}:h={th}:force_original_aspect_ratio=decrease[fg_scaled];\
             [bg_blurred][fg_scaled]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2",
            tw = tw, th = th, sigma = preset.blur_sigma
        )
    } else {
        format!(
            "scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th}",
            tw = tw, th = th
        )
    }
}
