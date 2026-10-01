use crate::subtitles::srt_writer::write_srt_for_input;
use crate::subtitles::whisper_runner::transcribe_to_segments;
use crate::video::encoding::is_passthrough_allowed;
use crate::video::ffmpeg::run_ffmpeg;
use crate::video::ffmpeg_args_builder::build_ffmpeg_args;
use crate::video::filter_builder::{build_filter_graph, validate_preset_consistency};
use crate::video::lock::ProcessingLock;
use crate::video::preset_adapter::create_render_plan_resolved;
use crate::video::probe::{check_file_ready, detect_orientation};
use crate::video::types::{ConversionResult, VideoError};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tracing::info;

struct PreparedTextOverlay {
    ass_path: PathBuf,
    fonts_dir: PathBuf,
}

#[derive(Clone)]
pub struct PreparedSubtitle {
    pub path: PathBuf,
    pub fonts_dir: Option<PathBuf>,
}

fn ass_colour(hex: &str, opacity: f32) -> String {
    let rgb = hex.strip_prefix('#').unwrap_or(hex);
    let (red, green, blue) = if rgb.len() == 6 {
        (&rgb[0..2], &rgb[2..4], &rgb[4..6])
    } else {
        ("FF", "FF", "FF")
    };
    let alpha = ((1.0 - opacity.clamp(0.0, 1.0)) * 255.0).round() as u8;
    format!("&H{alpha:02X}{blue}{green}{red}")
}

fn prepare_text_overlay(
    app: &AppHandle,
    plan: &crate::video::preset_adapter::RenderPlan,
    orientation: &crate::video::types::OrientationInfo,
    duration_secs: f64,
    stem: &str,
) -> Result<Option<PreparedTextOverlay>, VideoError> {
    if !plan.effects.text_overlay_enabled() {
        return Ok(None);
    }

    let layout = crate::video::render_layout::calculate_render_layout(plan, orientation, None);
    let temp_dir = crate::os_utils::OsUtils::get_temp_dir(app);
    let path = temp_dir.join(format!("{stem}_text_{}.ass", uuid::Uuid::new_v4()));
    let fonts_dir = temp_dir.join(format!("text_fonts_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&fonts_dir)?;
    let mut copied_fonts = HashSet::new();
    let mut prepared_layers = Vec::new();
    for (index, settings) in plan
        .effects
        .text_overlay
        .layers
        .iter()
        .filter(|layer| layer.enabled && !layer.text.trim().is_empty())
        .enumerate()
    {
        for font_path in
            crate::video::text_fonts::resolve_text_overlay_font_files(app, &settings.font_style)?
        {
            if copied_fonts.insert(font_path.clone()) {
                let file_name = font_path.file_name().ok_or_else(|| {
                    VideoError::InvalidInput(format!(
                        "Bundled text font '{}' has no file name",
                        crate::video::text_fonts::family(&settings.font_style).ass_name
                    ))
                })?;
                std::fs::copy(&font_path, fonts_dir.join(file_name))?;
            }
        }
        let text_font = crate::video::text_fonts::family(&settings.font_style);
        let nominal_size = settings.font_size.max(1) as f32;
        let style = crate::subtitles::ass_writer::AssStyle {
            name: format!("TextOverlay{}", index + 1),
            font_name: text_font.ass_name.to_string(),
            // Preview (CSS `font-size`) sets the em square; libass scales so
            // the Windows line cell equals `Fontsize`. Upscale by the
            // per-font win/UPM ratio so the export glyph size matches the
            // preview. Canonical `fontSize` storage is unchanged.
            font_size: crate::video::text_fonts::ass_font_size_for_style(
                &settings.font_style,
                nominal_size,
            ),
            primary_colour: ass_colour(&settings.color, settings.opacity),
            outline_colour: ass_colour(&settings.outline_color, settings.opacity),
            back_colour: "&HFF000000".to_string(),
            bold: settings.bold,
            italic: settings.italic,
            underline: settings.underline,
            strikethrough: settings.strikethrough,
            // Preview uses `-webkit-text-stroke` with `paint-order: stroke
            // fill`: the stroke is centered on the glyph edge and the fill
            // covers the inner half, so only ~w/2 is visible outside. ASS
            // `Outline` draws the full width outward, so halve it to match
            // the preview's visible thickness.
            outline: if settings.outline_enabled {
                settings.outline_width.max(0) as f32 / 2.0
            } else {
                0.0
            },
            shadow: 0.0,
            alignment: 5,
            margin_v: 0,
            play_res_y: layout.target_height,
            play_res_x: layout.target_width,
            position: None,
            spacing: crate::video::text_fonts::ass_letter_spacing_for_style(
                &settings.font_style,
                nominal_size,
            ),
            // Preview CSS `rotate()` is clockwise-positive; ASS `Angle`
            // (`\frz` convention) is counter-clockwise-positive, so negate
            // to match the preview. Center anchor is the same on both sides
            // (`translate(-50%,-50%)` + `center center` vs `\an5` + `\pos`),
            // so no origin/position change is needed. Image overlays keep a
            // straight pass-through because FFmpeg `rotate` is
            // clockwise-positive like CSS.
            angle: crate::video::text_fonts::ass_angle_for_text_rotation(
                settings.rotation,
            ),
        };
        prepared_layers.push((settings.text.clone(), style, settings.x, settings.y));
    }
    let duration_ms = (duration_secs.max(0.01) * 1000.0).ceil() as u64;
    let layer_entries: Vec<(&str, &crate::subtitles::ass_writer::AssStyle, f32, f32)> =
        prepared_layers
            .iter()
            .map(|(text, style, x, y)| (text.as_str(), style, *x, *y))
            .collect();
    crate::subtitles::ass_writer::write_text_overlays_ass(&path, &layer_entries, duration_ms)?;
    Ok(Some(PreparedTextOverlay {
        ass_path: path,
        fonts_dir,
    }))
}

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

    match std::fs::rename(temp_output_path, final_output_path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Do not strand a full-length temp artifact when the rename fails:
            // it would masquerade as a completed output and quietly consume
            // disk space that the Stage 3.3 gate accounts for. Best-effort
            // removal keeps the failure surface clean.
            let _ = std::fs::remove_file(temp_output_path);
            Err(VideoError::IoError(e))
        }
    }
}

pub async fn prepare_subtitles(
    app: &AppHandle,
    input: &str,
    output_dir: &str,
    source_duration_secs: f64,
    burn_subtitles: bool,
    export_subtitles: bool,
    target_width: u32,
    target_height: u32,
    foreground_frame_height: u32,
    blur_enabled: bool,
    subtitle_overlay: &crate::video::types::SubtitleOverlaySettings,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    on_progress: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<PreparedSubtitle, VideoError> {
    let input_path = Path::new(input);
    let segments = transcribe_to_segments(
        app,
        input_path,
        source_duration_secs,
        cancel_token,
        on_progress,
    )
    .await?;

    let mut result = PreparedSubtitle {
        path: PathBuf::new(),
        fonts_dir: None,
    };

    // 1. Export SRT if requested (in output directory)
    if export_subtitles {
        let output_path = Path::new(output_dir);
        let srt_path = write_srt_for_input(input_path, output_path, &segments)?;
        info!(
            "Generated SRT export for {} at {}",
            input_path.display(),
            srt_path.display()
        );
        result.path = srt_path;
    }

    // 2. Generate ASS for burn-in (always in temp directory)
    if burn_subtitles {
        let temp_dir = crate::os_utils::OsUtils::get_temp_dir(app);
        let stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("subtitle");
        let ass_path = temp_dir.join(format!("{}_{}.ass", stem, uuid::Uuid::new_v4()));
        let fonts_dir = temp_dir.join(format!("subtitle_fonts_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&fonts_dir)?;
        for font_path in crate::video::text_fonts::resolve_subtitle_overlay_font_files(
            app,
            &subtitle_overlay.font_style,
        )? {
            let file_name = font_path.file_name().ok_or_else(|| {
                VideoError::InvalidInput(format!(
                    "Bundled subtitle font '{}' has no file name",
                    crate::video::text_fonts::family(&subtitle_overlay.font_style).ass_name
                ))
            })?;
            std::fs::copy(&font_path, fonts_dir.join(file_name))?;
        }

        let style = crate::subtitles::positioning::calculate_ass_style(
            target_width,
            target_height,
            foreground_frame_height,
            blur_enabled,
            subtitle_overlay,
        );
        crate::subtitles::ass_writer::write_ass(&ass_path, &segments, &style)?;

        info!(
            "Generated ASS for burn-in for {} at {}",
            input_path.display(),
            ass_path.display()
        );

        // If we're burning in, the ASS path is the one we want to return for the FFmpeg filter
        result.path = ass_path;
        result.fonts_dir = Some(fonts_dir);
    }

    Ok(result)
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
        if let Some(existing_path) =
            resolve_existing_output_for_skip(app, &output_path_buf, alt_path_buf.as_deref()).await
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
    let _lock = ProcessingLock::acquire(app, input, output_path)?;

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

    // Observability (§21): what the user selected (post-resolution) and
    // whether explicit session intent forces re-encoding.
    info!(
        "render_single job={} resolved encoding: crf={} qualityPreset={} speedPreset={} audioBitrate={} force_reencode={}",
        job_id,
        plan.encoding.crf,
        plan.encoding.quality_preset,
        plan.encoding.speed_preset,
        plan.encoding.audio_bitrate,
        job.force_reencode,
    );

    // Applicability diagnostics (Issues #5/#6): the renderer stays correct by
    // omitting inapplicable flags; these logs explain the omission.
    if temp_output_path_str.to_lowercase().ends_with(".webm") {
        info!(
            "render_single job={} speed preset '{}' is not applicable to VP9/WebM; no -preset will be emitted",
            job_id, plan.encoding.speed_preset,
        );
    }
    if plan.effects.remove_audio_enabled() {
        info!(
            "render_single job={} audio bitrate '{}' has no effect while remove-audio is enabled (-an)",
            job_id, plan.encoding.audio_bitrate,
        );
    }

    // 7. Consistency Validation
    validate_preset_consistency(&plan).map_err(VideoError::InvalidInput)?;

    // 8. Passthrough Check (Route B: explicit encoding intent blocks `-c copy`)
    let current_ratio = orientation.display_width as f32 / orientation.display_height as f32;
    let target_ratio = job.ratio.get_ratio();
    let ratio_diff = (current_ratio - target_ratio).abs() / target_ratio;

    let has_transform = if let Some(t) = &job.effects.transform {
        t.rotate != 0 || t.flip_h || t.flip_v
    } else {
        false
    };

    let passthrough_allowed = is_passthrough_allowed(
        orientation.is_vertical,
        ratio_diff,
        job.effects.background_effect_enabled(),
        job.effects.remove_audio_enabled(),
        job.effects.burn_subtitles_enabled(),
        job.effects.text_overlay_enabled(),
        !plan.images.is_empty(),
        has_transform,
        job.force_reencode,
    );
    info!(
        "render_single job={} passthrough_allowed={} (vertical={} ratio_diff={:.4} force_reencode={})",
        job_id,
        passthrough_allowed,
        orientation.is_vertical,
        ratio_diff,
        job.force_reencode,
    );

    if passthrough_allowed {
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
    let text_overlay_path = prepare_text_overlay(app, &plan, &orientation, duration, stem)?;

    // 10. FFmpeg Command Building
    let text_overlay_str = text_overlay_path
        .as_ref()
        .and_then(|prepared| prepared.ass_path.to_str());
    let text_fonts_dir = text_overlay_path
        .as_ref()
        .and_then(|prepared| prepared.fonts_dir.to_str());
    let subtitle_str = job.subtitle_path.as_ref().and_then(|p| p.to_str());
    let subtitle_fonts_dir = job.subtitle_fonts_dir.as_ref().and_then(|p| p.to_str());
    let args_vec = build_ffmpeg_args(
        input,
        &temp_output_path_str,
        &filter,
        &plan,
        text_overlay_str,
        text_fonts_dir,
        subtitle_str,
        subtitle_fonts_dir,
        job.threads_per_job,
    );
    // Observability (§21): exact FFmpeg invocation for this render.
    info!("render_single job={} ffmpeg args: {:?}", job_id, args_vec);
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
    let is_cancelled = cancel_token
        .as_ref()
        .map(|t| t.is_cancelled())
        .unwrap_or(false);
    if ffmpeg_res.is_err() || is_cancelled {
        if temp_output_path.exists() {
            let _ = std::fs::remove_file(&temp_output_path);
            info!(
                "Cleaned up temporary output file: {}",
                temp_output_path.display()
            );
        }
    }
    if let Some(prepared) = &text_overlay_path {
        let _ = std::fs::remove_file(&prepared.ass_path);
        let _ = std::fs::remove_dir_all(&prepared.fonts_dir);
    }

    ffmpeg_res?;
    finalize_temp_output(app, &temp_output_path, &output_path_buf).await?;

    Ok(ConversionResult {
        output_path: output_path.clone(),
        ratio: job.ratio,
        skipped: false,
    })
}
