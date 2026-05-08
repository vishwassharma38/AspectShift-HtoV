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

fn collect_valid_video_inputs(entries: Vec<String>) -> Vec<String> {
    let mut files = Vec::new();
    let mut seen_paths = HashSet::new();

    for entry in entries {
        let entry_path = PathBuf::from(&entry);
        let metadata = match std::fs::metadata(&entry_path) {
            Ok(metadata) => metadata,
            Err(e) => {
                warn!("Skipping input entry (metadata failed): {} ({})", entry_path.display(), e);
                continue;
            }
        };

        if metadata.is_dir() {
            let read_dir = match std::fs::read_dir(&entry_path) {
                Ok(read_dir) => read_dir,
                Err(e) => {
                    warn!("Skipping input folder (read_dir failed): {} ({})", entry_path.display(), e);
                    continue;
                }
            };

            for child in read_dir {
                if let Ok(child) = child {
                    let child_path = child.path();
                    if child_path.is_file() && OsUtils::has_supported_video_extension(&child_path) {
                        let child_path_str = child_path.to_string_lossy().to_string();
                        if seen_paths.insert(child_path_str.clone()) {
                            files.push(child_path_str);
                        }
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
    if settings.output_dir.trim().is_empty() {
        return Err("Output directory cannot be empty".to_string());
    }
    let root_output_dir = PathBuf::from(&settings.output_dir);

    let valid_input_files = collect_valid_video_inputs(files);
    if valid_input_files.is_empty() {
        return Err("No valid video files found in selected input".to_string());
    }

    let targets = normalize_targets(&settings.targets)?;
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let thumb_dir = cache_dir.join("thumbnails");
    std::fs::create_dir_all(&thumb_dir).map_err(|e| e.to_string())?;

    let mut jobs = Vec::new();
    let mut initial_progress = Vec::new();
    let mut thumb_cache: HashMap<String, String> = HashMap::new();
    let mut duration_cache: HashMap<String, f64> = HashMap::new();

    for file in valid_input_files {
        // Probe duration once per input file
        let duration = if let Some(d) = duration_cache.get(&file) {
            *d
        } else {
            match crate::video::probe::check_file_ready(&app, &file).await {
                Ok(readiness) => {
                    duration_cache.insert(file.clone(), readiness.estimated_duration_secs);
                    readiness.estimated_duration_secs
                }
                Err(e) => {
                    warn!("Failed to probe duration for {}: {}", file, e);
                    0.0
                }
            }
        };

        // Generate thumbnail once per input file
        let thumb_path = if let Some(cached) = thumb_cache.get(&file) {
            Some(cached.clone())
        } else {
            let thumb_name = format!("{}.jpg", Uuid::new_v4());
            let thumb_dest = thumb_dir.join(thumb_name);
            let thumb_dest_str = thumb_dest.to_string_lossy().to_string();
            
            match crate::video::probe::generate_thumbnail(&app, &file, &thumb_dest_str).await {
                Ok(p) => {
                    thumb_cache.insert(file.clone(), p.clone());
                    Some(p)
                }
                Err(e) => {
                    warn!("Failed to generate thumbnail for {}: {}", file, e);
                    None
                }
            }
        };

        for target in &targets {
            let output_path = crate::video::paths::resolve_output_path(
                &root_output_dir,
                Path::new(&file),
                target,
                settings.enable_subfolders,
            );

            let job_id = Uuid::new_v4().to_string();
            let job = BatchJob {
                id: job_id.clone(),
                input_path: file.clone(),
                output: target.job.clone(),
                resolved_output_path: output_path.to_string_lossy().to_string(),
                thumbnail_path: thumb_path.clone(),
            };

            initial_progress.push(FileProgress {
                job_id: job_id.clone(),
                file_path: file.clone(),
                ratio: target.job.ratio.clone(),
                progress: 0.0,
                status: JobStatus::Queued,
                thumbnail_path: thumb_path.clone(),
                duration_secs: duration,
            });

            jobs.push(job);
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
    state.completed_jobs = 0;
    state.failed_jobs = 0;
    state.processed_duration_secs = 0.0;
    state.start_time = Some(std::time::Instant::now());
    state.cancellation_token = tokio_util::sync::CancellationToken::new();

    // Emit initial full batch state
    drop(state); // release lock before emit_batch_progress
    emit_batch_progress(&app, &manager.state).await;

    let state_clone = Arc::clone(&manager.state);
    let app_clone = app.clone();

    tokio::spawn(async move {
        let mut subtitle_cache: HashMap<String, PathBuf> = HashMap::new();

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
                
                // Update status to Processing
                if let Some(p) = s.job_progress.get_mut(&job.id) {
                    p.status = JobStatus::Processing;
                    let _ = app_clone.emit("batch://file-status", p.clone());
                }
                
                (job, s.cancellation_token.clone())
            };

            emit_batch_progress(&app_clone, &state_clone).await;

            let job_id = job.id.clone();
            let input_path = job.input_path.clone();

            // RESOLUTION PHASE
            let should_prepare_subtitles = job.output.effects.generate_subtitles_enabled()
                || job.output.effects.burn_subtitles_enabled();

            let mut prepared_subtitle = None;
            if should_prepare_subtitles {
                if let Some(path) = subtitle_cache.get(&input_path) {
                    prepared_subtitle = Some(path.clone());
                } else {
                    let output_dir = Path::new(&job.resolved_output_path)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| ".".to_string());

                    match prepare_subtitles(&app_clone, &input_path, &output_dir).await {
                        Ok(path) => {
                            subtitle_cache.insert(input_path.clone(), path.clone());
                            prepared_subtitle = Some(path);
                        }
                        Err(e) => {
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
                input_path: input_path.clone(),
                output_path: job.resolved_output_path.clone(),
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

            // RENDER PHASE
            let state_c = state_clone.clone();
            let jid_c = job_id.clone();
            let app_c = app_clone.clone();
            
            let on_progress = Box::new(move |percent: f32| {
                let state = state_c.clone();
                let jid = jid_c.clone();
                let app = app_c.clone();
                tokio::spawn(async move {
                    {
                        let mut s = state.lock().await;
                        if let Some(p) = s.job_progress.get_mut(&jid) {
                            p.progress = percent;
                        }
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
                    let mut duration = 0.0;
                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                        p.status = JobStatus::Completed;
                        p.progress = 100.0;
                        duration = p.duration_secs;
                        let _ = app_clone.emit("batch://file-status", p.clone());
                    }
                    s.completed_jobs += 1;
                    s.processed_duration_secs += duration;
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
                }
            }

            emit_batch_progress(&app_clone, &state_clone).await;
        }

        {
            let mut s = state_clone.lock().await;
            if s.status == BatchStatus::Processing {
                 s.status = BatchStatus::Completed;
            }
        }
        emit_batch_progress(&app_clone, &state_clone).await;
    });

    Ok(())
}

pub async fn cancel_batch(manager: State<'_, BatchManager>) -> Result<(), String> {
    manager.cancel().await;
    Ok(())
}

pub async fn get_batch_status(manager: State<'_, BatchManager>) -> Result<BatchProgress, String> {
    let state = manager.state.lock().await;
    let total = state.total_jobs;
    let completed = state.completed_jobs;
    let failed = state.failed_jobs;
    
    let mut processed_secs = state.processed_duration_secs;
    if let Some(current_id) = &state.current_job_id {
        if let Some(p) = state.job_progress.get(current_id) {
             processed_secs += (p.progress as f64 / 100.0) * p.duration_secs;
        }
    }

    let percentage = if state.total_duration_secs > 0.0 {
        ((processed_secs / state.total_duration_secs) * 100.0) as f32
    } else if total > 0 {
        ((completed + failed) as f32 / total as f32) * 100.0
    } else {
        0.0
    };

    let mut queue = Vec::new();
    for id in &state.all_job_ids {
        if let Some(p) = state.job_progress.get(id) {
            queue.push(p.clone());
        }
    }

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

    Ok(BatchProgress {
        total_jobs: total,
        completed_jobs: completed,
        failed_jobs: failed,
        percentage,
        status: state.status.clone(),
        current_job_id: state.current_job_id.clone(),
        queue,
        eta_seconds,
        speed,
        total_duration_secs: state.total_duration_secs,
        processed_duration_secs: processed_secs,
    })
}

pub async fn clear_batch(manager: State<'_, BatchManager>) -> Result<(), String> {
    manager.clear().await;
    Ok(())
}

async fn emit_batch_progress(app: &AppHandle, state_mutex: &Arc<Mutex<BatchState>>) {
    let state = state_mutex.lock().await;
    let total = state.total_jobs;
    let completed = state.completed_jobs;
    let failed = state.failed_jobs;

    let mut processed_secs = state.processed_duration_secs;
    if let Some(current_id) = &state.current_job_id {
        if let Some(p) = state.job_progress.get(current_id) {
             processed_secs += (p.progress as f64 / 100.0) * p.duration_secs;
        }
    }

    let percentage = if state.total_duration_secs > 0.0 {
        ((processed_secs / state.total_duration_secs) * 100.0) as f32
    } else if total > 0 {
        ((completed + failed) as f32 / total as f32) * 100.0
    } else {
        0.0
    };

    let mut queue = Vec::new();
    for id in &state.all_job_ids {
        if let Some(p) = state.job_progress.get(id) {
            queue.push(p.clone());
        }
    }

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

    let _ = app.emit(
        "batch://progress",
        BatchProgress {
            total_jobs: total,
            completed_jobs: completed,
            failed_jobs: failed,
            percentage,
            status: state.status.clone(),
            current_job_id: state.current_job_id.clone(),
            queue,
            eta_seconds,
            speed,
            total_duration_secs: state.total_duration_secs,
            processed_duration_secs: processed_secs,
        },
    );
}
