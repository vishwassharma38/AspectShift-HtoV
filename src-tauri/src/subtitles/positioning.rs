use crate::subtitles::ass_writer::AssStyle;
use crate::video::types::SubtitleOverlaySettings;

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleLayoutMetrics {
    pub font_size: u32,
    pub outline: f32,
    pub margin_v: u32,
    pub margin_h: u32,
    pub play_res_x: u32,
    pub play_res_y: u32,
}

fn hex_to_ass_colour(hex: &str, opacity: f32) -> String {
    let rgb = hex.strip_prefix('#').unwrap_or(hex);
    let (red, green, blue) = if rgb.len() == 6 {
        (&rgb[0..2], &rgb[2..4], &rgb[4..6])
    } else {
        ("FF", "FF", "FF")
    };
    let alpha = ((1.0 - opacity.clamp(0.0, 1.0)) * 255.0).round() as u8;
    format!("&H{alpha:02X}{blue}{green}{red}")
}

const REF_WIDTH: f32 = 1920.0;
const REF_HEIGHT: f32 = 1080.0;
const REF_FONT_SIZE: f32 = 54.0;
const MIN_FONT_SIZE: f32 = 24.0;
const MAX_FONT_SIZE: f32 = 78.0;
const MIN_MARGIN_H_PCT: f32 = 0.05;
const BASE_MARGIN_V_WIDE_PCT: f32 = 0.08;
const BASE_MARGIN_V_TALL_PCT: f32 = 0.22;
const OUTLINE_RATIO: f32 = 0.055;
const MIN_OUTLINE: f32 = 1.4;
const MAX_OUTLINE: f32 = 4.5;

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

pub fn calculate_layout_metrics(
    target_width: u32,
    target_height: u32,
    foreground_frame_height: u32,
    blur_enabled: bool,
    subtitle_overlay: &SubtitleOverlaySettings,
) -> SubtitleLayoutMetrics {
    let w = target_width.max(2) as f32;
    let h = target_height.max(2) as f32;
    let aspect_ratio = w / h;

    let area_scale = ((w * h) / (REF_WIDTH * REF_HEIGHT)).sqrt();
    let portrait_weight = clamp01((1.2 - aspect_ratio) / 0.7);

    let mut font_size = (REF_FONT_SIZE * area_scale) * (1.0 - (0.12 * portrait_weight));
    font_size = font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);

    let min_font_by_short_side = (h.min(w) * 0.028).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
    if font_size < min_font_by_short_side {
        font_size = min_font_by_short_side;
    }

    let base_margin_v_pct = BASE_MARGIN_V_WIDE_PCT
        + ((BASE_MARGIN_V_TALL_PCT - BASE_MARGIN_V_WIDE_PCT) * portrait_weight);
    let base_margin_v = h * base_margin_v_pct;

    let frame_h = foreground_frame_height.min(target_height) as f32;
    let bottom_gutter = ((h - frame_h) / 2.0).max(0.0);

    let margin_v = if blur_enabled && bottom_gutter > 0.0 {
        // Ensure subtitles are on the foreground video by adjusting the margin to be at least
        // above the blurred gutter area. We use a small fraction of the base margin as a buffer.
        base_margin_v.max(bottom_gutter + (base_margin_v * 0.2))
    } else {
        base_margin_v
    }
    .round() as u32;
    let margin_h = (w * MIN_MARGIN_H_PCT).round() as u32;
    let outline = (font_size * OUTLINE_RATIO).clamp(MIN_OUTLINE, MAX_OUTLINE);

    let font_size = subtitle_overlay
        .font_size
        .map(|size| size.max(1) as u32)
        .unwrap_or_else(|| font_size.round() as u32);
    let outline = if subtitle_overlay.outline_enabled {
        subtitle_overlay
            .outline_width
            .map(|width| width.max(0) as f32)
            .unwrap_or(outline)
    } else {
        0.0
    };

    SubtitleLayoutMetrics {
        font_size,
        outline,
        margin_v,
        margin_h,
        play_res_x: target_width.max(2),
        play_res_y: target_height.max(2),
    }
}

pub fn calculate_ass_style(
    target_width: u32,
    target_height: u32,
    foreground_frame_height: u32,
    blur_enabled: bool,
    subtitle_overlay: &SubtitleOverlaySettings,
) -> AssStyle {
    let metrics = calculate_layout_metrics(
        target_width,
        target_height,
        foreground_frame_height,
        blur_enabled,
        subtitle_overlay,
    );
    let text_font = crate::video::text_fonts::family(&subtitle_overlay.font_style);

    AssStyle {
        name: "Professional".to_string(),
        font_name: text_font.ass_name.to_string(),
        // Same em-vs-win-cell correction as text overlays. `metrics.font_size`
        // (used by the preview layout) keeps its em meaning; only the ASS
        // value is upscaled.
        font_size: ((metrics.font_size as f32 * text_font.ass_cell_ratio)
            .round()
            .max(1.0)) as u32,
        primary_colour: hex_to_ass_colour(&subtitle_overlay.color, subtitle_overlay.opacity),
        outline_colour: hex_to_ass_colour(
            &subtitle_overlay.outline_color,
            subtitle_overlay.opacity,
        ),
        back_colour: "&H00000000".to_string(),
        bold: subtitle_overlay.bold,
        italic: subtitle_overlay.italic,
        underline: false,
        strikethrough: false,
        outline: metrics.outline,
        shadow: 0.0,
        alignment: if subtitle_overlay.manual_position {
            5
        } else {
            2
        },
        margin_v: metrics.margin_v,
        play_res_y: metrics.play_res_y,
        play_res_x: metrics.play_res_x,
        position: subtitle_overlay
            .manual_position
            .then_some((subtitle_overlay.x, subtitle_overlay.y)),
        // Subtitle preview has no `letterSpacing`; keep ASS spacing at 0.
        spacing: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::calculate_ass_style;
    use crate::video::types::SubtitleOverlaySettings;

    #[test]
    fn subtitle_ass_font_size_uses_win_cell_correction() {
        // Preview keeps the em meaning; only the ASS export is upscaled.
        // Clean/Fira Sans ratio is 1.2: explicit 48 -> ASS 58.
        let overlay = SubtitleOverlaySettings {
            font_style: crate::video::types::TextFontStyle::Clean,
            font_size: Some(48),
            ..SubtitleOverlaySettings::default()
        };
        let style = calculate_ass_style(1280, 720, 720, false, &overlay);
        assert_eq!(style.font_size, 58);
        assert_eq!(style.spacing, 0.0);
    }

    #[test]
    fn subtitle_ass_bungee_correction_matches_text_path() {
        // Retro/Bungee ratio is 2.574: explicit 48 -> ASS 124, same as the
        // text-overlay conversion for the same nominal size.
        let overlay = SubtitleOverlaySettings {
            font_style: crate::video::types::TextFontStyle::Retro,
            font_size: Some(48),
            ..SubtitleOverlaySettings::default()
        };
        let style = calculate_ass_style(1280, 720, 720, false, &overlay);
        assert_eq!(style.font_size, 124);
    }
}

pub fn to_srt_force_style(metrics: &SubtitleLayoutMetrics) -> String {
    format!(
        "Alignment=2,MarginL={margin_h},MarginR={margin_h},MarginV={margin_v},FontName=Arial,FontSize={font_size},Bold=1,Outline={outline:.2},Shadow=0",
        margin_h = metrics.margin_h,
        margin_v = metrics.margin_v,
        font_size = metrics.font_size,
        outline = metrics.outline
    )
}
