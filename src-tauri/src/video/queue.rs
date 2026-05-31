use crate::video::types::{BatchJob, BatchStatus, FileProgress};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct BatchState {
    pub session_id: Option<String>,
    pub queue: VecDeque<BatchJob>,
    pub job_progress: HashMap<String, FileProgress>,
    pub all_job_ids: Vec<String>,
    pub current_job_id: Option<String>,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub total_jobs: usize,
    pub cancellation_token: CancellationToken,
    pub status: BatchStatus,
    // Telemetry fields
    pub start_time: Option<std::time::Instant>,
    pub total_duration_secs: f64,
    pub processed_duration_secs: f64,
    pub current_stage_id: Option<String>,
    pub current_stage_message: Option<String>,
    pub current_job_lifecycle_progress: f32,
}

pub struct BatchManager {
    pub state: Arc<Mutex<BatchState>>,
}

impl BatchManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BatchState {
                session_id: None,
                queue: VecDeque::new(),
                job_progress: HashMap::new(),
                all_job_ids: Vec::new(),
                current_job_id: None,
                completed_jobs: 0,
                failed_jobs: 0,
                total_jobs: 0,
                cancellation_token: CancellationToken::new(),
                status: BatchStatus::Idle,
                start_time: None,
                total_duration_secs: 0.0,
                processed_duration_secs: 0.0,
                current_stage_id: None,
                current_stage_message: None,
                current_job_lifecycle_progress: 0.0,
            })),
        }
    }

    pub async fn add_jobs(&self, jobs: Vec<BatchJob>, initial_progress: Vec<FileProgress>) {
        let mut state = self.state.lock().await;
        state.total_jobs += jobs.len();
        for job in jobs {
            state.all_job_ids.push(job.id.clone());
            state.queue.push_back(job);
        }
        for progress in initial_progress {
            state.total_duration_secs += progress.duration_secs;
            state.job_progress.insert(progress.job_id.clone(), progress);
        }
    }

    pub async fn clear(&self) {
        let mut state = self.state.lock().await;
        // If a worker is still active, force cancellation before resetting state
        // so old tasks cannot keep mutating a "new" cleared session.
        state.cancellation_token.cancel();
        state.queue.clear();
        state.session_id = None;
        state.job_progress.clear();
        state.all_job_ids.clear();
        state.current_job_id = None;
        state.completed_jobs = 0;
        state.failed_jobs = 0;
        state.total_jobs = 0;
        state.status = BatchStatus::Idle;
        state.cancellation_token = CancellationToken::new();
        state.start_time = None;
        state.total_duration_secs = 0.0;
        state.processed_duration_secs = 0.0;
        state.current_stage_id = None;
        state.current_stage_message = None;
        state.current_job_lifecycle_progress = 0.0;
    }

    pub async fn cancel(&self) {
        let token = {
            let mut state = self.state.lock().await;
            state.status = BatchStatus::Cancelled;
            // Mark all non-terminal jobs as cancelled in the progress map
            for progress in state.job_progress.values_mut() {
                match progress.status {
                    crate::video::types::JobStatus::Queued
                    | crate::video::types::JobStatus::Processing
                    | crate::video::types::JobStatus::Pending => {
                        progress.status = crate::video::types::JobStatus::Cancelled;
                    }
                    _ => {}
                }
            }
            state.cancellation_token.clone()
        }; // lock released here
        token.cancel();
    }
}
