use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use crate::video::types::{
    BatchJob, BatchJobSettings, BatchProgress, FileProgress, JobStatus
};
use crate::video::convert::convert_to_ratio;
use crate::video::queue::{BatchManager, BatchState};

pub async fn start_batch(
    app: AppHandle,
    manager: State<'_, BatchManager>,
    files: Vec<String>,
    settings: BatchJobSettings
) -> Result<(), String> {
    let mut jobs = Vec::new();
    for file in files {
        jobs.push(BatchJob {
            id: uuid::Uuid::new_v4().to_string(),
            file_path: file,
            settings: settings.clone(),
        });
    }

    manager.add_jobs(jobs).await;
    
    let mut state = manager.state.lock().await;
    if state.is_running {
        return Ok(());
    }
    state.is_running = true;
    state.cancellation_token = tokio_util::sync::CancellationToken::new();
    
    // Fix 4: Emit batch progress at start
    let _ = app.emit("batch://progress", BatchProgress {
        total_jobs: state.total_jobs,
        completed_jobs: state.completed_jobs,
        failed_jobs: state.failed_jobs,
        percentage: 0.0,
        current_job_id: None,
    });

    let state_clone = Arc::clone(&manager.state);
    let app_clone = app.clone();

    tokio::spawn(async move {
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
            let file_path = job.file_path.clone();
            
            let mut job_failed = false;

            // Process each ratio for the file
            for ratio in &job.settings.ratios {
                if token.is_cancelled() {
                    break;
                }

                // Fix 1: Emit Processing for this ratio
                let _ = app_clone.emit("batch://file-status", FileProgress {
                    job_id: job_id.clone(),
                    file_path: file_path.clone(),
                    ratio: ratio.clone(),
                    progress: 0.0,
                    status: JobStatus::Processing,
                });

                let app_handle = app_clone.clone();
                let job_id_inner = job_id.clone();
                let file_path_inner = file_path.clone();
                let ratio_inner = ratio.clone();
                let options_inner = job.settings.options.clone();
                let platform_config_inner = job.settings.platform_config.clone();
                let output_dir_inner = job.settings.output_dir.clone();
                let token_inner = token.clone();

                // Fix 3: Per-ratio error isolation
                let result = tokio::task::spawn_blocking(move || {
                    convert_to_ratio(
                        &app_handle,
                        job_id_inner,
                        file_path_inner,
                        output_dir_inner,
                        ratio_inner,
                        options_inner,
                        platform_config_inner,
                        Some(token_inner)
                    )
                }).await;

                match result {
                    Ok(Ok(_)) => {
                        let _ = app_clone.emit("batch://file-status", FileProgress {
                            job_id: job_id.clone(),
                            file_path: file_path.clone(),
                            ratio: ratio.clone(),
                            progress: 100.0,
                            status: JobStatus::Completed,
                        });
                    },
                    Ok(Err(e)) => {
                        job_failed = true;
                        let _ = app_clone.emit("batch://file-status", FileProgress {
                            job_id: job_id.clone(),
                            file_path: file_path.clone(),
                            ratio: ratio.clone(),
                            progress: 0.0,
                            status: JobStatus::Failed(e.to_string()),
                        });
                    },
                    Err(e) => {
                        job_failed = true;
                        let _ = app_clone.emit("batch://file-status", FileProgress {
                            job_id: job_id.clone(),
                            file_path: file_path.clone(),
                            ratio: ratio.clone(),
                            progress: 0.0,
                            status: JobStatus::Failed(format!("Task panicked: {}", e)),
                        });
                    }
                }
            }

            if token.is_cancelled() {
                let mut s = state_clone.lock().await;
                s.is_running = false;
                break;
            }

            // Update completed/failed count
            {
                let mut s = state_clone.lock().await;
                if job_failed {
                    s.failed_jobs += 1;
                } else {
                    s.completed_jobs += 1;
                }
            }
            
            emit_batch_progress(&app_clone, &state_clone).await;
        }
        
        // Fix 4: Emit batch progress at end
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

    let _ = app.emit("batch://progress", BatchProgress {
        total_jobs: total,
        completed_jobs: completed,
        failed_jobs: failed,
        percentage,
        current_job_id: state.current_job_id.clone(),
    });
}
