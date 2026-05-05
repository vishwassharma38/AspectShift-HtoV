use crate::subtitles::srt_writer::write_srt_for_input;
use crate::subtitles::whisper_runner::transcribe_to_segments;
use crate::video::ffmpeg::run_ffmpeg;
use crate::video::ffmpeg_args_builder::build_ffmpeg_args;
use crate::video::filter_builder::{build_filter_graph, validate_preset_consistency};
use crate::video::lock::ProcessingLock;
use crate::video::preset_adapter::create_render_plan;
use crate::video::probe::{check_file_ready, detect_orientation};
use crate::video::types::{
    ConversionResult, OutputJob, OutputTarget, VideoError,
};
use crate::video::validation::validate_output_job;
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
    target: &OutputTarget,
    use_subfolders: bool,
) -> String {
    crate::video::paths::resolve_output_path(Path::new(output_dir), Path::new(input), target, use_subfolders)
        .to_string_lossy()
        .to_string()
}

// LEGACY COMPATIBILITY ONLY:
// This reproduces pre-refactor label derivation.
// It must NEVER be used for new output paths.
// Exists solely for migration fallback checks.
// TODO: Remove once all users have migrated to the new normalization system.
// This function reproduces legacy label derivation and must not diverge from
// normalize_targets. Any changes to target classification logic must be
// reflected here until this is deleted.
fn derive_legacy_label(job: &OutputJob) -> String {
    if let Some(ref name) = job.preset_name {
        name.clone()
    } else {
        job.ratio.get_tag().to_string()
    }
}

// LEGACY: This function exists ONLY for backward compatibility checks.
// DO NOT use for new output generation.
// DO NOT expose outside this module.
fn legacy_sanitize_label(label: &str) -> String {
    label
        .replace(':', "x")
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "")
        .to_lowercase()
}

pub fn check_already_processed(
    input: &str,
    output_dir: &str,
    target: &OutputTarget,
    use_subfolders: bool,
    resolved_output_path: Option<&str>,
) -> bool {
    if !target.job.effects.skip_existing_enabled() {
        return false;
    }

    if let Some(path) = resolved_output_path {
        if Path::new(path).exists() {
            return true;
        }
    }

    // Check new sanitized path
    let output_path = get_deterministic_output_path(input, output_dir, target, use_subfolders);
    if Path::new(&output_path).exists() {
        return true;
    }

    // Backward compatibility: Check legacy sanitized path
    if use_subfolders {
        let mut legacy_target = target.clone();
        let raw_label = derive_legacy_label(&target.job);
        legacy_target.label = legacy_sanitize_label(&raw_label);
        let legacy_path = get_deterministic_output_path(input, output_dir, &legacy_target, true);
        if Path::new(&legacy_path).exists() {
            return true;
        }
    }

    false
}

pub async fn render_single(
    app: &AppHandle,
    job_id: String,
    input: String,
    output_dir: String,
    job: OutputJob,
    subtitle_path: Option<PathBuf>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    resolved_output_path: Option<String>,
) -> Result<ConversionResult, VideoError> {
    validate_output_job(&job)?;

    // Normalize target ONCE at entry point
    let target = crate::video::targets::normalize_targets(&[job])
        .map_err(|e| VideoError::InvalidInput(e))?
        .pop()
        .unwrap();

    // 1. Determine final output path
    let output_path = if let Some(path) = resolved_output_path {
        path
    } else {
        get_deterministic_output_path(&input, &output_dir, &target, false)
    };

    let output_path_buf = PathBuf::from(&output_path);
    let final_output_dir = output_path_buf
        .parent()
        .ok_or_else(|| VideoError::InvalidInput("Invalid output path".to_string()))?;

    // 2. File Readiness Check
    let readiness = check_file_ready(app, &input).await?;
    let duration = readiness.estimated_duration_secs;

    // 3. Already Processed Check (including migration-aware logic)
    if check_already_processed(&input, &output_dir, &target, false, Some(&output_path)) {
        return Ok(ConversionResult {
            output_path,
            ratio: target.job.ratio,
            skipped: true,
        });
    }

    // 4. Acquire Lock
    let _lock = ProcessingLock::acquire(app, &input)?;

    // 5. Subtitle Preparation
    let should_generate_subtitles =
        target.job.effects.generate_subtitles_enabled() || target.job.effects.burn_subtitles_enabled();
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

    // 6. Ensure output directory exists
    if !final_output_dir.exists() {
        std::fs::create_dir_all(final_output_dir)?;
    }

    // 7. Orientation Detection
    let orientation = detect_orientation(app, &input).await?;

    let stem = Path::new(&input)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");

    let file_label = stem.to_string();
    let ratio_label = target.job.ratio.get_tag().to_string();

    // 9. Resolved render plan
    let plan = create_render_plan(target.job.clone(), &input)?;

    // 10. Consistency Validation
    validate_preset_consistency(&plan).map_err(VideoError::InvalidInput)?;

    // 11. Passthrough Check
    let current_ratio = orientation.display_width as f32 / orientation.display_height as f32;
    let target_ratio = target.job.ratio.get_ratio();
    let ratio_diff = (current_ratio - target_ratio).abs() / target_ratio;

    let has_transform = if let Some(t) = &target.job.effects.transform {
        t.rotate != 0 || t.flip_h || t.flip_v
    } else {
        false
    };

    if orientation.is_vertical
        && ratio_diff < 0.02
        && !target.job.effects.blur_enabled()
        && !target.job.effects.remove_audio_enabled()
        && !target.job.effects.burn_subtitles_enabled()
        && plan.logo.is_none()
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
        )
        .await?;
        return Ok(ConversionResult {
            output_path,
            ratio: target.job.ratio,
            skipped: false,
        });
    }

    // 12. Filter Construction
    let filter = build_filter_graph(&plan, &orientation);

    // 13. FFmpeg Command Building
    let subtitle_str = subtitle_path.as_ref().and_then(|p| p.to_str());
    let args_vec = build_ffmpeg_args(&input, &output_path, &filter, &plan, subtitle_str);
    let args: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    run_ffmpeg(
        app,
        &args,
        &job_id,
        &file_label,
        &ratio_label,
        duration,
        cancel_token,
    )
    .await?;

    Ok(ConversionResult {
        output_path,
        ratio: target.job.ratio,
        skipped: false,
    })
}
