use crate::video::types::BatchJob;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct BatchState {
    pub queue: VecDeque<BatchJob>,
    pub current_job_id: Option<String>,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub total_jobs: usize,
    pub cancellation_token: CancellationToken,
    pub is_running: bool,
}

pub struct BatchManager {
    pub state: Arc<Mutex<BatchState>>,
}

impl BatchManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BatchState {
                queue: VecDeque::new(),
                current_job_id: None,
                completed_jobs: 0,
                failed_jobs: 0,
                total_jobs: 0,
                cancellation_token: CancellationToken::new(),
                is_running: false,
            })),
        }
    }

    pub async fn add_jobs(&self, jobs: Vec<BatchJob>) {
        let mut state = self.state.lock().await;
        state.total_jobs += jobs.len();
        for job in jobs {
            state.queue.push_back(job);
        }
    }

    pub async fn clear(&self) {
        let mut state = self.state.lock().await;
        state.queue.clear();
        state.current_job_id = None;
        state.completed_jobs = 0;
        state.failed_jobs = 0;
        state.total_jobs = 0;
        state.is_running = false;
        state.cancellation_token = CancellationToken::new();
    }

    pub async fn cancel(&self) {
        let token = {
            let mut state = self.state.lock().await;
            state.is_running = false;
            state.cancellation_token.clone()
        }; // lock released here
        token.cancel();
    }
}
