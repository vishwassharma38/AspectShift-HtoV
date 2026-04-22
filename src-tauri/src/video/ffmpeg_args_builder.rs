use crate::os_utils::OsUtils;
use crate::subtitles::positioning::get_subtitle_style;
use crate::video::preset_adapter::FfmpegPreset;

fn with_subtitle_filter(filter_graph: &str, subtitle_path: &str, subtitle_style: &str) -> String {
    let escaped_path = OsUtils::escape_filter_path(subtitle_path);
    let escaped_style = subtitle_style.replace('\'', "\\'");
    let subtitle_filter = format!("subtitles='{escaped_path}':force_style='{escaped_style}'");

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
    preset: &FfmpegPreset,
    subtitle_path: Option<&str>,
) -> Vec<String> {
    let final_filter_graph = if preset.burn_subtitles {
        if let Some(path) = subtitle_path {
            let style = get_subtitle_style(preset.ratio.get_ratio());
            with_subtitle_filter(filter_graph, path, &style)
        } else {
            filter_graph.to_string()
        }
    } else {
        filter_graph.to_string()
    };

    let mut args = vec!["-i".to_string(), input.to_string()];

    if let Some(logo) = &preset.logo {
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
        if !preset.remove_audio {
            args.push("-map".to_string());
            args.push("0:a?".to_string());
        }
    }

    if preset.remove_audio {
        args.push("-an".to_string());
    } else {
        let bitrate = preset.audio_bitrate.as_deref().unwrap_or("128k");
        args.extend_from_slice(&[
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            bitrate.to_string(),
        ]);
    }

    let codec = get_video_codec(output);
    args.push("-c:v".to_string());
    args.push(codec.to_string());

    // Use custom quality settings if explicitly enabled and supported by codec
    if preset.custom_encoding_enabled {
        if let Some(crf) = preset.crf {
            if supports_crf(codec) {
                let clamped_crf = crf.clamp(0, 51);
                args.push("-crf".to_string());
                args.push(clamped_crf.to_string());
            }
        }

        if let Some(speed_preset) = &preset.preset {
            if supports_preset(codec) {
                args.push("-preset".to_string());
                args.push(speed_preset.clone());
            }
        }
    } else {
        // Fallback to legacy quality preset system
        let quality_args = preset.quality.get_ffmpeg_args();
        for arg in quality_args {
            args.push(arg.to_string());
        }
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
