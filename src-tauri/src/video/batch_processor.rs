use crate::os_utils::OsUtils;
use crate::video::convert::{prepare_subtitles, render_single};
use crate::video::queue::{BatchManager, BatchState};
use crate::video::targets::normalize_targets;
use crate::video::types::{
    BatchJob, BatchJobSettings, BatchProgress, FileProgress, JobStatus,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use tracing::{info, warn};

fn collect_valid_video_inputs(entries: Vec<String>) -> Vec<String> {
    let mut files = Vec::new();
    let mut seen_paths = HashSet::new();

    // Normalize incoming paths (files and folders) into validated video files only.
    for entry in entries {
        let entry_path = PathBuf::from(&entry);
        let metadata = match std::fs::metadata(&entry_path) {
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
            // Folder input: expand one level and keep only supported video files.
            let read_dir = match std::fs::read_dir(&entry_path) {
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

            for child in read_dir {
                let child = match child {
                    Ok(child) => child,
                    Err(e) => {
                        warn!(
                            "Skipping folder entry (invalid dir entry): {} ({})",
                            entry_path.display(),
                            e
                        );
                        continue;
                    }
                };

                let child_path = child.path();
                let child_metadata = match child.metadata() {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        warn!(
                            "Skipping entry (metadata failed): {} ({})",
                            child_path.display(),
                            e
                        );
                        continue;
                    }
                };

                if !child_metadata.is_file() {
                    info!("Skipping non-file entry: {}", child_path.display());
                    continue;
                }

                // Prevent directories/unsupported files from entering the ffprobe pipeline.
                if !OsUtils::has_supported_video_extension(&child_path) {
                    info!("Skipping unsupported input file: {}", child_path.display());
                    continue;
                }

                let child_path_str = child_path.to_string_lossy().to_string();
                if seen_paths.insert(child_path_str.clone()) {
                    files.push(child_path_str);
                }
            }
            continue;
        }

        if !metadata.is_file() {
            info!("Skipping non-file input entry: {}", entry_path.display());
            continue;
        }

        // Prevent unsupported files from entering the ffprobe pipeline.
        if !OsUtils::has_supported_video_extension(&entry_path) {
            info!("Skipping unsupported input file: {}", entry_path.display());
            continue;
        }

        if seen_paths.insert(entry.clone()) {
            files.push(entry);
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
    // A. Validate output_dir EARLY
    if settings.output_dir.trim().is_empty() {
        return Err("Output directory cannot be empty".to_string());
    }
    let root_output_dir = Path::new(&settings.output_dir);

    let valid_input_files = collect_valid_video_inputs(files);
    if valid_input_files.is_empty() {
        return Err("No valid video files found in selected input".to_string());
    }

    let mut jobs = Vec::new();
    let targets = normalize_targets(&settings.targets)?;

    for file in valid_input_files {
        for target in &targets {
            // Use the shared path resolver
            let output_path =
                crate::video::paths::resolve_output_path(
                    &root_output_dir,
                    Path::new(&file),
                    target,
                    settings.enable_subfolders,
                );

            // B. Ensure directory exists BEFORE skip logic
            if let Some(parent) = output_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Err(format!(
                        "Failed to create output directory {}: {}",
                        parent.display(),
                        e
                    ));
                }
            }

            let output_path_str = output_path.to_string_lossy().to_string();

            // E. Add debug logging (non-intrusive)
            info!("Resolved output path: {}", output_path_str);

            // Skip logic
            if target.job.effects.skip_existing_enabled() && output_path.exists() {
                continue;
            }

            jobs.push(BatchJob {
                input_path: file.clone(),
                output: target.job.clone(),
                resolved_output_path: output_path_str,
            });
        }
    }

    if jobs.is_empty() {
        return Ok(());
    }

    manager.add_jobs(jobs).await;

    let mut state = manager.state.lock().await;
    if state.is_running {
        return Ok(());
    }
    state.is_running = true;
    state.cancellation_token = tokio_util::sync::CancellationToken::new();

    // Emit batch progress at start
    let _ = app.emit(
        "batch://progress",
        BatchProgress {
            total_jobs: state.total_jobs,
            completed_jobs: state.completed_jobs,
            failed_jobs: state.failed_jobs,
            percentage: 0.0,
            current_job_id: None,
        },
    );

    let state_clone = Arc::clone(&manager.state);
    let app_clone = app.clone();

    tokio::spawn(async move {
        let mut subtitle_cache: HashMap<String, PathBuf> = HashMap::new();

        loop {
            let (job, token) = {
                let mut s = state_clone.lock().await;
                if s.cancellation_token.is_cancelled() {
                    s.is_running = false;
                    break;
                }
                let job = s.queue.pop_front();
                s.current_job_id = job.as_ref().map(|j| j.output.id.clone());
                (job, s.cancellation_token.clone())
            };

            let job = match job {
                Some(j) => j,
                None => {
                    let mut s = state_clone.lock().await;
                    s.is_running = false;
                    s.current_job_id = None;
                    break;
                }
            };

            // Emit batch progress
            emit_batch_progress(&app_clone, &state_clone).await;

            let job_id = job.output.id.clone();
            let input_path = job.input_path.clone();
            let ratio = job.output.ratio.clone();

            // Emit Processing status
            let _ = app_clone.emit(
                "batch://file-status",
                FileProgress {
                    job_id: job_id.clone(),
                    file_path: input_path.clone(),
                    ratio: ratio.clone(),
                    progress: 0.0,
                    status: JobStatus::Processing,
                },
            );

            // RESOLUTION PHASE: Combine Spec + Input into ResolvedJob
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
                            let _ = app_clone.emit(
                                "batch://file-status",
                                FileProgress {
                                    job_id: job_id.clone(),
                                    file_path: input_path.clone(),
                                    ratio: ratio.clone(),
                                    progress: 0.0,
                                    status: JobStatus::Failed(failure),
                                },
                            );

                            {
                                let mut s = state_clone.lock().await;
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
                s.is_running = false;
                break;
            }

            // RENDER PHASE: Consume ResolvedJob
            let result = render_single(
                &app_clone,
                resolved_job,
                Some(token.clone()),
            )
            .await;

            match result {
                Ok(_) => {
                    let _ = app_clone.emit(
                        "batch://file-status",
                        FileProgress {
                            job_id: job_id.clone(),
                            file_path: input_path.clone(),
                            ratio: ratio.clone(),
                            progress: 100.0,
                            status: JobStatus::Completed,
                        },
                    );
                    {
                        let mut s = state_clone.lock().await;
                        s.completed_jobs += 1;
                    }
                }
                Err(e) => {
                    let _ = app_clone.emit(
                        "batch://file-status",
                        FileProgress {
                            job_id: job_id.clone(),
                            file_path: input_path.clone(),
                            ratio: ratio.clone(),
                            progress: 0.0,
                            status: JobStatus::Failed(e.to_string()),
                        },
                    );
                    {
                        let mut s = state_clone.lock().await;
                        s.failed_jobs += 1;
                    }
                }
            }

            emit_batch_progress(&app_clone, &state_clone).await;
        }

        // Emit final progress
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
    let percentage = if total > 0 {
        ((completed + failed) as f32 / total as f32) * 100.0
    } else {
        0.0
    };

    Ok(BatchProgress {
        total_jobs: total,
        completed_jobs: completed,
        failed_jobs: failed,
        percentage,
        current_job_id: state.current_job_id.clone(),
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
    let percentage = if total > 0 {
        ((completed + failed) as f32 / total as f32) * 100.0
    } else {
        0.0
    };

    let _ = app.emit(
        "batch://progress",
        BatchProgress {
            total_jobs: total,
            completed_jobs: completed,
            failed_jobs: failed,
            percentage,
            current_job_id: state.current_job_id.clone(),
        },
    );
}
