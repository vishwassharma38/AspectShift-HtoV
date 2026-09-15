use crate::os_utils::OsUtils;
use crate::video::convert::{prepare_subtitles, render_single, PreparedSubtitle};
use crate::video::queue::{clear_terminal_progress_fields, BatchManager, BatchState};
use crate::video::targets::normalize_targets;
use crate::video::types::{
    BatchJob, BatchJobSettings, BatchProgress, BatchStatus, FileProgress, JobStatus,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use tracing::warn;
use uuid::Uuid;

#[derive(Clone, Copy)]
struct LifecycleWeights {
    prepare: f32,
    subtitle: f32,
    render_prepare: f32,
    rendering: f32,
    finalize: f32,
}

impl LifecycleWeights {
    fn for_job(has_subtitles: bool) -> Self {
        if has_subtitles {
            Self {
                prepare: 8.0,
                subtitle: 28.0,
                render_prepare: 14.0,
                rendering: 45.0,
                finalize: 5.0,
            }
        } else {
            Self {
                prepare: 14.0,
                subtitle: 0.0,
                render_prepare: 16.0,
                rendering: 65.0,
                finalize: 5.0,
            }
        }
    }
}

fn set_stage(
    state: &mut BatchState,
    job_id: &str,
    stage_id: &str,
    stage_message: String,
    lifecycle_progress: f32,
) {
    state.current_stage_id = Some(stage_id.to_string());
    state.current_stage_message = Some(stage_message);
    state.current_job_lifecycle_progress =
        state.current_job_lifecycle_progress.max(lifecycle_progress);
    state
        .job_lifecycle_progress
        .entry(job_id.to_string())
        .and_modify(|current| *current = current.max(lifecycle_progress))
        .or_insert(lifecycle_progress);
}

fn sanitize_terminal_state(state: &mut BatchState) {
    if matches!(
        state.status,
        BatchStatus::Completed | BatchStatus::Failed | BatchStatus::Cancelled
    ) {
        clear_terminal_progress_fields(state);
    }
}

async fn collect_valid_video_inputs(entries: Vec<String>) -> Vec<String> {
    let mut files = Vec::new();
    let mut seen_paths = HashSet::new();

    for entry in entries {
        let entry_path = PathBuf::from(&entry);
        let metadata = match tokio::fs::metadata(&entry_path).await {
            Ok(metadata) => metadata,
            Err(e) => {
                warn!(
                    "Skipping input entry (metadata failed): {} ({})",
                    entry_path.display(),
                    e
                );
                continue;
            }
        };

        if metadata.is_dir() {
            let mut read_dir = match tokio::fs::read_dir(&entry_path).await {
                Ok(read_dir) => read_dir,
                Err(e) => {
                    warn!(
                        "Skipping input folder (read_dir failed): {} ({})",
                        entry_path.display(),
                        e
                    );
                    continue;
                }
            };

            while let Ok(Some(child)) = read_dir.next_entry().await {
                let child_path = child.path();
                if child_path.is_file() && OsUtils::has_supported_video_extension(&child_path) {
                    let child_path_str = child_path.to_string_lossy().to_string();
                    if seen_paths.insert(child_path_str.clone()) {
                        files.push(child_path_str);
                    }
                }
            }
            continue;
        }

        if metadata.is_file() && OsUtils::has_supported_video_extension(&entry_path) {
            if seen_paths.insert(entry.clone()) {
                files.push(entry);
            }
        }
    }
    files
}

enum JobOutcome {
    Processed,
    Skipped,
    Cancelled,
}

pub async fn start_batch(
    app: AppHandle,
    manager: State<'_, BatchManager>,
    files: Vec<String>,
    settings: BatchJobSettings,
) -> Result<(), String> {
    {
        let state = manager.state.lock().await;
        if state.status == BatchStatus::Processing {
            return Err("A batch is already processing".to_string());
        }
    }

    manager.clear().await;

    if settings.output_dir.trim().is_empty() {
        return Err("Output directory cannot be empty".to_string());
    }
    let root_output_dir = PathBuf::from(&settings.output_dir);
    if let Ok(cleaned) = crate::video::paths::cleanup_orphan_temp_outputs(&root_output_dir) {
        if cleaned > 0 {
            warn!(
                "Cleaned {} stale temporary render output(s) under {}",
                cleaned,
                root_output_dir.display()
            );
        }
    }

    let valid_input_files = collect_valid_video_inputs(files).await;
    if valid_input_files.is_empty() {
        return Err("No valid video files found in selected input".to_string());
    }

    let targets = normalize_targets(&settings.targets)?;
    let thumb_dir = if let Ok(runtime) = crate::runtime_paths::RuntimePaths::from_app(&app) {
        runtime.thumbnail_cache_dir()
    } else {
        let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
        cache_dir.join("thumbnails")
    };
    let _ = tokio::fs::create_dir_all(&thumb_dir).await;

    let mut jobs = Vec::new();
    let mut initial_progress = Vec::new();
    let session_id = Uuid::new_v4().to_string();
    let thumbnail_transform = targets
        .first()
        .and_then(|target| target.job.effects.transform.clone());

    // Parallel preparation: Probe and Thumbnail for each input file
    let mut preparation_tasks = tokio::task::JoinSet::new();
    for file in valid_input_files {
        let app_c = app.clone();
        let thumb_dir_c = thumb_dir.clone();
        let thumbnail_transform_c = thumbnail_transform.clone();
        preparation_tasks.spawn(async move {
            let res = crate::video::probe::check_file_ready(&app_c, &file).await;
            let (duration, probe_error) = match res {
                Ok(readiness) => (readiness.estimated_duration_secs, None),
                Err(e) => {
                    warn!("Failed to probe video file {}: {}", file, e);
                    (0.0, Some(e.to_string()))
                }
            };

            let thumb_name = format!("{}.jpg", Uuid::new_v4());
            let thumb_dest = thumb_dir_c.join(thumb_name);
            let thumb_dest_str = thumb_dest.to_string_lossy().to_string();

            let thumb_path = match crate::video::probe::generate_thumbnail(
                &app_c,
                &file,
                &thumb_dest_str,
                thumbnail_transform_c.as_ref(),
            )
            .await
            {
                Ok(p) => Some(p),
                Err(e) => {
                    warn!("Failed to generate thumbnail for {}: {}", file, e);
                    None
                }
            };

            (file, duration, thumb_path, probe_error)
        });
    }

    while let Some(res) = preparation_tasks.join_next().await {
        if let Ok((file, duration, thumb_path, probe_error)) = res {
            for target in &targets {
                let output_path = crate::video::paths::resolve_output_path(
                    &root_output_dir,
                    Path::new(&file),
                    target,
                    settings.enable_subfolders,
                );

                let alt_output_path = crate::video::paths::resolve_output_path(
                    &root_output_dir,
                    Path::new(&file),
                    target,
                    !settings.enable_subfolders,
                );

                let job_id = Uuid::new_v4().to_string();
                let job = BatchJob {
                    id: job_id.clone(),
                    input_path: file.clone(),
                    output: target.job.clone(),
                    resolved_output_path: output_path.to_string_lossy().to_string(),
                    alt_output_path: Some(alt_output_path.to_string_lossy().to_string()),
                    thumbnail_path: thumb_path.clone(),
                };

                initial_progress.push(FileProgress {
                    session_id: session_id.clone(),
                    job_id: job_id.clone(),
                    file_path: file.clone(),
                    ratio: target.job.ratio.clone(),
                    progress: 0.0,
                    status: if let Some(err) = &probe_error {
                        JobStatus::Failed(err.clone())
                    } else {
                        JobStatus::Queued
                    },
                    thumbnail_path: thumb_path.clone(),
                    duration_secs: duration,
                    selection: target.job.selection.clone(),
                });

                if probe_error.is_none() {
                    jobs.push(job);
                }
            }
        }
    }

    if jobs.is_empty() {
        return Ok(());
    }

    manager.add_jobs(jobs, initial_progress).await;

    let mut state = manager.state.lock().await;
    if state.status == BatchStatus::Processing {
        return Ok(());
    }
    state.status = BatchStatus::Processing;
    state.session_id = Some(session_id.clone());
    state.completed_jobs = 0;
    state.failed_jobs = 0;
    state.processed_duration_secs = 0.0;
    state.start_time = Some(std::time::Instant::now());
    state.cancellation_token = tokio_util::sync::CancellationToken::new();

    // Emit initial full batch state
    drop(state);
    emit_batch_progress(&app, &manager.state).await;

    let state_clone = Arc::clone(&manager.state);
    let app_clone = app.clone();

    tokio::spawn(async move {
        let mut subtitle_cache: HashMap<String, PreparedSubtitle> = HashMap::new();
        let mut temp_srt_paths: Vec<PathBuf> = Vec::new();
        let mut temp_subtitle_font_dirs: Vec<PathBuf> = Vec::new();

        loop {
            let (job, token) = {
                let mut s = state_clone.lock().await;

                if s.cancellation_token.is_cancelled() || s.status == BatchStatus::Cancelled {
                    s.status = BatchStatus::Cancelled;
                    sanitize_terminal_state(&mut s);
                    break;
                }

                let job = s.queue.pop_front();
                if job.is_none() {
                    s.status = if s.failed_jobs > 0 {
                        BatchStatus::Failed
                    } else {
                        BatchStatus::Completed
                    };
                    sanitize_terminal_state(&mut s);
                    break;
                }

                let job = job.unwrap();
                s.current_job_id = Some(job.id.clone());
                s.current_job_lifecycle_progress = 0.0;
                s.job_lifecycle_progress.insert(job.id.clone(), 0.0);
                (job, s.cancellation_token.clone())
            };

            emit_batch_progress(&app_clone, &state_clone).await;

            let outcome = process_batch_job(
                &app_clone,
                &state_clone,
                job,
                token,
                &session_id,
                &mut subtitle_cache,
                &mut temp_srt_paths,
                &mut temp_subtitle_font_dirs,
            )
            .await;

            match outcome {
                JobOutcome::Cancelled => break,
                JobOutcome::Processed | JobOutcome::Skipped => {}
            }
        }

        {
            let mut s = state_clone.lock().await;
            if s.status == BatchStatus::Processing {
                s.status = if s.failed_jobs > 0 {
                    BatchStatus::Failed
                } else {
                    BatchStatus::Completed
                };
            }
            sanitize_terminal_state(&mut s);
        }

        // Cleanup temporary subtitle files
        for path in temp_srt_paths {
            let _ = std::fs::remove_file(path);
        }
        for path in temp_subtitle_font_dirs {
            let _ = std::fs::remove_dir_all(path);
        }

        emit_batch_progress(&app_clone, &state_clone).await;
    });

    Ok(())
}

async fn process_batch_job(
    app: &AppHandle,
    state: &Arc<Mutex<BatchState>>,
    job: BatchJob,
    token: tokio_util::sync::CancellationToken,
    session_id: &str,
    subtitle_cache: &mut HashMap<String, PreparedSubtitle>,
    temp_srt_paths: &mut Vec<PathBuf>,
    temp_subtitle_font_dirs: &mut Vec<PathBuf>,
) -> JobOutcome {
    let job_id = job.id.clone();
    let input_path = job.input_path.clone();
    let output_path = PathBuf::from(&job.resolved_output_path);
    let alt_output_path = job.alt_output_path.as_deref().map(Path::new);

    if job.output.effects.skip_existing_enabled() {
        {
            let mut s = state.lock().await;
            set_stage(
                &mut s,
                &job_id,
                "checking_existing_output",
                "Checking for existing output...".to_string(),
                2.0,
            );
        }
        emit_batch_progress(app, state).await;

        if let Some(existing_path) = crate::video::convert::resolve_existing_output_for_skip(
            app,
            &output_path,
            alt_output_path,
        )
        .await
        {
            {
                let mut s = state.lock().await;
                let mut duration = 0.0;
                if let Some(p) = s.job_progress.get_mut(&job_id) {
                    p.status = JobStatus::Completed;
                    p.progress = 100.0;
                    duration = p.duration_secs;
                    let _ = app.emit("batch://file-status", p.clone());
                }
                s.completed_jobs += 1;
                s.processed_duration_secs += duration;
                s.current_job_lifecycle_progress = 100.0;
                s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
                s.current_job_id = None;
                set_stage(
                    &mut s,
                    &job_id,
                    "skipping_existing_output",
                    format!(
                        "Skipped existing output: {}",
                        existing_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("output")
                    ),
                    100.0,
                );
            }
            emit_batch_progress(app, state).await;
            return JobOutcome::Skipped;
        }
    }

    {
        let mut s = state.lock().await;
        if let Some(p) = s.job_progress.get_mut(&job.id) {
            p.status = JobStatus::Processing;
            let _ = app.emit("batch://file-status", p.clone());
        }
    }
    emit_batch_progress(app, state).await;

    let should_prepare_subtitles = job.output.effects.export_subtitles_enabled()
        || job.output.effects.burn_subtitles_enabled();
    let weights = LifecycleWeights::for_job(should_prepare_subtitles);
    let session_id = session_id.to_string();

    {
        let mut s = state.lock().await;
        set_stage(
            &mut s,
            &job_id,
            "preparing_video",
            format!(
                "Preparing {}",
                Path::new(&input_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("video")
            ),
            weights.prepare,
        );
    }
    emit_batch_progress(app, state).await;

    let mut prepared_subtitle = None;
    if should_prepare_subtitles {
        {
            let mut s = state.lock().await;
            set_stage(
                &mut s,
                &job_id,
                "preparing_subtitles",
                "Preparing subtitles...".to_string(),
                weights.prepare,
            );
        }
        emit_batch_progress(app, state).await;

        let orientation_for_subtitles =
            match crate::video::probe::detect_orientation(app, &input_path).await {
                Ok(o) => o,
                Err(e) => {
                    let failure = format!("Failed to detect orientation for subtitles: {}", e);
                    let mut s = state.lock().await;
                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                        p.status = JobStatus::Failed(failure);
                    }
                    s.failed_jobs += 1;
                    emit_batch_progress(app, state).await;
                    return JobOutcome::Processed;
                }
            };

        let subtitle_job = crate::video::types::ResolvedJob {
            id: "subtitle-layout-job".to_string(),
            session_id: session_id.clone(),
            input_path: input_path.clone(),
            output_path: String::new(),
            alt_output_path: None,
            ratio: job.output.ratio.clone(),
            encoding: job.output.encoding.clone(),
            effects: job.output.effects.clone(),
            platform_config: job.output.platform_config.clone(),
            subtitle_path: None,
            subtitle_fonts_dir: None,
        };

        let subtitle_plan =
            match crate::video::preset_adapter::create_render_plan_resolved(&subtitle_job) {
                Ok(p) => p,
                Err(e) => {
                    let failure = e.to_string();
                    let mut s = state.lock().await;
                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                        p.status = JobStatus::Failed(failure);
                    }
                    s.failed_jobs += 1;
                    emit_batch_progress(app, state).await;
                    return JobOutcome::Processed;
                }
            };

        let subtitle_layout = crate::video::render_layout::calculate_render_layout(
            &subtitle_plan,
            &orientation_for_subtitles,
            None,
        );

        let subtitle_style_key = serde_json::to_string(&job.output.effects.subtitle_overlay)
            .unwrap_or_else(|_| "subtitle-style".to_string());
        let subtitle_cache_key = format!(
            "{}|{}x{}|fg{}|blur{}|burn{}|export{}|{}",
            input_path,
            subtitle_layout.target_width,
            subtitle_layout.target_height,
            subtitle_layout.foreground_frame_height,
            job.output.effects.background_effect_enabled(),
            job.output.effects.burn_subtitles_enabled(),
            job.output.effects.export_subtitles_enabled(),
            subtitle_style_key
        );

        if let Some(path) = subtitle_cache.get(&subtitle_cache_key) {
            prepared_subtitle = Some(path.clone());
            {
                let mut s = state.lock().await;
                set_stage(
                    &mut s,
                    &job_id,
                    "embedding_subtitles",
                    "Embedding subtitles...".to_string(),
                    weights.prepare + weights.subtitle,
                );
            }
            emit_batch_progress(app, state).await;
        } else {
            let is_export = job.output.effects.export_subtitles_enabled();
            let sub_output_dir = if is_export {
                Path::new(&job.resolved_output_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            } else {
                crate::os_utils::OsUtils::get_temp_dir(app)
                    .to_string_lossy()
                    .to_string()
            };
            let source_duration_secs = {
                let s = state.lock().await;
                s.job_progress
                    .get(&job_id)
                    .map(|p| p.duration_secs)
                    .unwrap_or(0.0)
            };

            match prepare_subtitles(
                app,
                &input_path,
                &sub_output_dir,
                source_duration_secs,
                job.output.effects.burn_subtitles_enabled(),
                is_export,
                subtitle_layout.target_width,
                subtitle_layout.target_height,
                subtitle_layout.foreground_frame_height,
                job.output.effects.background_effect_enabled(),
                &job.output.effects.subtitle_overlay,
                Some(token.clone()),
                Some(Box::new({
                    let state = state.clone();
                    let app = app.clone();
                    let session = session_id.clone();
                    let token = token.clone();
                    let jid_c = job_id.clone();
                    move |subtitle_percent: f32| {
                        let state = state.clone();
                        let app = app.clone();
                        let session = session.clone();
                        let token = token.clone();
                        let jid_c = jid_c.clone();
                        tokio::spawn(async move {
                            if token.is_cancelled() {
                                return;
                            }
                            let mut s = state.lock().await;
                            if s.session_id.as_deref() != Some(session.as_str()) {
                                return;
                            }
                            let lifecycle = weights.prepare
                                + (weights.subtitle * (subtitle_percent.clamp(0.0, 100.0) / 100.0));
                            set_stage(
                                &mut s,
                                &jid_c,
                                "generating_subtitles",
                                "Generating subtitles...".to_string(),
                                lifecycle,
                            );
                            drop(s);
                            emit_batch_progress(&app, &state).await;
                        });
                    }
                })),
            )
            .await
            {
                Ok(path) => {
                    if !is_export {
                        temp_srt_paths.push(path.path.clone());
                        if let Some(fonts_dir) = &path.fonts_dir {
                            temp_subtitle_font_dirs.push(fonts_dir.clone());
                        }
                    }
                    subtitle_cache.insert(subtitle_cache_key, path.clone());
                    prepared_subtitle = Some(path);
                    {
                        let mut s = state.lock().await;
                        set_stage(
                            &mut s,
                            &job_id,
                            "embedding_subtitles",
                            "Embedding subtitles...".to_string(),
                            weights.prepare + weights.subtitle,
                        );
                    }
                    emit_batch_progress(app, state).await;
                }
                Err(e) => {
                    if token.is_cancelled() {
                        let mut s = state.lock().await;
                        s.status = BatchStatus::Cancelled;
                        sanitize_terminal_state(&mut s);
                        return JobOutcome::Cancelled;
                    }
                    let failure = e.to_string();
                    {
                        let mut s = state.lock().await;
                        if let Some(p) = s.job_progress.get_mut(&job_id) {
                            p.status = JobStatus::Failed(failure.clone());
                        }
                        s.failed_jobs += 1;
                    }
                    emit_batch_progress(app, state).await;
                    return JobOutcome::Processed;
                }
            }
        }
    }

    let resolved_job = crate::video::types::ResolvedJob {
        id: job_id.clone(),
        session_id: session_id.clone(),
        input_path: input_path.clone(),
        output_path: job.resolved_output_path.clone(),
        alt_output_path: job.alt_output_path.clone(),
        ratio: job.output.ratio.clone(),
        encoding: job.output.encoding.clone(),
        effects: job.output.effects.clone(),
        platform_config: job.output.platform_config.clone(),
        subtitle_path: prepared_subtitle
            .as_ref()
            .map(|prepared| prepared.path.clone()),
        subtitle_fonts_dir: prepared_subtitle
            .as_ref()
            .and_then(|prepared| prepared.fonts_dir.clone()),
    };

    if token.is_cancelled() {
        let mut s = state.lock().await;
        s.status = BatchStatus::Cancelled;
        sanitize_terminal_state(&mut s);
        return JobOutcome::Cancelled;
    }

    let state_c = state.clone();
    let jid_c = job_id.clone();
    let app_c = app.clone();
    let session_c = session_id.clone();
    let token_c = token.clone();

    {
        let mut s = state.lock().await;
        set_stage(
            &mut s,
            &job_id,
            "preparing_render",
            "Preparing render...".to_string(),
            weights.prepare + weights.subtitle + weights.render_prepare,
        );
    }
    emit_batch_progress(app, state).await;

    let on_progress = Box::new(move |percent: f32| {
        let percent = percent.clamp(0.0, 100.0);
        let state = state_c.clone();
        let jid = jid_c.clone();
        let app = app_c.clone();
        let session = session_c.clone();
        let token = token_c.clone();
        tokio::spawn(async move {
            if token.is_cancelled() {
                return;
            }
            {
                let mut s = state.lock().await;
                if s.session_id.as_deref() != Some(session.as_str()) {
                    return;
                }
                if let Some(p) = s.job_progress.get_mut(&jid) {
                    p.progress = percent;
                } else {
                    return;
                }
                let render_base = weights.prepare + weights.subtitle + weights.render_prepare;
                let lifecycle =
                    render_base + (weights.rendering * (percent.clamp(0.0, 100.0) / 100.0));
                set_stage(
                    &mut s,
                    &jid,
                    "rendering_video",
                    "Rendering video...".to_string(),
                    lifecycle,
                );
            }
            emit_batch_progress(&app, &state).await;
        });
    });

    let result = render_single(app, resolved_job, Some(token.clone()), Some(on_progress)).await;

    if token.is_cancelled() {
        let mut s = state.lock().await;
        s.status = BatchStatus::Cancelled;
        sanitize_terminal_state(&mut s);
        return JobOutcome::Cancelled;
    }

    match result {
        Ok(_) => {
            let mut s = state.lock().await;
            set_stage(
                &mut s,
                &job_id,
                "finalizing_output",
                "Finalizing output...".to_string(),
                100.0 - weights.finalize,
            );
            let mut duration = 0.0;
            if let Some(p) = s.job_progress.get_mut(&job_id) {
                p.status = JobStatus::Completed;
                p.progress = 100.0;
                duration = p.duration_secs;
                let _ = app.emit("batch://file-status", p.clone());
            }
            s.completed_jobs += 1;
            s.processed_duration_secs += duration;
            s.current_job_lifecycle_progress = 100.0;
            s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
            s.current_job_id = None;
        }
        Err(e) => {
            let mut s = state.lock().await;
            let mut duration = 0.0;
            if let Some(p) = s.job_progress.get_mut(&job_id) {
                p.status = JobStatus::Failed(e.to_string());
                duration = p.duration_secs;
                let _ = app.emit("batch://file-status", p.clone());
            }
            s.failed_jobs += 1;
            s.processed_duration_secs += duration;
            s.current_job_lifecycle_progress = 100.0;
            s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
            s.current_job_id = None;
        }
    }

    emit_batch_progress(app, state).await;

    JobOutcome::Processed
}

/// Duration-weighted aggregation of processed time across all active jobs.
///
/// `active_jobs` is a slice of `(progress_ratio, duration_secs)` pairs where
/// `progress_ratio` is normalized to `0.0..=1.0`. Completed duration is the
/// raw duration already accumulated by finished jobs and is added once. Every
/// active job contributes `progress_ratio * duration_secs`; queued/not-started
/// jobs are never passed in and therefore contribute nothing.
fn calculate_processed_duration(completed_duration_secs: f64, active_jobs: &[(f32, f64)]) -> f64 {
    let mut total = completed_duration_secs;
    for &(progress_ratio, duration_secs) in active_jobs {
        total += (progress_ratio as f64) * duration_secs;
    }
    total
}

/// Collects the duration-weighted contribution of every currently-active job.
///
/// A job is active iff its `FileProgress.status` is `Processing`. For each such
/// job the lifecycle progress recorded per job (from `set_stage`) is converted
/// to a ratio and paired with its duration. The result is ready to feed into
/// `calculate_processed_duration`; this deliberately does not depend on
/// `current_job_id` identifying the only active job.
fn active_job_duration_contributions(state: &BatchState) -> Vec<(f32, f64)> {
    state
        .job_progress
        .iter()
        .filter(|(_, p)| matches!(p.status, JobStatus::Processing))
        .map(|(job_id, p)| {
            let lifecycle = state
                .job_lifecycle_progress
                .get(job_id)
                .copied()
                .unwrap_or(p.progress)
                .clamp(0.0, 100.0);
            ((lifecycle / 100.0), p.duration_secs)
        })
        .collect()
}

fn calculate_stats(state: &BatchState) -> (f32, f32, Option<f64>, f64) {
    let total = state.total_jobs;
    let completed = state.completed_jobs;
    let failed = state.failed_jobs;

    let mut processed_secs = calculate_processed_duration(
        state.processed_duration_secs,
        &active_job_duration_contributions(state),
    );
    processed_secs = processed_secs.clamp(0.0, state.total_duration_secs.max(0.0));

    let percentage = if state.total_duration_secs > 0.0 {
        (((processed_secs / state.total_duration_secs) * 100.0) as f32).clamp(0.0, 100.0)
    } else if total > 0 {
        (((completed + failed) as f32 / total as f32) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let (speed, eta_seconds) = if let Some(start) = state.start_time {
        let elapsed = start.elapsed().as_secs_f32();
        if elapsed > 0.1 && processed_secs > 0.1 {
            let speed = processed_secs as f32 / elapsed;
            let remaining_duration = (state.total_duration_secs - processed_secs).max(0.0);
            let eta = if speed > 0.01 {
                Some(remaining_duration / speed as f64)
            } else {
                None
            };
            (speed, eta)
        } else {
            (0.0, None)
        }
    } else {
        (0.0, None)
    };

    (percentage, speed, eta_seconds, processed_secs)
}

pub async fn cancel_batch(manager: State<'_, BatchManager>) -> Result<(), String> {
    manager.cancel().await;
    Ok(())
}

pub async fn get_batch_status(manager: State<'_, BatchManager>) -> Result<BatchProgress, String> {
    let mut state = manager.state.lock().await;
    sanitize_terminal_state(&mut state);
    let (percentage, speed, eta_seconds, processed_secs) = calculate_stats(&state);

    let mut queue = Vec::new();
    for id in &state.all_job_ids {
        if let Some(p) = state.job_progress.get(id) {
            queue.push(p.clone());
        }
    }

    Ok(BatchProgress {
        session_id: state.session_id.clone(),
        total_jobs: state.total_jobs,
        completed_jobs: state.completed_jobs,
        failed_jobs: state.failed_jobs,
        percentage,
        status: state.status.clone(),
        current_job_id: state.current_job_id.clone(),
        queue,
        eta_seconds,
        speed,
        total_duration_secs: state.total_duration_secs,
        processed_duration_secs: processed_secs,
        current_stage_id: state.current_stage_id.clone(),
        current_stage_message: state.current_stage_message.clone(),
    })
}

pub async fn clear_batch(manager: State<'_, BatchManager>) -> Result<(), String> {
    manager.clear().await;
    Ok(())
}

async fn emit_batch_progress(app: &AppHandle, state_mutex: &Arc<Mutex<BatchState>>) {
    let mut state = state_mutex.lock().await;
    sanitize_terminal_state(&mut state);
    let (percentage, speed, eta_seconds, processed_secs) = calculate_stats(&state);

    let mut queue = Vec::new();
    for id in &state.all_job_ids {
        if let Some(p) = state.job_progress.get(id) {
            queue.push(p.clone());
        }
    }

    let _ = app.emit(
        "batch://progress",
        BatchProgress {
            session_id: state.session_id.clone(),
            total_jobs: state.total_jobs,
            completed_jobs: state.completed_jobs,
            failed_jobs: state.failed_jobs,
            percentage,
            status: state.status.clone(),
            current_job_id: state.current_job_id.clone(),
            queue,
            eta_seconds,
            speed,
            total_duration_secs: state.total_duration_secs,
            processed_duration_secs: processed_secs,
            current_stage_id: state.current_stage_id.clone(),
            current_stage_message: state.current_stage_message.clone(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::types::{AspectRatio, SelectionMetadata, TargetType};
    use std::collections::VecDeque;
    use tokio_util::sync::CancellationToken;

    fn assert_duration_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected processed duration {}, got {}",
            expected,
            actual
        );
    }

    fn assert_percent_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-2,
            "expected percentage {}, got {}",
            expected,
            actual
        );
    }

    fn test_file_progress(job_id: &str, duration_secs: f64, status: JobStatus) -> FileProgress {
        FileProgress {
            session_id: "test-session".to_string(),
            job_id: job_id.to_string(),
            file_path: format!("{}.mp4", job_id),
            ratio: AspectRatio::Ratio9x16,
            progress: 0.0,
            status,
            thumbnail_path: None,
            duration_secs,
            selection: SelectionMetadata {
                source_type: TargetType::AspectRatio,
                source_id: "test-source".to_string(),
                label: "test".to_string(),
            },
        }
    }

    fn test_batch_state(
        files: Vec<FileProgress>,
        completed_jobs: usize,
        processed_duration_secs: f64,
        total_duration_secs: f64,
        lifecycle: &[(&str, f32)],
        current_job_id: Option<&str>,
    ) -> BatchState {
        let job_progress = files
            .into_iter()
            .map(|p| (p.job_id.clone(), p))
            .collect::<HashMap<_, _>>();
        let job_lifecycle_progress = lifecycle
            .iter()
            .map(|(id, value)| ((*id).to_string(), *value))
            .collect::<HashMap<_, _>>();
        let all_job_ids = job_progress.keys().cloned().collect::<Vec<_>>();
        let total_jobs = all_job_ids.len();
        BatchState {
            session_id: Some("test-session".to_string()),
            queue: VecDeque::new(),
            job_progress,
            all_job_ids,
            current_job_id: current_job_id.map(|id| id.to_string()),
            completed_jobs,
            failed_jobs: 0,
            total_jobs,
            cancellation_token: CancellationToken::new(),
            status: BatchStatus::Processing,
            start_time: None,
            total_duration_secs,
            processed_duration_secs,
            current_stage_id: None,
            current_stage_message: None,
            current_job_lifecycle_progress: 0.0,
            job_lifecycle_progress,
        }
    }

    #[test]
    fn multi_job_aggregation_sums_all_active_jobs() {
        let active = &[(0.8, 100.0), (0.3, 200.0), (0.5, 300.0)];
        let total = calculate_processed_duration(0.0, active);
        assert_duration_close(total, 290.0);
    }

    #[test]
    fn aggregation_includes_completed_duration_once() {
        assert_duration_close(calculate_processed_duration(120.0, &[]), 120.0);

        let active = &[(0.8, 100.0), (0.3, 200.0), (0.5, 300.0)];
        let total = calculate_processed_duration(120.0, active);
        assert_duration_close(total, 410.0);
    }

    #[test]
    fn calculate_stats_single_active_job_matches_old_formula() {
        let state = test_batch_state(
            vec![
                test_file_progress("done", 120.0, JobStatus::Completed),
                test_file_progress("active", 100.0, JobStatus::Processing),
            ],
            1,
            120.0,
            220.0,
            &[("active", 80.0)],
            Some("active"),
        );

        let (percentage, _, _, processed_secs) = calculate_stats(&state);
        assert_duration_close(processed_secs, 200.0);
        assert_percent_close(percentage, 200.0 / 220.0 * 100.0);
    }

    #[test]
    fn calculate_stats_sums_all_active_jobs_regardless_of_current_job_id() {
        let state = test_batch_state(
            vec![
                test_file_progress("a", 100.0, JobStatus::Processing),
                test_file_progress("b", 200.0, JobStatus::Processing),
                test_file_progress("c", 300.0, JobStatus::Processing),
            ],
            0,
            0.0,
            600.0,
            &[("a", 80.0), ("b", 30.0), ("c", 50.0)],
            Some("c"),
        );

        let (percentage, _, _, processed_secs) = calculate_stats(&state);
        assert_duration_close(processed_secs, 290.0);
        assert_percent_close(percentage, 290.0 / 600.0 * 100.0);
    }

    #[test]
    fn calculate_stats_mixed_batch_includes_completed_and_all_active() {
        let state = test_batch_state(
            vec![
                test_file_progress("done", 250.0, JobStatus::Completed),
                test_file_progress("active-1", 100.0, JobStatus::Processing),
                test_file_progress("active-2", 300.0, JobStatus::Processing),
                test_file_progress("queued", 75.0, JobStatus::Queued),
            ],
            1,
            250.0,
            725.0,
            &[("active-1", 80.0), ("active-2", 50.0)],
            Some("active-2"),
        );

        let (_, _, _, processed_secs) = calculate_stats(&state);
        assert_duration_close(processed_secs, 480.0);
    }

    #[test]
    fn calculate_stats_without_active_jobs_only_counts_completed() {
        let state = test_batch_state(
            vec![
                test_file_progress("done", 100.0, JobStatus::Completed),
                test_file_progress("queued", 50.0, JobStatus::Queued),
            ],
            1,
            100.0,
            150.0,
            &[],
            None,
        );

        let (percentage, _, _, processed_secs) = calculate_stats(&state);
        assert_duration_close(processed_secs, 100.0);
        assert_percent_close(percentage, 100.0 / 150.0 * 100.0);
    }

    #[test]
    fn calculate_stats_zero_progress_active_job_contributes_nothing() {
        let state = test_batch_state(
            vec![test_file_progress("active", 200.0, JobStatus::Processing)],
            0,
            0.0,
            200.0,
            &[("active", 0.0)],
            Some("active"),
        );

        let (percentage, _, _, processed_secs) = calculate_stats(&state);
        assert_duration_close(processed_secs, 0.0);
        assert_percent_close(percentage, 0.0);
    }
}
