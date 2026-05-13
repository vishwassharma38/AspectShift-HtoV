use crate::subtitles::srt_writer::write_srt_for_input;
use crate::subtitles::whisper_runner::transcribe_to_segments;
use crate::video::ffmpeg::run_ffmpeg;
use crate::video::ffmpeg_args_builder::build_ffmpeg_args;
use crate::video::filter_builder::{build_filter_graph, validate_preset_consistency};
use crate::video::lock::ProcessingLock;
use crate::video::preset_adapter::create_render_plan_resolved;
use crate::video::probe::{check_file_ready, detect_orientation};
use crate::video::types::{ConversionResult, VideoError};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tracing::info;

async fn is_valid_completed_output(app: &AppHandle, output_path: &Path) -> bool {
    if !output_path.exists() {
        return false;
    }

    let meta = match std::fs::metadata(output_path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if meta.len() == 0 {
        return false;
    }

    match check_file_ready(app, &output_path.to_string_lossy()).await {
        Ok(readiness) => readiness.estimated_duration_secs > 0.0,
        Err(_) => false,
    }
}

pub async fn resolve_existing_output_for_skip(
    app: &AppHandle,
    output_path: &Path,
    alt_output_path: Option<&Path>,
) -> Option<PathBuf> {
    if is_valid_completed_output(app, output_path).await {
        return Some(output_path.to_path_buf());
    }

    if let Some(alt_path) = alt_output_path {
        if is_valid_completed_output(app, alt_path).await {
            return Some(alt_path.to_path_buf());
        }
    }

    None
}

async fn finalize_temp_output(
    app: &AppHandle,
    temp_output_path: &Path,
    final_output_path: &Path,
) -> Result<(), VideoError> {
    if !is_valid_completed_output(app, temp_output_path).await {
        let _ = std::fs::remove_file(temp_output_path);
        return Err(VideoError::ProcessingFailed {
            stderr: "Temporary output failed validation".to_string(),
        });
    }

    if final_output_path.exists() {
        let _ = std::fs::remove_file(final_output_path);
    }

    std::fs::rename(temp_output_path, final_output_path)?;
    Ok(())
}

pub async fn prepare_subtitles(
    app: &AppHandle,
    input: &str,
    output_dir: &str,
    source_duration_secs: f64,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    on_progress: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<PathBuf, VideoError> {
    let input_path = Path::new(input);
    let output_path = Path::new(output_dir);
    let segments = transcribe_to_segments(
        app,
        input_path,
        source_duration_secs,
        cancel_token,
        on_progress,
    )
    .await?;
    let srt_path = write_srt_for_input(input_path, output_path, &segments)?;
    info!(
        "Generated subtitle file for {} at {}",
        input_path.display(),
        srt_path.display()
    );
    Ok(srt_path)
}

pub async fn render_single(
    app: &AppHandle,
    job: crate::video::types::ResolvedJob,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    on_progress: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<ConversionResult, VideoError> {
    let input = &job.input_path;
    let output_path = &job.output_path;
    let output_path_buf = PathBuf::from(output_path);
    let temp_output_path = crate::video::paths::resolve_temp_output_path(&output_path_buf);
    let temp_output_path_str = temp_output_path.to_string_lossy().to_string();
    let session_id = &job.session_id;
    let job_id = &job.id;

    // 1. File Readiness Check
    let readiness = check_file_ready(app, input).await?;
    let duration = readiness.estimated_duration_secs;

    // 2. Skip logic should be handled by caller, but we check existence for safety
    if job.effects.skip_existing_enabled() {
        let alt_path_buf = job.alt_output_path.as_ref().map(PathBuf::from);
        if let Some(existing_path) = resolve_existing_output_for_skip(
            app,
            &output_path_buf,
            alt_path_buf.as_deref(),
        )
        .await
        {
            return Ok(ConversionResult {
                output_path: existing_path.to_string_lossy().to_string(),
                ratio: job.ratio.clone(),
                skipped: true,
            });
        }

        if output_path_buf.exists() {
            let _ = std::fs::remove_file(&output_path_buf);
        }
    }

    // 3. Acquire Lock
    let _lock = ProcessingLock::acquire(app, input)?;

    // 4. Ensure output directory exists
    if let Some(parent) = output_path_buf.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    if temp_output_path.exists() {
        let _ = std::fs::remove_file(&temp_output_path);
    }

    // 5. Orientation Detection
    let orientation = detect_orientation(app, input).await?;

    let stem = Path::new(input)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");

    let file_label = stem.to_string();
    let ratio_label = job.ratio.get_tag().to_string();

    // 6. Resolved render plan
    // We need to pass the resolved job data to create_render_plan
    // I'll update RenderPlan to take the necessary fields.
    let plan = create_render_plan_resolved(&job)?;

    // 7. Consistency Validation
    validate_preset_consistency(&plan).map_err(VideoError::InvalidInput)?;

    // 8. Passthrough Check
    let current_ratio = orientation.display_width as f32 / orientation.display_height as f32;
    let target_ratio = job.ratio.get_ratio();
    let ratio_diff = (current_ratio - target_ratio).abs() / target_ratio;

    let has_transform = if let Some(t) = &job.effects.transform {
        t.rotate != 0 || t.flip_h || t.flip_v
    } else {
        false
    };

    if orientation.is_vertical
        && ratio_diff < 0.02
        && !job.effects.blur_enabled()
        && !job.effects.remove_audio_enabled()
        && !job.effects.burn_subtitles_enabled()
        && plan.logo.is_none()
        && !has_transform
    {
        let args = ["-i", input, "-c", "copy", "-y", &temp_output_path_str];
        run_ffmpeg(
            app,
            &args,
            session_id,
            job_id,
            &file_label,
            &ratio_label,
            duration,
            cancel_token,
            on_progress,
        )
        .await?;
        finalize_temp_output(app, &temp_output_path, &output_path_buf).await?;
        return Ok(ConversionResult {
            output_path: output_path.clone(),
            ratio: job.ratio.clone(),
            skipped: false,
        });
    }

    // 9. Filter Construction
    let filter = build_filter_graph(&plan, &orientation);

    // 10. FFmpeg Command Building
    let subtitle_str = job.subtitle_path.as_ref().and_then(|p| p.to_str());
    let args_vec = build_ffmpeg_args(input, &temp_output_path_str, &filter, &plan, subtitle_str);
    let args: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    let ffmpeg_res = run_ffmpeg(
        app,
        &args,
        session_id,
        job_id,
        &file_label,
        &ratio_label,
        duration,
        cancel_token.clone(),
        on_progress,
    )
    .await;

    // 11. Cleanup on Failure/Cancellation
    let is_cancelled = cancel_token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false);
    if ffmpeg_res.is_err() || is_cancelled {
        if temp_output_path.exists() {
            let _ = std::fs::remove_file(&temp_output_path);
            info!("Cleaned up temporary output file: {}", temp_output_path.display());
        }
    }

    ffmpeg_res?;
    finalize_temp_output(app, &temp_output_path, &output_path_buf).await?;

    Ok(ConversionResult {
        output_path: output_path.clone(),
        ratio: job.ratio,
        skipped: false,
    })
}
