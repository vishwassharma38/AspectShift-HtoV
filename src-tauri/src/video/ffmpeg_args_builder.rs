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
    threads_per_job: Option<usize>,
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

    // Thread budget hint for software encoders (Stage 0.3).
    // This is a hint to FFmpeg's internal threading, not an exact CPU-thread guarantee.
    if let Some(threads) = threads_per_job {
        if threads > 0 {
            args.push("-threads".to_string());
            args.push(threads.to_string());
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

#[cfg(test)]
mod tests {
    use super::{build_ffmpeg_args, with_ass_filter};
    use crate::video::preset_adapter::RenderPlan;
    use crate::video::types::{
        AspectRatio, EncodingProfile, SubtitleOverlaySettings, TextOverlaySettings,
        VideoEffectsSettings,
    };

    fn test_plan() -> RenderPlan {
        RenderPlan {
            ratio: AspectRatio::Ratio9x16,
            encoding: EncodingProfile::standard(),
            effects: VideoEffectsSettings {
                blur: None,
                white_background: None,
                overlays: None,
                subtitles: None,
                color_filter: None,
                blur_sigma: None,
                remove_audio: None,
                export_subtitles: None,
                burn_subtitles: None,
                skip_existing: None,
                output_format: None,
                logo: None,
                text_overlay: TextOverlaySettings::default(),
                subtitle_overlay: SubtitleOverlaySettings::default(),
                transform: None,
            },
            platform_config: None,
            logo: None,
        }
    }

    fn build_args(output: &str, threads_per_job: Option<usize>) -> Vec<String> {
        let plan = test_plan();
        build_ffmpeg_args(
            "input.mp4",
            output,
            "null",
            &plan,
            None,
            None,
            None,
            None,
            threads_per_job,
        )
    }

    // --- Existing filter-graph tests (unchanged) ---

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

    // --- Test A: No thread budget ---

    #[test]
    fn no_thread_budget_does_not_emit_threads_flag() {
        let args = build_args("output.mp4", None);
        assert!(
            !args.contains(&"-threads".to_string()),
            "args with None must not contain -threads, got: {args:?}"
        );
    }

    // --- Test B: libx264 with thread budget ---

    #[test]
    fn libx264_with_thread_budget_emits_threads() {
        let args = build_args("output.mp4", Some(4));
        let threads_pos = args.iter().position(|a| a == "-threads");
        let pos = threads_pos.expect("-threads must be present for libx264 with budget");
        assert_eq!(args[pos + 1], "4");
    }

    #[test]
    fn libx264_args_intact_with_thread_budget() {
        let args = build_args("output.mp4", Some(4));
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"-crf".to_string()));
        assert!(args.contains(&"-preset".to_string()));
        assert!(args.contains(&"-pix_fmt".to_string()));
        assert!(args.contains(&"yuv420p".to_string()));
        assert!(args.contains(&"-movflags".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
    }

    // --- Test C: libvpx-vp9 with thread budget ---

    #[test]
    fn libvpx_vp9_with_thread_budget_emits_threads() {
        let args = build_args("output.webm", Some(4));
        let threads_pos = args.iter().position(|a| a == "-threads");
        let pos = threads_pos.expect("-threads must be present for libvpx-vp9 with budget");
        assert_eq!(args[pos + 1], "4");
    }

    #[test]
    fn libvpx_vp9_args_intact_with_thread_budget() {
        let args = build_args("output.webm", Some(4));
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libvpx-vp9".to_string()));
        assert!(args.contains(&"-crf".to_string()));
        assert!(args.contains(&"-pix_fmt".to_string()));
        assert!(args.contains(&"yuv420p".to_string()));
        // libvpx-vp9 has no -preset
        assert!(!args.contains(&"-preset".to_string()));
        // libvpx-vp9 output is .webm, not .mp4 → no -movflags
        assert!(!args.contains(&"-movflags".to_string()));
    }

    // --- Test D: Regression — None produces exactly the pre-stage args ---

    #[test]
    fn no_threads_budget_matches_pre_stage_args() {
        let args_none = build_args("output.mp4", None);
        // Pre-stage: no -threads argument was ever emitted
        assert!(!args_none.windows(2).any(|w| w[0] == "-threads"));
        // Verify the core args structure is unchanged
        let args_with_zero = build_args("output.mp4", Some(0));
        // Some(0) should also not emit -threads (guard: threads > 0)
        assert!(
            !args_with_zero.contains(&"-threads".to_string()),
            "Some(0) must not emit -threads"
        );
    }

    #[test]
    fn none_and_some_zero_produce_identical_args() {
        let args_none = build_args("output.mp4", None);
        let args_zero = build_args("output.mp4", Some(0));
        assert_eq!(args_none, args_zero);
    }

    // --- Test E: Different explicit values ---

    #[test]
    fn different_thread_values_are_propagated() {
        let args_2 = build_args("output.mp4", Some(2));
        let args_8 = build_args("output.mp4", Some(8));

        let pos_2 = args_2.iter().position(|a| a == "-threads").unwrap();
        assert_eq!(args_2[pos_2 + 1], "2");

        let pos_8 = args_8.iter().position(|a| a == "-threads").unwrap();
        assert_eq!(args_8[pos_8 + 1], "8");

        // The full args must differ only in the thread value
        assert_ne!(args_2[pos_2 + 1], args_8[pos_8 + 1]);
    }

    #[test]
    fn threads_flag_appears_after_codec_settings() {
        let args = build_args("output.mp4", Some(4));
        let codec_pos = args.iter().position(|a| a == "-c:v").unwrap();
        let threads_pos = args.iter().position(|a| a == "-threads").unwrap();
        assert!(threads_pos > codec_pos, "-threads must appear after -c:v");
    }
}
