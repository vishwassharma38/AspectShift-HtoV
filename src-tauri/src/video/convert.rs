use crate::subtitles::srt_writer::write_srt_for_input;
use crate::subtitles::whisper_runner::transcribe_to_segments;
use crate::video::ffmpeg::run_ffmpeg;
use crate::video::ffmpeg_args_builder::build_ffmpeg_args;
use crate::video::filter_builder::{build_filter_graph, validate_preset_consistency};
use crate::video::lock::ProcessingLock;
use crate::video::preset_adapter::legacy_to_preset;
use crate::video::probe::{check_file_ready, detect_orientation};
use crate::video::types::{
    AspectRatio, ConversionOptions, ConversionResult, PlatformConfig, VideoError, OutputTags,
};
use crate::os_utils::OsUtils;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tracing::info;

pub async fn prepare_subtitles(
    app: &AppHandle,
    input: &str,
    output_dir: &str,
) -> Result<PathBuf, VideoError> {
    let input_path = Path::new(input);
    let output_path = Path::new(output_dir);
    let segments = transcribe_to_segments(app, input_path).await?;
    let srt_path = write_srt_for_input(input_path, output_path, &segments)?;
    info!(
        "Generated subtitle file for {} at {}",
        input_path.display(),
        srt_path.display()
    );
    Ok(srt_path)
}

pub fn get_deterministic_output_path(
    input: &str,
    output_dir: &str,
    ratio: &AspectRatio,
    options: &ConversionOptions,
) -> String {
    let input_path = Path::new(input);
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let ext = options.output_format.get_extension();
    
    let platform_tag = options.preset.as_ref().map(|name| {
        let base_name = name.split('(').next().unwrap_or(name).trim();
        OsUtils::sanitize_path_component(base_name)
    });

    let tags = OutputTags {
        ratio: ratio.get_tag().replace(':', "x"),
        platform: platform_tag,
        blur: options.blur_background,
        logo: options.logo.as_ref().map(|l| l.enabled).unwrap_or(false),
        subtitles: options.burn_subtitles || options.generate_subtitles,
        no_audio: options.remove_audio,
    };

    let filename = tags.get_output_filename(stem, ext);
    Path::new(output_dir).join(filename).to_string_lossy().to_string()
}

pub fn check_already_processed(
    input: &str,
    output_dir: &str,
    ratio: &AspectRatio,
    options: &ConversionOptions,
    resolved_output_path: Option<&str>,
) -> bool {
    if !options.skip_existing {
        return false;
    }

    let output_path = if let Some(path) = resolved_output_path {
        path.to_string()
    } else {
        get_deterministic_output_path(input, output_dir, ratio, options)
    };

    Path::new(&output_path).exists()
}

pub async fn convert_to_ratio(
    app: &AppHandle,
    job_id: String,
    input: String,
    output_dir: String,
    ratio: AspectRatio,
    options: ConversionOptions,
    platform_config: Option<PlatformConfig>,
    subtitle_path: Option<PathBuf>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    resolved_output_path: Option<String>,
) -> Result<ConversionResult, VideoError> {
    // 1. Determine final output path
    let output_path = if let Some(path) = resolved_output_path {
        path
    } else {
        get_deterministic_output_path(&input, &output_dir, &ratio, &options)
    };

    let output_path_buf = PathBuf::from(&output_path);
    let final_output_dir = output_path_buf.parent().ok_or_else(|| VideoError::InvalidInput("Invalid output path".to_string()))?;

    // 2. File Readiness Check
    let readiness = check_file_ready(app, &input).await?;
    let duration = readiness.estimated_duration_secs;

    let should_generate_subtitles = options.generate_subtitles || options.burn_subtitles;
    let subtitle_path = if should_generate_subtitles {
        if let Some(existing) = subtitle_path {
            Some(existing)
        } else {
            // Ensure output dir for subtitles exists
            if !final_output_dir.exists() {
                std::fs::create_dir_all(final_output_dir)?;
            }
            Some(prepare_subtitles(app, &input, &final_output_dir.to_string_lossy()).await?)
        }
    } else {
        None
    };

    // 3. Already Processed Check
    if options.skip_existing && Path::new(&output_path).exists() {
        return Ok(ConversionResult {
            output_path,
            ratio,
            skipped: true,
        });
    }

    // 4. Acquire Lock
    let _lock = ProcessingLock::acquire(&input, &final_output_dir.to_string_lossy())?;

    // 5. Ensure output directory exists
    if !final_output_dir.exists() {
        std::fs::create_dir_all(final_output_dir)?;
    }

    // 6. Orientation Detection
    let orientation = detect_orientation(app, &input).await?;

    let stem = Path::new(&input)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");

    let file_label = stem.to_string();
    let ratio_label = ratio.get_tag().to_string();

    // 7. Logo Detection
    let logo_path = if let Some(logo_opts) = &options.logo {
        if logo_opts.enabled {
            if let Some(path) = &logo_opts.path {
                if Path::new(path).exists() {
                    Some(path.clone())
                } else {
                    None
                }
            } else {
                let input_path = Path::new(&input);
                let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
                let logo_file = parent.join("logo.png");
                if logo_file.exists() {
                    Some(logo_file.to_string_lossy().to_string())
                } else {
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // 8. Bridge to Preset System
    let preset = legacy_to_preset(ratio.clone(), options.clone(), logo_path, platform_config);

    // 9. Consistency Validation
    validate_preset_consistency(&preset).map_err(|e| VideoError::InvalidInput(e))?;

    // 10. Passthrough Check
    let current_ratio = orientation.display_width as f32 / orientation.display_height as f32;
    let target_ratio = ratio.get_ratio();
    let ratio_diff = (current_ratio - target_ratio).abs() / target_ratio;

    let has_transform = if let Some(t) = &options.transform {
        t.rotate != 0 || t.flip_h || t.flip_v
    } else {
        false
    };

    if orientation.is_vertical
        && ratio_diff < 0.02
        && !options.blur_background
        && !options.remove_audio
        && !options.burn_subtitles
        && preset.logo.is_none()
        && !has_transform
    {
        let args = ["-i", &input, "-c", "copy", "-y", &output_path];
        run_ffmpeg(
            app,
            &args,
            &job_id,
            &file_label,
            &ratio_label,
            duration,
            cancel_token,
        ).await?;
        return Ok(ConversionResult {
            output_path,
            ratio,
            skipped: false,
        });
    }

    // 11. Filter Construction
    let filter = build_filter_graph(&preset, &orientation);

    // 12. FFmpeg Command Building
    let subtitle_str = subtitle_path.as_ref().and_then(|p| p.to_str());
    let args_vec = build_ffmpeg_args(&input, &output_path, &filter, &preset, subtitle_str);
    let args: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    run_ffmpeg(
        app,
        &args,
        &job_id,
        &file_label,
        &ratio_label,
        duration,
        cancel_token,
    ).await?;

    Ok(ConversionResult {
        output_path,
        ratio,
        skipped: false,
    })
}
