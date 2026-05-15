use crate::os_utils::OsUtils;
use crate::video::convert::{prepare_subtitles, render_single};
use crate::video::queue::{BatchManager, BatchState};
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
    stage_id: &str,
    stage_message: String,
    lifecycle_progress: f32,
) {
    state.current_stage_id = Some(stage_id.to_string());
    state.current_stage_message = Some(stage_message);
    state.current_job_lifecycle_progress = state.current_job_lifecycle_progress.max(lifecycle_progress);
}

async fn collect_valid_video_inputs(entries: Vec<String>) -> Vec<String> {
    let mut files = Vec::new();
    let mut seen_paths = HashSet::new();

    for entry in entries {
        let entry_path = PathBuf::from(&entry);
        let metadata = match tokio::fs::metadata(&entry_path).await {
            Ok(metadata) => metadata,
            Err(e) => {
                warn!("Skipping input entry (metadata failed): {} ({})", entry_path.display(), e);
                continue;
              }
        };

        if metadata.is_dir() {
            let mut read_dir = match tokio::fs::read_dir(&entry_path).await {
                Ok(read_dir) => read_dir,
                Err(e) => {
                    warn!("Skipping input folder (read_dir failed): {} ({})", entry_path.display(), e);
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
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let thumb_dir = cache_dir.join("thumbnails");
    let _ = tokio::fs::create_dir_all(&thumb_dir).await;

    let mut jobs = Vec::new();
    let mut initial_progress = Vec::new();
    let session_id = Uuid::new_v4().to_string();

    // Parallel preparation: Probe and Thumbnail for each input file
    let mut preparation_tasks = tokio::task::JoinSet::new();
    for file in valid_input_files {
        let app_c = app.clone();
        let thumb_dir_c = thumb_dir.clone();
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
            
            let thumb_path = match crate::video::probe::generate_thumbnail(&app_c, &file, &thumb_dest_str).await {
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
        let mut subtitle_cache: HashMap<String, PathBuf> = HashMap::new();
        let mut temp_srt_paths: Vec<PathBuf> = Vec::new();

        loop {
            let (job, token) = {
                let mut s = state_clone.lock().await;
                
                if s.cancellation_token.is_cancelled() || s.status == BatchStatus::Cancelled {
                    s.status = BatchStatus::Cancelled;
                    break;
                }
                
                let job = s.queue.pop_front();
                if job.is_none() {
                    s.status = if s.failed_jobs > 0 { BatchStatus::Failed } else { BatchStatus::Completed };
                    s.current_job_id = None;
                    break;
                }
                
                let job = job.unwrap();
                s.current_job_id = Some(job.id.clone());
                s.current_job_lifecycle_progress = 0.0;
                (job, s.cancellation_token.clone())
            };

            emit_batch_progress(&app_clone, &state_clone).await;

            let job_id = job.id.clone();
            let input_path = job.input_path.clone();
            let output_path = PathBuf::from(&job.resolved_output_path);
            let alt_output_path = job.alt_output_path.as_deref().map(Path::new);

            if job.output.effects.skip_existing_enabled() {
                {
                    let mut s = state_clone.lock().await;
                    set_stage(
                        &mut s,
                        "checking_existing_output",
                        "Checking for existing output...".to_string(),
                        2.0,
                    );
                }
                emit_batch_progress(&app_clone, &state_clone).await;

                if let Some(existing_path) = crate::video::convert::resolve_existing_output_for_skip(
                    &app_clone,
                    &output_path,
                    alt_output_path,
                )
                .await
                {
                    {
                        let mut s = state_clone.lock().await;
                        let mut duration = 0.0;
                        if let Some(p) = s.job_progress.get_mut(&job_id) {
                            p.status = JobStatus::Completed;
                            p.progress = 100.0;
                            duration = p.duration_secs;
                            let _ = app_clone.emit("batch://file-status", p.clone());
                        }
                        s.completed_jobs += 1;
                        s.processed_duration_secs += duration;
                        s.current_job_lifecycle_progress = 100.0;
                        set_stage(
                            &mut s,
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
                    emit_batch_progress(&app_clone, &state_clone).await;
                    continue;
                }
            }

            {
                let mut s = state_clone.lock().await;
                if let Some(p) = s.job_progress.get_mut(&job.id) {
                    p.status = JobStatus::Processing;
                    let _ = app_clone.emit("batch://file-status", p.clone());
                }
            }
            emit_batch_progress(&app_clone, &state_clone).await;

            let should_prepare_subtitles = job.output.effects.export_subtitles_enabled()
                || job.output.effects.burn_subtitles_enabled();
            let weights = LifecycleWeights::for_job(should_prepare_subtitles);

            {
                let mut s = state_clone.lock().await;
                set_stage(
                    &mut s,
                    "preparing_video",
                    format!("Preparing {}", Path::new(&input_path).file_name().and_then(|n| n.to_str()).unwrap_or("video")),
                    weights.prepare,
                );
            }
            emit_batch_progress(&app_clone, &state_clone).await;

            let mut prepared_subtitle = None;
            if should_prepare_subtitles {
                {
                    let mut s = state_clone.lock().await;
                    set_stage(
                        &mut s,
                        "preparing_subtitles",
                        "Preparing subtitles...".to_string(),
                        weights.prepare,
                    );
                }
                emit_batch_progress(&app_clone, &state_clone).await;

                if let Some(path) = subtitle_cache.get(&input_path) {
                    prepared_subtitle = Some(path.clone());
                    {
                        let mut s = state_clone.lock().await;
                        set_stage(
                            &mut s,
                            "embedding_subtitles",
                            "Embedding subtitles...".to_string(),
                            weights.prepare + weights.subtitle,
                        );
                    }
                    emit_batch_progress(&app_clone, &state_clone).await;
                } else {
                    let is_export = job.output.effects.export_subtitles_enabled();
                    let sub_output_dir = if is_export {
                        Path::new(&job.resolved_output_path)
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| ".".to_string())
                    } else {
                        crate::os_utils::OsUtils::get_temp_dir(&app_clone)
                            .to_string_lossy()
                            .to_string()
                    };
                    let source_duration_secs = {
                        let s = state_clone.lock().await;
                        s.job_progress
                            .get(&job_id)
                            .map(|p| p.duration_secs)
                            .unwrap_or(0.0)
                    };

                    let target_ratio = job.output.ratio.get_ratio();
                    let target_height = 1080;
                    let target_width = (target_height as f32 * target_ratio) as u32;

                    match prepare_subtitles(
                        &app_clone,
                        &input_path,
                        &sub_output_dir,
                        source_duration_secs,
                        job.output.effects.burn_subtitles_enabled(),
                        is_export,
                        target_width,
                        target_height,
                        Some(token.clone()),
                        Some(Box::new({
                            let state = state_clone.clone();
                            let app = app_clone.clone();
                            let session = session_id.clone();
                            let token = token.clone();
                            move |subtitle_percent: f32| {
                                let state = state.clone();
                                let app = app.clone();
                                let session = session.clone();
                                let token = token.clone();
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
                                        "generating_subtitles",
                                        "Generating subtitles...".to_string(),
                                        lifecycle,
                                    );
                                    drop(s);
                                    emit_batch_progress(&app, &state).await;
                                });
                            }
                        })),
                    ).await {
                        Ok(path) => {
                            if !is_export {
                                temp_srt_paths.push(path.clone());
                            }
                            subtitle_cache.insert(input_path.clone(), path.clone());
                            prepared_subtitle = Some(path);
                            {
                                let mut s = state_clone.lock().await;
                                set_stage(
                                    &mut s,
                                    "embedding_subtitles",
                                    "Embedding subtitles...".to_string(),
                                    weights.prepare + weights.subtitle,
                                );
                            }
                            emit_batch_progress(&app_clone, &state_clone).await;
                        }
                        Err(e) => {
                            if token.is_cancelled() {
                                let mut s = state_clone.lock().await;
                                s.status = BatchStatus::Cancelled;
                                break;
                            }
                            let failure = e.to_string();
                            {
                                let mut s = state_clone.lock().await;
                                if let Some(p) = s.job_progress.get_mut(&job_id) {
                                    p.status = JobStatus::Failed(failure.clone());
                                }
                                s.failed_jobs += 1;
                            }
                            emit_batch_progress(&app_clone, &state_clone).await;
                            continue;
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
                subtitle_path: prepared_subtitle,
            };

            if token.is_cancelled() {
                let mut s = state_clone.lock().await;
                s.status = BatchStatus::Cancelled;
                break;
            }

            let state_c = state_clone.clone();
            let jid_c = job_id.clone();
            let app_c = app_clone.clone();
            let session_c = session_id.clone();
            let token_c = token.clone();

            {
                let mut s = state_clone.lock().await;
                set_stage(
                    &mut s,
                    "preparing_render",
                    "Preparing render...".to_string(),
                    weights.prepare + weights.subtitle + weights.render_prepare,
                );
            }
            emit_batch_progress(&app_clone, &state_clone).await;
            
            let on_progress = Box::new(move |percent: f32| {
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
                        let lifecycle = render_base + (weights.rendering * (percent.clamp(0.0, 100.0) / 100.0));
                        set_stage(
                            &mut s,
                            "rendering_video",
                            "Rendering video...".to_string(),
                            lifecycle,
                        );
                    }
                    emit_batch_progress(&app, &state).await;
                });
            });

            let result = render_single(
                &app_clone,
                resolved_job,
                Some(token.clone()),
                Some(on_progress),
            )
            .await;

            if token.is_cancelled() {
                 let mut s = state_clone.lock().await;
                 s.status = BatchStatus::Cancelled;
                 break;
            }

            match result {
                Ok(_) => {
                    let mut s = state_clone.lock().await;
                    set_stage(
                        &mut s,
                        "finalizing_output",
                        "Finalizing output...".to_string(),
                        100.0 - weights.finalize,
                    );
                    let mut duration = 0.0;
                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                        p.status = JobStatus::Completed;
                        p.progress = 100.0;
                        duration = p.duration_secs;
                        let _ = app_clone.emit("batch://file-status", p.clone());
                    }
                    s.completed_jobs += 1;
                    s.processed_duration_secs += duration;
                    s.current_job_lifecycle_progress = 100.0;
                }
                Err(e) => {
                    let mut s = state_clone.lock().await;
                    let mut duration = 0.0;
                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                        p.status = JobStatus::Failed(e.to_string());
                        duration = p.duration_secs;
                        let _ = app_clone.emit("batch://file-status", p.clone());
                    }
                    s.failed_jobs += 1;
                    s.processed_duration_secs += duration;
                    s.current_job_lifecycle_progress = 100.0;
                }
            }

            emit_batch_progress(&app_clone, &state_clone).await;
        }

        {
            let mut s = state_clone.lock().await;
            if s.status == BatchStatus::Processing {
                 s.status = BatchStatus::Completed;
            }
            s.current_stage_id = Some("batch_finalizing".to_string());
            s.current_stage_message = Some("Finalizing batch...".to_string());
        }
        
        // Cleanup temporary SRT files
        for path in temp_srt_paths {
            let _ = std::fs::remove_file(path);
        }

        emit_batch_progress(&app_clone, &state_clone).await;
    });

    Ok(())
}

fn calculate_stats(state: &BatchState) -> (f32, f32, Option<f64>, f64) {
    let total = state.total_jobs;
    let completed = state.completed_jobs;
    let failed = state.failed_jobs;

    let mut processed_secs = state.processed_duration_secs;
    if let Some(current_id) = &state.current_job_id {
        if let Some(p) = state.job_progress.get(current_id) {
            processed_secs += (state.current_job_lifecycle_progress as f64 / 100.0) * p.duration_secs;
        }
    }

    let percentage = if state.total_duration_secs > 0.0 {
        ((processed_secs / state.total_duration_secs) * 100.0) as f32
    } else if total > 0 {
        ((completed + failed) as f32 / total as f32) * 100.0
    } else {
        0.0
    };

    let (speed, eta_seconds) = if let Some(start) = state.start_time {
        let elapsed = start.elapsed().as_secs_f32();
        if elapsed > 0.1 && processed_secs > 0.1 {
            let speed = processed_secs as f32 / elapsed;
            let remaining_duration = state.total_duration_secs - processed_secs;
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
    let state = manager.state.lock().await;
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
    let state = state_mutex.lock().await;
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
