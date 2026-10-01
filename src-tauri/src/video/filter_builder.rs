use crate::video::overlay_geometry::{overlay_center_x_expression, overlay_center_y_expression};
use crate::video::preset_adapter::RenderPlan;
use crate::video::render_layout::{calculate_render_layout, PreviewFitMode};
use crate::video::types::{ImagePreset, OrientationInfo, VideoTransform};

pub(crate) fn get_transform_filters(transform: &VideoTransform) -> (String, bool) {
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

/// Builds the per-image FFmpeg filter chain (without input/output labels).
///
/// Order: crop -> flip -> scale -> rotation -> format/opacity.
fn image_filter_chain(image: &ImagePreset, target_width: u32) -> String {
    let mut stages: Vec<String> = Vec::new();

    // Crop: which portion of the source image is visible (independent from
    // transform). Fractions of the source dimensions.
    let crop = &image.crop;
    let is_full_crop = (crop.x - 0.0).abs() < 1e-6
        && (crop.y - 0.0).abs() < 1e-6
        && (crop.width - 1.0).abs() < 1e-6
        && (crop.height - 1.0).abs() < 1e-6;
    if !is_full_crop {
        stages.push(format!(
            "crop=w=iw*{w:.6}:h=ih*{h:.6}:x=iw*{x:.6}:y=ih*{y:.6}",
            w = crop.width,
            h = crop.height,
            x = crop.x,
            y = crop.y
        ));
    }

    if image.flip_h {
        stages.push("hflip".to_string());
    }
    if image.flip_v {
        stages.push("vflip".to_string());
    }

    let scale_w = ((target_width as f32 * image.scale).round() as u32).max(2);
    stages.push(format!("scale=w={scale_w}:h=-1"));

    if image.rotation.abs() > 1e-6 {
        // Rotate about the center, expanding the frame so corners are not
        // clipped; transparent fill preserves the overlay alpha.
        stages.push(format!(
            "rotate={rotation:.6}*PI/180:c=black@0:ow=rotw(iw):oh=roth(ih)",
            rotation = image.rotation
        ));
    }

    stages.push(format!(
        "format=rgba,colorchannelmixer=aa={opacity:.6}",
        opacity = image.opacity
    ));

    stages.join(",")
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
    let uses_complex_graph = plan.effects.background_effect_enabled()
        || !plan.images.is_empty()
        || plan.effects.text_overlay_enabled()
        || has_transform;

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
        } else if plan.effects.white_background_enabled() {
            filter_stages.push(format!(
                "[0:v]{transform}[v_transformed];\
                 color=c=white:s={tw}x{th}[bg_white];\
                 [v_transformed]{fg_filter}[fg_scaled];\
                 [bg_white][fg_scaled]overlay=x=(main_w-overlay_w)/2:y=(main_h-overlay_h)/2:shortest=1[v]",
                transform = transform_filter,
                tw = tw,
                th = th,
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
    } else if plan.effects.white_background_enabled() {
        filter_stages.push(format!(
            "color=c=white:s={tw}x{th}[bg_white];\
             [0:v]{fg_filter}[fg_scaled];\
             [bg_white][fg_scaled]overlay=x=(main_w-overlay_w)/2:y=(main_h-overlay_h)/2:shortest=1[v]",
            tw = tw,
            th = th,
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

    // Stage 2: Image Overlays (independent ImageOverlay objects).
    //
    // Canonical pipeline per image:
    // ```text
    // source -> crop -> flip -> scale -> rotation -> position -> frame clipping
    // ```
    // Position is always the canonical center anchor
    // (`main_w*x-overlay_w/2`); no clamping, so off-canvas centers stay
    // off-canvas and the frame clips visibility. Preview and export share
    // the same semantics.
    for (index, image) in plan.images.iter().enumerate() {
        let input_label = index + 1;
        let out_label = format!("img{index}_processed");
        let chain = image_filter_chain(image, tw);
        let x = overlay_center_x_expression(image.x);
        let y = overlay_center_y_expression(image.y);
        filter_stages.push(format!(
            "[{input_label}:v]{chain}[{out_label}];\
             [v][{out_label}]overlay=x={x}:y={y}[v]"
        ));
    }

    filter_stages.join(";")
}

#[cfg(test)]
mod tests {
    use super::{build_filter_graph, image_filter_chain};
    use crate::video::preset_adapter::RenderPlan;
    use crate::video::types::{
        AspectRatio, EncodingProfile, ImageCrop, ImagePreset, OrientationInfo,
        VideoEffectsSettings,
    };

    fn image_preset(x: f32, y: f32) -> ImagePreset {
        ImagePreset {
            path: "a.png".to_string(),
            x,
            y,
            scale: 0.25,
            rotation: 0.0,
            opacity: 1.0,
            flip_h: false,
            flip_v: false,
            crop: ImageCrop::default(),
            is_gif: false,
        }
    }

    fn plan_with_images(images: Vec<ImagePreset>) -> (RenderPlan, OrientationInfo) {
        let effects: VideoEffectsSettings =
            serde_json::from_str("{}").expect("default effects should deserialize");
        let plan = RenderPlan {
            ratio: AspectRatio::Ratio9x16,
            encoding: EncodingProfile::standard(),
            effects,
            platform_config: None,
            images,
        };
        let orientation = OrientationInfo {
            width: 1920,
            height: 1080,
            rotation: 0,
            is_vertical: false,
            display_width: 1920,
            display_height: 1080,
        };
        (plan, orientation)
    }

    #[test]
    fn image_keeps_center_anchor_at_frame_center() {
        let (plan, orientation) = plan_with_images(vec![image_preset(0.5, 0.5)]);
        let graph = build_filter_graph(&plan, &orientation);
        assert!(graph.contains("main_w*0.500000-overlay_w/2"));
        assert!(graph.contains("main_h*0.500000-overlay_h/2"));
    }

    #[test]
    fn image_preserves_signed_off_canvas_coordinates() {
        // Canonical, unbounded geometry: off-canvas centers stay off-canvas
        // in the signed FFmpeg expression; the frame clips visibility.
        let (plan, orientation) = plan_with_images(vec![image_preset(-0.2, 1.1)]);
        let graph = build_filter_graph(&plan, &orientation);
        assert!(graph.contains("main_w*-0.200000-overlay_w/2"));
        assert!(graph.contains("main_h*1.100000-overlay_h/2"));
    }

    #[test]
    fn multiple_images_produce_independent_overlay_stages() {
        let (plan, orientation) =
            plan_with_images(vec![image_preset(0.3, 0.3), image_preset(0.7, 0.7)]);
        let graph = build_filter_graph(&plan, &orientation);
        assert!(graph.contains("[1:v]"));
        assert!(graph.contains("[2:v]"));
        assert!(graph.contains("main_w*0.300000-overlay_w/2"));
        assert!(graph.contains("main_w*0.700000-overlay_w/2"));
    }

    #[test]
    fn image_chain_follows_crop_flip_scale_rotate_opacity_order() {
        let preset = ImagePreset {
            path: "a.png".to_string(),
            x: 0.5,
            y: 0.5,
            scale: 0.25,
            rotation: 15.0,
            opacity: 0.8,
            flip_h: true,
            flip_v: true,
            crop: ImageCrop {
                x: 0.1,
                y: 0.1,
                width: 0.8,
                height: 0.8,
            },
            is_gif: false,
        };
        let chain = image_filter_chain(&preset, 1080);
        let crop_pos = chain.find("crop=").expect("crop stage");
        let hflip_pos = chain.find("hflip").expect("hflip stage");
        let vflip_pos = chain.find("vflip").expect("vflip stage");
        let scale_pos = chain.find("scale=").expect("scale stage");
        let rotate_pos = chain.find("rotate=").expect("rotate stage");
        let opacity_pos = chain
            .find("colorchannelmixer")
            .expect("opacity stage");
        assert!(crop_pos < hflip_pos);
        assert!(hflip_pos < scale_pos);
        assert!(vflip_pos < scale_pos);
        assert!(scale_pos < rotate_pos);
        assert!(rotate_pos < opacity_pos);
    }
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
