use crate::os_utils::OsUtils;
use crate::video::preset_adapter::RenderPlan;

fn with_ass_filter(filter_graph: &str, ass_path: &str, fonts_dir: Option<&str>) -> String {
    let escaped_path = OsUtils::escape_filter_path(ass_path);
    let fonts = fonts_dir
        .map(OsUtils::escape_filter_path)
        .map(|path| format!(":fontsdir='{path}'"))
        .unwrap_or_default();
    let subtitle_filter = format!("ass='{escaped_path}'{fonts}");

    if uses_complex_graph(filter_graph) {
        format!("{filter_graph};[v]{subtitle_filter}[v]")
    } else {
        format!("{filter_graph},{subtitle_filter}")
    }
}

fn uses_complex_graph(filter_graph: &str) -> bool {
    let has_named_labels = filter_graph.contains('[') && filter_graph.contains(']');
    let has_multiple_stages = filter_graph.contains(';');
    let has_explicit_input_specifier = filter_graph.contains("[0:") || filter_graph.contains("[1:");

    has_named_labels || has_multiple_stages || has_explicit_input_specifier
}

fn get_video_codec(output: &str) -> &'static str {
    if output.to_lowercase().ends_with(".webm") {
        "libvpx-vp9"
    } else {
        "libx264"
    }
}

fn get_audio_codec(output: &str) -> &'static str {
    if output.to_lowercase().ends_with(".webm") {
        "libopus"
    } else {
        "aac"
    }
}

fn supports_crf(codec: &str) -> bool {
    matches!(codec, "libx264" | "libx265" | "libvpx-vp9")
}

fn supports_preset(codec: &str) -> bool {
    matches!(codec, "libx264" | "libx265")
}

pub fn build_ffmpeg_args(
    input: &str,
    output: &str,
    filter_graph: &str,
    plan: &RenderPlan,
    text_overlay_path: Option<&str>,
    text_fonts_dir: Option<&str>,
    subtitle_path: Option<&str>,
    subtitle_fonts_dir: Option<&str>,
) -> Vec<String> {
    let filter_with_text = if let Some(path) = text_overlay_path {
        with_ass_filter(filter_graph, path, text_fonts_dir)
    } else {
        filter_graph.to_string()
    };
    let final_filter_graph = if plan.effects.burn_subtitles_enabled() {
        if let Some(path) = subtitle_path {
            with_ass_filter(&filter_with_text, path, subtitle_fonts_dir)
        } else {
            filter_with_text
        }
    } else {
        filter_with_text
    };

    let mut args = vec!["-i".to_string(), input.to_string()];

    if let Some(logo) = &plan.logo {
        args.push("-i".to_string());
        args.push(logo.path.clone());
    }

    let use_filter_complex = uses_complex_graph(&final_filter_graph);

    if use_filter_complex {
        args.push("-filter_complex".to_string());
    } else {
        args.push("-vf".to_string());
    }

    args.push(final_filter_graph);

    if use_filter_complex {
        args.push("-map".to_string());
        args.push("[v]".to_string());

        // Map audio if present
        if !plan.effects.remove_audio_enabled() {
            args.push("-map".to_string());
            args.push("0:a?".to_string());
        }
    }

    if plan.effects.remove_audio_enabled() {
        args.push("-an".to_string());
    } else {
        let audio_codec = get_audio_codec(output);
        args.extend_from_slice(&[
            "-c:a".to_string(),
            audio_codec.to_string(),
            "-b:a".to_string(),
            plan.encoding.audio_bitrate.clone(),
        ]);
    }

    let codec = get_video_codec(output);
    args.push("-c:v".to_string());
    args.push(codec.to_string());

    if supports_crf(codec) {
        args.push("-crf".to_string());
        args.push(plan.encoding.crf.to_string());
    }

    if supports_preset(codec) {
        args.push("-preset".to_string());
        args.push(plan.encoding.speed_preset.clone());
    }

    // Web optimization: fast start for MP4
    if output.to_lowercase().ends_with(".mp4") {
        args.push("-movflags".to_string());
        args.push("+faststart".to_string());
    }

    // Force compatibility format
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());

    args.extend_from_slice(&["-y".to_string(), output.to_string()]);

    args
}

#[cfg(test)]
mod tests {
    use super::with_ass_filter;

    #[test]
    fn subtitles_are_appended_after_a_labeled_text_stage() {
        let graph = "[0:v]null[v];[v]drawtext=text=hello[v]";
        let combined = with_ass_filter(graph, "subtitles.ass", None);
        assert_eq!(
            combined,
            "[0:v]null[v];[v]drawtext=text=hello[v];[v]ass='subtitles.ass'[v]"
        );
    }

    #[test]
    fn text_overlay_is_composited_before_burned_subtitles() {
        let graph = "[0:v]null[v]";
        let with_text = with_ass_filter(graph, "text.ass", Some("fonts"));
        let combined = with_ass_filter(&with_text, "captions.ass", None);

        assert_eq!(
            combined,
            "[0:v]null[v];[v]ass='text.ass':fontsdir='fonts'[v];[v]ass='captions.ass'[v]"
        );
    }
}
