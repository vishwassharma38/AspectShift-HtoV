use crate::video::convert::{convert_to_ratio, prepare_subtitles};
use crate::video::queue::{BatchManager, BatchState};
use crate::video::types::{BatchJob, BatchJobSettings, BatchProgress, FileProgress, JobStatus};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

pub async fn start_batch(
    app: AppHandle,
    manager: State<'_, BatchManager>,
    files: Vec<String>,
    settings: BatchJobSettings,
) -> Result<(), String> {
    let mut jobs = Vec::new();
    for file in files {
        for target in &settings.targets {
            let input_path = Path::new(&file);
            let stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("video");
            let ext = target.options.output_format.get_extension();
            
            let tag = if let Some(name) = &target.preset_name {
                name.to_lowercase().replace(' ', "_").replace('/', "_")
            } else {
                target.ratio.get_tag().replace(':', "x")
            };

            let output_name = format!("{}_{}.{}", stem, tag, ext);
            let output_path = Path::new(&settings.output_dir)
                .join(output_name)
                .to_string_lossy()
                .to_string();

            jobs.push(BatchJob {
                id: uuid::Uuid::new_v4().to_string(),
                input_path: file.clone(),
                target_ratio: target.ratio.clone(),
                target_preset: target.options.preset.clone(),
                active_effects: target.options.clone(),
                platform_config: target.platform_config.clone(),
                resolved_output_path: output_path,
            });
        }
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
                s.current_job_id = job.as_ref().map(|j| j.id.clone());
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

            let job_id = job.id.clone();
            let input_path = job.input_path.clone();
            let ratio = job.target_ratio.clone();

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

            let should_prepare_subtitles =
                job.active_effects.generate_subtitles || job.active_effects.burn_subtitles;
            
            let mut prepared_subtitle = None;
            if should_prepare_subtitles {
                if let Some(path) = subtitle_cache.get(&input_path) {
                    prepared_subtitle = Some(path.clone());
                } else {
                    // Get output dir from resolved path
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

            if token.is_cancelled() {
                let mut s = state_clone.lock().await;
                s.is_running = false;
                break;
            }

            let result = convert_to_ratio(
                &app_clone,
                job_id.clone(),
                input_path.clone(),
                Path::new(&job.resolved_output_path).parent().unwrap().to_string_lossy().to_string(),
                ratio.clone(),
                job.active_effects.clone(),
                job.platform_config.clone(),
                prepared_subtitle,
                Some(token.clone()),
            ).await;

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
