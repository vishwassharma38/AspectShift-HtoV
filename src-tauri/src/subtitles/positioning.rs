use crate::subtitles::ass_writer::AssStyle;

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

    let base_margin_v_pct =
        BASE_MARGIN_V_WIDE_PCT + ((BASE_MARGIN_V_TALL_PCT - BASE_MARGIN_V_WIDE_PCT) * portrait_weight);
    let base_margin_v = h * base_margin_v_pct;

    let frame_h = foreground_frame_height.min(target_height) as f32;
    let bottom_gutter = ((h - frame_h) / 2.0).max(0.0);
    let gutter_anchor = if blur_enabled && bottom_gutter > 0.0 {
        bottom_gutter * 0.55
    } else {
        0.0
    };

    let margin_v = base_margin_v.max(gutter_anchor).round() as u32;
    let margin_h = (w * MIN_MARGIN_H_PCT).round() as u32;
    let outline = (font_size * OUTLINE_RATIO).clamp(MIN_OUTLINE, MAX_OUTLINE);

    SubtitleLayoutMetrics {
        font_size: font_size.round() as u32,
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
) -> AssStyle {
    let metrics =
        calculate_layout_metrics(target_width, target_height, foreground_frame_height, blur_enabled);

    AssStyle {
        name: "Professional".to_string(),
        font_name: "Arial".to_string(),
        font_size: metrics.font_size,
        primary_colour: "&H00FFFFFF".to_string(),
        outline_colour: "&H00000000".to_string(),
        back_colour: "&H00000000".to_string(),
        bold: true,
        outline: metrics.outline,
        shadow: 0.0,
        alignment: 2, // Bottom Center
        margin_v: metrics.margin_v,
        play_res_y: metrics.play_res_y,
        play_res_x: metrics.play_res_x,
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
