use crate::subtitles::ass_writer::AssStyle;

pub fn calculate_ass_style(target_width: u32, target_height: u32) -> AssStyle {
    let aspect_ratio = target_width as f32 / target_height as f32;
    
    // We use a virtual resolution for ASS rendering to keep it consistent
    let play_res_y = 1080;
    let play_res_x = (play_res_y as f32 * aspect_ratio) as u32;

    // Font size: approx 5% of height for 1080p -> ~54px
    let font_size = (play_res_y as f32 * 0.05).round() as u32;

    // MarginV (Bottom Margin):
    // 9:16 (0.5625) -> Safe area for mobile UI (TikTok/Reels) is approx 20-25% from bottom
    // 1:1 (1.0) -> Lower third is approx 15-20% from bottom
    // 16:9 (1.77) -> Cinematic is approx 10% from bottom
    let margin_v_pct = if aspect_ratio < 0.6 {
        // 9:16 or thinner
        0.22
    } else if aspect_ratio < 1.1 {
        // 1:1 or 4:5
        0.16
    } else if aspect_ratio < 1.5 {
        // 2:3 or 3:4
        0.12
    } else {
        // 16:9 or wider
        0.08
    };

    let margin_v = (play_res_y as f32 * margin_v_pct).round() as u32;

    AssStyle {
        name: "Professional".to_string(),
        font_name: "Arial".to_string(),
        font_size,
        primary_colour: "&H00FFFFFF".to_string(),
        outline_colour: "&H00000000".to_string(),
        back_colour: "&H00000000".to_string(),
        bold: true,
        outline: 2.5,
        shadow: 0.0,
        alignment: 2, // Bottom Center
        margin_v,
        play_res_y,
        play_res_x,
    }
}

// Deprecated: keep for backward compatibility if needed by old calls, 
// but we should transition all calls to calculate_ass_style
pub fn get_subtitle_style(aspect_ratio: f32) -> String {
    let style = calculate_ass_style(1920, (1920.0 / aspect_ratio) as u32);
    format!("Alignment={},MarginV={}", style.alignment, style.margin_v)
}
