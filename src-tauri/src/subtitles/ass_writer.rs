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
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub outline: f32,
    pub shadow: f32,
    pub alignment: u32,
    pub margin_v: u32,
    pub play_res_y: u32,
    pub play_res_x: u32,
    pub position: Option<(f32, f32)>,
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
            italic: false,
            underline: false,
            strikethrough: false,
            outline: 2.0,
            shadow: 0.0,
            alignment: 2, // Bottom Center
            margin_v: 50,
            play_res_y: 1080,
            play_res_x: 1920,
            position: None,
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
        "Style: {},{},{},{},&H000000FF,{},{},{},{},{},{},100,100,0,0,1,{},{},{},20,20,{},1\n\n",
        style.name,
        style.font_name,
        style.font_size,
        style.primary_colour,
        style.outline_colour,
        style.back_colour,
        if style.bold { -1 } else { 0 },
        if style.italic { -1 } else { 0 },
        if style.underline { -1 } else { 0 },
        if style.strikethrough { -1 } else { 0 },
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
        let text = if let Some((x, y)) = style.position {
            let position_x = (x.clamp(0.0, 1.0) * style.play_res_x as f32).round() as u32;
            let position_y = (y.clamp(0.0, 1.0) * style.play_res_y as f32).round() as u32;
            format!(
                "{{\\an{}\\pos({position_x},{position_y})}}{text}",
                style.alignment
            )
        } else {
            text
        };
        body.push_str(&format!(
            "Dialogue: 0,{},{},{},,,0,0,0,{}\n",
            start, end, style.name, text
        ));
    }

    fs::write(output_path, body)?;
    Ok(())
}

fn escape_ass_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace("\r\n", "\\N")
        .replace(['\r', '\n'], "\\N")
}

/// Writes a persistent, full-duration text layer using the same ASS renderer as
/// burned subtitles. ASS is used here because it supports all four formatting
/// flags consistently, including underline and strikeout.
pub fn write_text_overlay_ass(
    output_path: &Path,
    text: &str,
    style: &AssStyle,
    x: f32,
    y: f32,
    duration_ms: u64,
) -> Result<(), VideoError> {
    write_text_overlays_ass(output_path, &[(text, style, x, y)], duration_ms)
}

pub fn write_text_overlays_ass(
    output_path: &Path,
    layers: &[(&str, &AssStyle, f32, f32)],
    duration_ms: u64,
) -> Result<(), VideoError> {
    let mut body = String::new();
    body.push_str("[Script Info]\n");
    body.push_str("ScriptType: v4.00+\n");
    let (play_res_x, play_res_y) = layers
        .first()
        .map(|(_, style, _, _)| (style.play_res_x, style.play_res_y))
        .unwrap_or((1920, 1080));
    body.push_str(&format!("PlayResX: {}\n", play_res_x));
    body.push_str(&format!("PlayResY: {}\n", play_res_y));
    body.push_str("ScaledBorderAndShadow: yes\n\n");
    body.push_str("[V4+ Styles]\n");
    body.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    for (_, style, _, _) in layers {
        body.push_str(&format!(
            "Style: {},{},{},{},&H000000FF,{},{},{},{},{},{},100,100,0,0,1,{},{},5,0,0,0,1\n",
            style.name,
            style.font_name,
            style.font_size,
            style.primary_colour,
            style.outline_colour,
            style.back_colour,
            if style.bold { -1 } else { 0 },
            if style.italic { -1 } else { 0 },
            if style.underline { -1 } else { 0 },
            if style.strikethrough { -1 } else { 0 },
            style.outline,
            style.shadow,
        ));
    }
    body.push('\n');
    body.push_str("[Events]\n");
    body.push_str(
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
    );

    let end = format_ass_timestamp(duration_ms.max(10));
    for (index, (text, style, x, y)) in layers.iter().enumerate() {
        let position_x = (x.clamp(0.0, 1.0) * style.play_res_x as f32).round() as u32;
        let position_y = (y.clamp(0.0, 1.0) * style.play_res_y as f32).round() as u32;
        body.push_str(&format!(
            "Dialogue: {index},0:00:00.00,{end},{},,0,0,0,,{{\\an5\\pos({position_x},{position_y})}}{}\n",
            style.name,
            escape_ass_text(text),
        ));
    }

    fs::write(output_path, body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{write_ass, write_text_overlay_ass, write_text_overlays_ass, AssStyle};
    use crate::subtitles::SubtitleSegment;

    #[test]
    fn text_overlay_ass_combines_all_formatting_flags() {
        let path = std::env::temp_dir().join(format!(
            "aspectshift_text_overlay_{}.ass",
            uuid::Uuid::new_v4()
        ));
        let style = AssStyle {
            bold: true,
            italic: true,
            underline: true,
            strikethrough: true,
            ..AssStyle::default()
        };

        write_text_overlay_ass(&path, "Hello\n{world}", &style, 0.25, 0.75, 5_000)
            .expect("text overlay ASS should be written");
        let content = std::fs::read_to_string(&path).expect("ASS should be readable");
        let _ = std::fs::remove_file(path);

        assert!(content.contains("-1,-1,-1,-1,100,100"));
        assert!(content.contains("\\pos(480,810)"));
        assert!(content.contains("Hello\\N\\{world\\}"));
    }

    #[test]
    fn text_overlay_ass_writes_multiple_layers_in_order() {
        let path = std::env::temp_dir().join(format!(
            "aspectshift_text_overlay_multi_{}.ass",
            uuid::Uuid::new_v4()
        ));
        let style_one = AssStyle {
            name: "TextOverlay1".to_string(),
            ..AssStyle::default()
        };
        let style_two = AssStyle {
            name: "TextOverlay2".to_string(),
            ..AssStyle::default()
        };
        write_text_overlays_ass(
            &path,
            &[
                ("One", &style_one, 0.25, 0.25),
                ("Two", &style_two, 0.75, 0.75),
            ],
            5_000,
        )
        .expect("multi-layer text overlay ASS should be written");
        let content = std::fs::read_to_string(&path).expect("ASS should be readable");
        let _ = std::fs::remove_file(path);

        assert!(content.contains("Style: TextOverlay1"));
        assert!(content.contains("Style: TextOverlay2"));
        assert!(content.contains("Dialogue: 0,0:00:00.00,0:00:05.00,TextOverlay1"));
        assert!(content.contains("Dialogue: 1,0:00:00.00,0:00:05.00,TextOverlay2"));
    }

    #[test]
    fn text_overlay_ass_preserves_text_order_and_special_characters() {
        let path = std::env::temp_dir().join(format!(
            "aspectshift_text_overlay_order_{}.ass",
            uuid::Uuid::new_v4()
        ));
        let style = AssStyle {
            name: "TextOverlay1".to_string(),
            ..AssStyle::default()
        };
        let cases = [
            ("this is text one", "this is text one"),
            ("Add Text", "Add Text"),
            ("It's 10:30, Vish!", "It's 10:30, Vish!"),
            ("100% ready", "100% ready"),
            ("hello: world", "hello: world"),
            ("नमस्ते दुनिया", "नमस्ते दुनिया"),
            ("🔥 BEST BUILD EVER 🔥", "🔥 BEST BUILD EVER 🔥"),
        ];
        let layers: Vec<(&str, &AssStyle, f32, f32)> = cases
            .iter()
            .map(|(text, _)| (*text, &style, 0.5, 0.5))
            .collect();

        write_text_overlays_ass(&path, &layers, 5_000).expect("text overlay ASS should be written");
        let content = std::fs::read_to_string(&path).expect("ASS should be readable");
        let _ = std::fs::remove_file(path);

        for (_, expected) in cases {
            assert!(content.contains(expected), "missing {expected}");
        }
        assert!(!content.contains("enotxet"));
        assert!(!content.contains("this is text oneAdd Text"));
    }

    #[test]
    fn subtitle_ass_manual_position_uses_pos_without_text_decoration() {
        let path = std::env::temp_dir().join(format!(
            "aspectshift_subtitle_position_{}.ass",
            uuid::Uuid::new_v4()
        ));
        let style = AssStyle {
            font_name: "Fira Sans".to_string(),
            bold: true,
            italic: true,
            underline: false,
            strikethrough: false,
            alignment: 5,
            play_res_x: 1920,
            play_res_y: 1080,
            position: Some((0.25, 0.75)),
            ..AssStyle::default()
        };
        let segments = vec![SubtitleSegment {
            start_ms: 0,
            end_ms: 1_000,
            text: "Hello".to_string(),
            words: Vec::new(),
        }];

        write_ass(&path, &segments, &style).expect("subtitle ASS should be written");
        let content = std::fs::read_to_string(&path).expect("ASS should be readable");
        let _ = std::fs::remove_file(path);

        assert!(content.contains("Fira Sans"));
        assert!(content.contains("-1,-1,0,0,100,100"));
        assert!(content.contains("\\an5\\pos(480,810)"));
    }
}
