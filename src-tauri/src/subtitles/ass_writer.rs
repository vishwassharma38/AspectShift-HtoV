use crate::subtitles::SubtitleSegment;
use crate::video::types::VideoError;
use std::fs;
use std::path::Path;

pub struct AssStyle {
    pub name: String,
    pub font_name: String,
    pub font_size: u32,
    pub primary_colour: String,
    pub outline_colour: String,
    pub back_colour: String,
    pub bold: bool,
    pub outline: f32,
    pub shadow: f32,
    pub alignment: u32,
    pub margin_v: u32,
    pub play_res_y: u32,
    pub play_res_x: u32,
}

impl Default for AssStyle {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            font_name: "Arial".to_string(),
            font_size: 48,
            primary_colour: "&H00FFFFFF".to_string(), // White
            outline_colour: "&H00000000".to_string(), // Black
            back_colour: "&H00000000".to_string(),    // Black
            bold: true,
            outline: 2.0,
            shadow: 0.0,
            alignment: 2, // Bottom Center
            margin_v: 50,
            play_res_y: 1080,
            play_res_x: 1920,
        }
    }
}

pub fn format_ass_timestamp(total_ms: u64) -> String {
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms % 3_600_000) / 60_000;
    let seconds = (total_ms % 60_000) / 1_000;
    let centis = (total_ms % 1_000) / 10;
    format!("{hours:01}:{minutes:02}:{seconds:02}.{centis:02}")
}

pub fn write_ass(
    output_path: &Path,
    segments: &[SubtitleSegment],
    style: &AssStyle,
) -> Result<(), VideoError> {
    let mut body = String::new();

    // Script Info
    body.push_str("[Script Info]\n");
    body.push_str("ScriptType: v4.00+\n");
    body.push_str(&format!("PlayResX: {}\n", style.play_res_x));
    body.push_str(&format!("PlayResY: {}\n", style.play_res_y));
    body.push_str("ScaledBorderAndShadow: yes\n\n");

    // Styles
    body.push_str("[V4+ Styles]\n");
    body.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    body.push_str(&format!(
        "Style: {},{},{},{},&H000000FF,{},{},{},0,0,0,100,100,0,0,1,{},{},{},20,20,{},1\n\n",
        style.name,
        style.font_name,
        style.font_size,
        style.primary_colour,
        style.outline_colour,
        style.back_colour,
        if style.bold { -1 } else { 0 },
        style.outline,
        style.shadow,
        style.alignment,
        style.margin_v
    ));

    // Events
    body.push_str("[Events]\n");
    body.push_str(
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
    );

    for segment in segments {
        let start = format_ass_timestamp(segment.start_ms);
        let end = format_ass_timestamp(segment.end_ms);
        let text = segment.text.trim().replace('\n', "\\N");
        body.push_str(&format!(
            "Dialogue: 0,{},{},{},,,0,0,0,{}\n",
            start, end, style.name, text
        ));
    }

    fs::write(output_path, body)?;
    Ok(())
}
