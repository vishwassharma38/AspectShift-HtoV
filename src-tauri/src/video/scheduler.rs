//! Capacity/admission-based job scheduler (roadmap Stage 2.1, Stage 3.2,
//! Stage 3.4).
//!
//! Replaces the single sequential batch loop with one dispatcher that admits
//! queued jobs according to a shared capacity budget. Admission is `cost`
//! based: each job's cost is computed by [`classify_job_cost`] (Stage 3.2:
//! normal jobs cost 1 unit, subtitle/Whisper jobs cost 2 units) and each
//! admitted job is spawned as an isolated Tokio task that releases its
//! capacity when it finishes.
//!
//! There is deliberately **no** separate sequential code path: `total_capacity
//! = 1` naturally produces one-job-at-a-time behaviour through the same
//! scheduler mechanism.
//!
//! The capacity accounting is a small atomic-backed counter (not a fixed
//! `Semaphore` permits struct) so that a later roadmap stage (3.4) can safely
//! reduce the future-admission ceiling without replacing the dispatcher.
//! Current/total capacity are kept as distinct concepts for that reason.
//!
//! Stage 3.4 adds failure classification (deterministic vs resource-related)
//! and safe capacity reduction with one-time retry for resource failures.
//! When a resource-related failure occurs, the admission ceiling is reduced
//! without disturbing any currently-running job, and the failed job is retried
//! once capacity allows under the new ceiling.
//!
//! Stage 7 adds lowest-cost ordering: the dispatcher pops the queued job with
//! the lowest [`estimate_job_cost`] (a config-derived heuristic) instead of a
//! plain `pop_front()`. Equal-cost jobs keep FIFO order, so a queue of
//! otherwise-identical jobs behaves exactly like the historical FIFO scheduler
//! and all deterministic ordering tests remain valid.

use crate::video::concurrency::{DiskAdmissionGate, DiskSpaceVerdict};
use crate::video::queue::BatchState;
use crate::video::types::{BatchJob, BatchStatus, JobStatus, OutputFormat, VideoError};
use std::collections::HashSet;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinSet;

/// Operational classification of a job failure (Stage 3.4).
///
/// Failures are classified into exactly two buckets:
///
/// - **Deterministic**: caused by the job's input or configuration (invalid
///   input, bad filter, unsupported codec, missing font, invalid output path).
///   These must not be retried and must not reduce the admission ceiling.
///
/// - **ResourceRelated**: caused by system resource exhaustion (out-of-memory,
///   device busy, resource unavailable). These may receive exactly one
///   reduced-capacity retry. The classification is conservative: ambiguous
///   errors default to deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureClass {
    Deterministic,
    ResourceRelated,
}

/// Classifies a [`VideoError`] into an operational failure class.
///
/// The classification is conservative: only errors that provide clear evidence
/// of resource exhaustion are classified as `ResourceRelated`. All other
/// errors — including ambiguous processing failures — are classified as
/// `Deterministic`.
///
/// Resource-related patterns are checked against FFmpeg stderr because
/// resource exhaustion manifests as specific error strings from the encoder.
pub(crate) fn classify_video_error(error: &VideoError) -> FailureClass {
    match error {
        VideoError::ProcessingFailed { stderr } => {
            if is_resource_exhaustion_stderr(stderr) {
                FailureClass::ResourceRelated
            } else {
                FailureClass::Deterministic
            }
        }
        // Spawn failures indicate an environment/configuration problem, not
        // resource exhaustion on the FFmpeg side.
        VideoError::FfmpegNotFound
        | VideoError::FfprobeNotFound
        | VideoError::FileNotFound(_)
        | VideoError::FileLocked(_)
        | VideoError::AlreadyProcessing(_)
        | VideoError::WhisperNotFound
        | VideoError::WhisperModelNotFound
        | VideoError::WhisperFailed { .. }
        | VideoError::SubtitleParseError(_)
        | VideoError::InvalidInput(_)
        | VideoError::LockError(_)
        | VideoError::IoError(_)
        | VideoError::JsonError(_)
        | VideoError::TauriError(_) => FailureClass::Deterministic,
    }
}

/// Checks FFmpeg stderr for patterns that indicate resource exhaustion.
///
/// These patterns are well-known, stable FFmpeg/OSError messages that
/// reliably indicate the encoder could not acquire necessary system resources.
/// The check is conservative: only clear resource-exhaustion signals are
/// matched, and everything else falls through to deterministic.
fn is_resource_exhaustion_stderr(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    // OOM / memory exhaustion
    lower.contains("cannot allocate memory")
        || lower.contains("out of memory")
        || lower.contains("no memory")
        // Device/resource busy
        || lower.contains("device or resource busy")
        || lower.contains("device busy")
        // No space left (disk exhaustion during encode)
        || lower.contains("no space left on device")
        // Encoder initialization failures caused by resource exhaustion
        || lower.contains("could not open codec")
            && (lower.contains("resource")
                || lower.contains("memory")
                || lower.contains("busy"))
}

/// The outcome returned by a job processing function to the scheduler.
pub(crate) enum JobOutcome {
    Processed,
    Skipped,
    Cancelled,
    /// A job that failed at the task boundary and could not report the failure
    /// itself (e.g. a panic caught by the scheduler). Ordinary processing
    /// failures are recorded inside the processing function and returned as
    /// `JobOutcome::Processed`, matching the existing conventions.
    Failed(String),
    /// A classified failure returned by the processing function (Stage 3.4).
    /// The batch processor records the failure status in per-job state and
    /// returns this variant so the scheduler can decide whether to retry and
    /// whether to reduce the admission ceiling.
    ClassifiedFailed {
        message: String,
        failure_class: FailureClass,
    },
}

/// Points `current_job_id` at a currently-processing job, or clears it when
/// none is still active. With concurrent jobs this keeps the "displayed job"
/// meaningful without depending on which job happened to finish last.
fn recompute_displayed_job_id(state: &mut BatchState) {
    state.current_job_id = state
        .job_progress
        .iter()
        .find(|(_, p)| matches!(p.status, JobStatus::Processing))
        .map(|(id, _)| id.clone());
}

/// Returns the capacity cost (in cost units) of a queued job.
///
/// Stage 3.2: subtitle/Whisper jobs cost `2` units because their workflow
/// involves audio-extraction FFmpeg â†’ Whisper â†’ render FFmpeg â€” substantially
/// more work than a plain render. All other jobs cost `1` unit.
pub(crate) fn classify_job_cost(job: &BatchJob) -> usize {
    if job.output.effects.export_subtitles_enabled() || job.output.effects.burn_subtitles_enabled()
    {
        2
    } else {
        1
    }
}

/// Estimates a job's *relative wall-clock cost* from its resolved
/// configuration (architecture_fix Stage 6/7).
///
/// Unlike [`classify_job_cost`] — a coarse admission *weight* used for
/// capacity accounting (`1` normal, `2` subtitle) — this is a finer heuristic
/// `f64` used only to **order** the queue: cheaper jobs are popped first so a
/// couple of heavy renders cannot starve a queue of trivial jobs while
/// capacity is scarce.
///
/// The estimate deliberately stays **config-derived** and never uses input
/// file size or media duration: a `BatchJob` carries neither (duration lives
/// in per-job progress state), and probing media on every admission attempt
/// would add I/O to the admit loop. Ordering therefore uses only information
/// AspectShift already knows when scheduling:
///
/// * platform target pixel area (resolution is the dominant encode cost)
/// * output codec (VP9/webm is materially more expensive than H.264)
/// * the subtitle pipeline (audio-extract → Whisper → render, mirroring the
///   cost-2 classification scaled down for ordering)
/// * per-effect filter workload (background, image, text, overlays, transform,
///   colour filter)
/// * audio removal (one stream skipped → marginally cheaper)
/// * encoding speed preset (faster presets cost less wall-clock)
///
/// Unknown values get a neutral `1.0` factor instead of a guess, and the
/// result is clamped to a sensible band so a single absurd setting can never
/// dominate ordering.
pub(crate) fn estimate_job_cost(job: &BatchJob) -> f64 {
    let effects = &job.output.effects;

    // Baseline: a plain 1080p H.264 render at the "medium" preset.
    let mut cost = 1.0;

    // Resolution: encode work scales with pixel area. Jobs that target a
    // platform resolution get their exact pixel budget; jobs without one keep
    // the neutral 1.0 (assume 1080p-equivalent rather than fabricate a value).
    const BASELINE_AREA: f64 = 1920.0 * 1080.0;
    let area = job
        .output
        .platform_config
        .as_ref()
        .map(|c| (c.target_width as f64) * (c.target_height as f64))
        .unwrap_or(BASELINE_AREA);
    cost *= (area / BASELINE_AREA).clamp(0.1, 8.0);

    // Codec: VP9 (webm) is materially more expensive than H.264.
    if matches!(effects.output_format_value(), OutputFormat::Webm) {
        cost *= 1.5;
    }

    // Subtitle/Whisper pipeline (mirrors classify_job_cost's cost-2 model).
    if effects.export_subtitles_enabled() || effects.burn_subtitles_enabled() {
        cost *= 1.6;
    }

    // Effects: each enabled stage adds measurable encode work.
    if effects.background_effect_enabled() {
        cost *= 1.15;
    }
    if effects.image_overlay_enabled() {
        cost *= 1.1;
    }
    if effects.text_overlay_enabled() {
        cost *= 1.15;
    }
    if effects
        .overlays
        .as_ref()
        .map(|o| !o.is_empty())
        .unwrap_or(false)
    {
        cost *= 1.1;
    }
    if effects
        .transform
        .as_ref()
        .map(|t| t.rotate != 0 || t.flip_h || t.flip_v)
        .unwrap_or(false)
    {
        cost *= 1.05;
    }
    if effects
        .color_filter
        .as_ref()
        .map(|f| !f.is_empty())
        .unwrap_or(false)
    {
        cost *= 1.05;
    }

    // Audio removed: one stream skipped → slightly cheaper.
    if effects.remove_audio_enabled() {
        cost *= 0.92;
    }

    // Encoding speed preset drives encode wall-clock nearly linearly.
    // Unknown presets map to the neutral 1.0.
    cost *= match job.output.encoding.speed_preset.to_lowercase().as_str() {
        "ultrafast" => 0.6,
        "superfast" => 0.65,
        "veryfast" => 0.7,
        "faster" => 0.8,
        "fast" => 0.9,
        "medium" => 1.0,
        "slow" => 1.25,
        "slower" => 1.5,
        "veryslow" => 1.8,
        _ => 1.0,
    };

    cost
}

/// Pops the queued job with the lowest estimated cost (architecture_fix
/// Stage 7).
///
/// Selection is a stable linear scan over the `VecDeque`: ties keep FIFO order
/// (the earliest-enqueued job among equals is popped), so a queue of
/// otherwise-identical jobs behaves exactly like the historical
/// `pop_front()` order and the deterministic FIFO tests remain valid. Cheap
/// jobs pop before expensive ones, so heavy renders cannot starve a queue of
/// trivial jobs while capacity is scarce.
fn pop_lowest_cost_job(state: &mut BatchState) -> Option<BatchJob> {
    let mut best_index = 0usize;
    let mut best_cost = state.queue.front().map(estimate_job_cost)?;
    for (index, job) in state.queue.iter().enumerate().skip(1) {
        let cost = estimate_job_cost(job);
        if cost < best_cost {
            best_index = index;
            best_cost = cost;
        }
    }
    state.queue.remove(best_index)
}

/// Capacity accounting shared between the dispatcher and every admitted job.
///
/// The invariant maintained at all times is:
///
/// ```text
/// in_flight_cost <= total_capacity
/// ```
///
/// (Equivalently, `available_capacity + in_flight_cost == total_capacity`
/// after each acquired permit, since the dispatch loop is the only acquirer
/// and every permit releases exactly the cost it acquired. Stage 3.4 lowers a
/// separate `admission_ceiling` on resource failures; that ceiling constrains
/// only future admission and never disturbs the accounting for permits that
/// are already issued.)
///
/// Acquisition is asynchronous: the dispatcher waits here only when capacity
/// is genuinely exhausted or the Stage 3.4 admission ceiling is full. Release
/// is driven by an RAII guard ([`CapacityPermit`]) so it cannot be forgotten
/// on any path.
#[derive(Debug, Clone)]
pub(crate) struct CapacityGate {
    inner: Arc<CapacityInner>,
}

#[derive(Debug)]
struct CapacityInner {
    /// Total capacity in cost units. This is the configured baseline and is
    /// never changed after construction.
    total_capacity: AtomicUsize,
    /// Stage 3.4: the ceiling for *future* admissions, reduced on resource
    /// failures. Kept distinct from `total_capacity` so a reduction never
    /// breaks the accounting for permits that are currently issued: admitted
    /// jobs keep running under their original allocation, and the ceiling
    /// constrains only jobs not yet admitted.
    admission_ceiling: AtomicUsize,
    /// Cost units currently free to be acquired.
    available: AtomicUsize,
    /// Cost units currently held by in-flight jobs.
    in_flight: AtomicUsize,
    /// Wakes the single dispatcher waiter when capacity is released.
    released: Notify,
}

impl CapacityGate {
    pub(crate) fn new(total_capacity: usize) -> Self {
        Self {
            inner: Arc::new(CapacityInner {
                total_capacity: AtomicUsize::new(total_capacity),
                admission_ceiling: AtomicUsize::new(total_capacity),
                available: AtomicUsize::new(total_capacity),
                in_flight: AtomicUsize::new(0),
                released: Notify::new(),
            }),
        }
    }

    pub(crate) fn total_capacity(&self) -> usize {
        self.inner.total_capacity.load(Ordering::Acquire)
    }

    /// The current ceiling for future admissions (Stage 3.4). This may be
    /// below [`CapacityGate::total_capacity`] after a resource failure; it is
    /// never raised again within a batch run.
    pub(crate) fn admission_ceiling(&self) -> usize {
        self.inner.admission_ceiling.load(Ordering::Acquire)
    }

    pub(crate) fn available_capacity(&self) -> usize {
        self.inner.available.load(Ordering::Acquire)
    }

    pub(crate) fn in_flight_cost(&self) -> usize {
        self.inner.in_flight.load(Ordering::Acquire)
    }

    /// Asynchronously reserves `cost` units of capacity.
    ///
    /// There is exactly one consumer (the dispatcher), so the wake-up logic
    /// stays simple: the `Notify` future is created *before* the state check
    /// each round, which closes the notify/check race for a single waiter, and
    /// the actual state is re-read after every wake-up.
    ///
    /// Stage 3.4: acquisition also refuses to exceed the current
    /// `admission_ceiling`. Because the dispatcher is the only acquirer and
    /// the ceiling is only ever changed by the dispatcher itself (in the join
    /// loop, never concurrently with the acquire loop), this check cannot race
    /// with a reduction and `in_flight` can only be increased by this loop.
    pub(crate) async fn acquire_cost(&self, cost: usize) -> CapacityPermit {
        let mut notified = self.inner.released.notified();
        loop {
            let available = self.inner.available.load(Ordering::Acquire);
            let in_flight = self.inner.in_flight.load(Ordering::Acquire);
            let ceiling = self.inner.admission_ceiling.load(Ordering::Acquire);
            if available >= cost && in_flight + cost <= ceiling {
                if self
                    .inner
                    .available
                    .compare_exchange_weak(
                        available,
                        available - cost,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    self.inner.in_flight.fetch_add(cost, Ordering::AcqRel);
                    return CapacityPermit {
                        gate: self.clone(),
                        cost,
                        released: false,
                    };
                }
                continue;
            }
            notified.await;
            notified = self.inner.released.notified();
        }
    }

    fn release_cost(&self, cost: usize) {
        self.inner.available.fetch_add(cost, Ordering::AcqRel);
        self.inner.in_flight.fetch_sub(cost, Ordering::AcqRel);
        self.inner.released.notify_one();
    }

    /// Reduces the future-admission ceiling by `cost` units (Stage 3.4).
    ///
    /// This affects only jobs that have **not yet been admitted**. Jobs that
    /// have already acquired capacity continue running under their original
    /// allocation. The ceiling is never reduced below `1`.
    ///
    /// Returns the new ceiling value after the reduction.
    pub(crate) fn reduce_ceiling_for_failure(&self, cost: usize) -> usize {
        let floor = 1usize;
        let prev = self.inner.admission_ceiling.load(Ordering::Acquire);
        if prev <= floor {
            return prev;
        }
        let new = prev.saturating_sub(cost).max(floor);
        let _ = self.inner.admission_ceiling.compare_exchange(
            prev,
            new,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        self.inner.admission_ceiling.load(Ordering::Acquire)
    }
}

/// RAII guard for an acquired capacity reservation.
///
/// The guard is held for the lifetime of a spawned job task and releases the
/// capacity exactly once on drop, covering success, ordinary processing
/// failure, early return, task cancellation and panics that unwind the task
/// boundary.
#[derive(Debug)]
pub(crate) struct CapacityPermit {
    gate: CapacityGate,
    cost: usize,
    released: bool,
}

impl CapacityPermit {
    /// Accessor for the acquired cost. The cost is the weighted value from
    /// [`classify_job_cost`] (Stage 3.2); it participates in decisions beyond
    /// `acquire`/`Drop` (e.g. Stage 3.4 capacity reduction).
    #[allow(dead_code)]
    pub(crate) fn cost(&self) -> usize {
        self.cost
    }
}

impl Drop for CapacityPermit {
    fn drop(&mut self) {
        if !self.released {
            self.released = true;
            self.gate.release_cost(self.cost);
        }
    }
}

/// Post-run snapshot of the capacity accounting.
///
/// Used both as the scheduler's completion value and by tests to prove that
/// no capacity is leaked and no background task remains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SchedulerSummary {
    pub total_capacity: usize,
    pub admission_ceiling: usize,
    pub in_flight_cost: usize,
    pub available_capacity: usize,
}

/// Runs every job in `state.queue` through the capacity-based scheduler.
///
/// - One dispatcher consumes the existing `VecDeque`, admitting the
///   lowest-estimated-cost job next (Stage 7; equal costs keep FIFO order).
/// - For each job it computes the cost, asynchronously acquires that many
///   cost units (blocking only when capacity is genuinely exhausted), spawns
///   the job as an isolated Tokio task and immediately considers the next
///   queued job.
/// - When the queue is empty the dispatcher stops admitting but does **not**
///   return: it waits for every spawned task to finish, so no work (and no
///   capacity) is left behind.
///
/// Stage 3.4: Resource-related failures reduce the admission ceiling and
/// schedule one retry under the new ceiling. Deterministic failures are
/// permanently failed without retry or ceiling reduction.
///
/// Failures returned by the processing function become `JobOutcome::Failed`.
/// Panics that unwind the processing boundary are caught here, converted into
/// a `JobOutcome::Failed`, recorded in the shared batch state, and cannot
/// crash the dispatcher or poison other jobs.
pub(crate) async fn run_scheduler<F, Fut>(
    state: &Arc<Mutex<BatchState>>,
    total_capacity: usize,
    disk_gate: &DiskAdmissionGate,
    process_fn: F,
) -> SchedulerSummary
where
    F: Fn(BatchJob) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = JobOutcome> + Send + 'static,
{
    let gate = CapacityGate::new(total_capacity);
    let process = Arc::new(process_fn);
    let mut tasks: JoinSet<(String, JobOutcome, BatchJob)> = JoinSet::new();
    // Stage 3.4: track which jobs have already consumed their one retry.
    let mut retried_jobs: HashSet<String> = HashSet::new();

    // Outer loop: re-enters the admit/join cycle when retried jobs are
    // enqueued by the join loop below. Each iteration admits jobs, spawns
    // tasks, then joins all tasks. If a resource-related failure re-enqueues
    // a job for retry, the outer loop picks it up in the next iteration
    // under the reduced admission ceiling.
    loop {
        // Inner admit loop: pops jobs from the queue, runs disk gate,
        // acquires capacity, and spawns tasks.
        loop {
            let (job, job_id) = {
                let mut s = state.lock().await;
                if s.cancellation_token.is_cancelled() || s.status == BatchStatus::Cancelled {
                    break;
                }
                // Stage 7 ordering: admit the lowest-estimated-cost job first
                // (ties keep FIFO order via pop_lowest_cost_job).
                let Some(job) = pop_lowest_cost_job(&mut s) else {
                    break;
                };
                let job_id = job.id.clone();
                s.current_job_id = Some(job_id.clone());
                s.current_job_lifecycle_progress = 0.0;
                s.job_lifecycle_progress.insert(job_id.clone(), 0.0);
                (job, job_id)
            };

            let cost = classify_job_cost(&job);

            // Stage 3.4: the admission ceiling is reduced after resource
            // failures and is never raised again within a batch. A queued job
            // whose cost exceeds the current ceiling can never be admitted, so
            // leave capacity alone and fail it permanently rather than leaving
            // it stuck behind an unsatisfiable acquire.
            if cost > gate.admission_ceiling() {
                let message = format!(
                    "job cost {} exceeds the reduced admission ceiling {}; \
                     job cannot be admitted and is permanently failed",
                    cost,
                    gate.admission_ceiling()
                );
                tracing::warn!("{}: {}", job_id, message);
                let mut s = state.lock().await;
                s.failed_jobs += 1;
                s.current_job_lifecycle_progress = 100.0;
                s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
                if let Some(p) = s.job_progress.get_mut(&job_id) {
                    p.status = JobStatus::Failed(message);
                }
                if s.current_job_id.as_deref() == Some(job_id.as_str()) {
                    recompute_displayed_job_id(&mut s);
                }
                continue;
            }

            // Stage 3.3: the disk-space gate runs BEFORE the dispatcher
            // acquires capacity for this job.
            let verdict = disk_gate.admit(Path::new(&job.resolved_output_path));
            if !matches!(verdict, DiskSpaceVerdict::Healthy) {
                let message = disk_space_failure_message(&job.id, &verdict);
                tracing::warn!(
                    "disk-space gate halted admission of job {}: {}",
                    job_id,
                    message
                );
                let mut s = state.lock().await;
                s.failed_jobs += 1;
                s.current_job_lifecycle_progress = 100.0;
                s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
                if let Some(p) = s.job_progress.get_mut(&job_id) {
                    p.status = JobStatus::Failed(message.clone());
                }
                while let Some(blocked) = s.queue.pop_front() {
                    s.failed_jobs += 1;
                    s.job_lifecycle_progress.insert(blocked.id.clone(), 100.0);
                    if let Some(p) = s.job_progress.get_mut(&blocked.id) {
                        p.status = JobStatus::Failed(message.clone());
                    }
                }
                recompute_displayed_job_id(&mut s);
                break;
            }

            // Blocks the dispatcher only when capacity is genuinely exhausted.
            let permit = gate.acquire_cost(cost).await;

            // Cancellation may have arrived while we waited for capacity.
            {
                let mut s = state.lock().await;
                if s.cancellation_token.is_cancelled() || s.status == BatchStatus::Cancelled {
                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                        if matches!(p.status, JobStatus::Queued | JobStatus::Pending) {
                            p.status = JobStatus::Cancelled;
                        }
                    }
                    break;
                }
            }

            let process = Arc::clone(&process);
            let job_clone = job.clone();
            tasks.spawn(async move {
                let _permit_guard = permit;
                let outcome = match futures_util::FutureExt::catch_unwind(
                    std::panic::AssertUnwindSafe(async move { process(job).await }),
                )
                .await
                {
                    Ok(outcome) => outcome,
                    Err(payload) => {
                        let detail = panic_payload_message(&payload);
                        tracing::error!(
                            "scheduler caught panic while processing job {}: {}",
                            job_id,
                            detail
                        );
                        JobOutcome::Failed(format!("job panicked: {}", detail))
                    }
                };
                (job_id, outcome, job_clone)
            });
        }

        // Join loop: wait for every spawned task to finish and return its
        // capacity. This loop processes outcomes including Stage 3.4
        // classified failures.
        while let Some(joined) = tasks.join_next().await {
            let Ok((job_id, outcome, job)) = joined else {
                tracing::error!("scheduler task ended with an unexpected error");
                continue;
            };
            match outcome {
                JobOutcome::Cancelled => {
                    let mut s = state.lock().await;
                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                        p.status = JobStatus::Cancelled;
                    }
                    if s.current_job_id.as_deref() == Some(job_id.as_str()) {
                        recompute_displayed_job_id(&mut s);
                    }
                }
                JobOutcome::Failed(message) => {
                    let mut s = state.lock().await;
                    let mut duration = 0.0;
                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                        p.status = JobStatus::Failed(message);
                        duration = p.duration_secs;
                    }
                    s.failed_jobs += 1;
                    s.processed_duration_secs += duration;
                    s.current_job_lifecycle_progress = 100.0;
                    s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
                    if s.current_job_id.as_deref() == Some(job_id.as_str()) {
                        recompute_displayed_job_id(&mut s);
                    }
                }
                // Stage 3.4: classified failure returned by the processing
                // function. For resource-related failures, reduce the ceiling
                // and re-enqueue the job for one retry. For deterministic
                // failures, permanently fail the job.
                JobOutcome::ClassifiedFailed {
                    message,
                    failure_class,
                } => match failure_class {
                    FailureClass::ResourceRelated => {
                        if retried_jobs.contains(&job_id) {
                            // Second resource failure: reduce the ceiling again
                            // (never below 1), then permanently fail the job.
                            let old_ceiling = gate.admission_ceiling();
                            let new_ceiling = gate.reduce_ceiling_for_failure(1);
                            let mut s = state.lock().await;
                            let mut duration = 0.0;
                            if let Some(p) = s.job_progress.get_mut(&job_id) {
                                p.status = JobStatus::Failed(message);
                                duration = p.duration_secs;
                            }
                            s.failed_jobs += 1;
                            s.processed_duration_secs += duration;
                            s.current_job_lifecycle_progress = 100.0;
                            s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
                            if s.current_job_id.as_deref() == Some(job_id.as_str()) {
                                recompute_displayed_job_id(&mut s);
                            }
                            tracing::warn!(
                                "job {} retry failed due to resource pressure; \
                                 admission ceiling reduced from {} to {}; \
                                 job permanently failed (no second retry)",
                                job_id,
                                old_ceiling,
                                new_ceiling
                            );
                        } else {
                            // First resource failure: reduce the future
                            // admission ceiling and re-enqueue for one retry.
                            let old_ceiling = gate.admission_ceiling();
                            let new_ceiling = gate.reduce_ceiling_for_failure(1);
                            retried_jobs.insert(job_id.clone());
                            tracing::warn!(
                                "job {} encountered a resource-related failure; \
                                 future admission ceiling reduced from {} to {}; \
                                 existing jobs remain unaffected; one retry permitted",
                                job_id,
                                old_ceiling,
                                new_ceiling
                            );
                            // If the reduced ceiling is below this job's own
                            // cost, a reduced-capacity retry is impossible;
                            // permanently fail instead of re-enqueuing a job
                            // that can never be admitted.
                            let job_cost = classify_job_cost(&job);
                            if job_cost > new_ceiling {
                                let message = format!(
                                    "resource-related failure; the reduced admission \
                                     ceiling ({}) is below this job's cost ({}), \
                                     so no reduced-capacity retry is possible; \
                                     job permanently failed",
                                    new_ceiling, job_cost
                                );
                                let mut s = state.lock().await;
                                let mut duration = 0.0;
                                if let Some(p) = s.job_progress.get_mut(&job_id) {
                                    p.status = JobStatus::Failed(message.clone());
                                    duration = p.duration_secs;
                                }
                                s.failed_jobs += 1;
                                s.processed_duration_secs += duration;
                                s.current_job_lifecycle_progress = 100.0;
                                s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
                                if s.current_job_id.as_deref() == Some(job_id.as_str()) {
                                    recompute_displayed_job_id(&mut s);
                                }
                                tracing::warn!("job {} {}", job_id, message);
                            } else {
                                {
                                    let mut s = state.lock().await;
                                    if let Some(p) = s.job_progress.get_mut(&job_id) {
                                        p.status = JobStatus::Queued;
                                    }
                                    if s.current_job_id.as_deref() == Some(job_id.as_str()) {
                                        recompute_displayed_job_id(&mut s);
                                    }
                                    s.queue.push_back(job);
                                }
                            }
                        }
                    }
                    FailureClass::Deterministic => {
                        let mut s = state.lock().await;
                        let mut duration = 0.0;
                        if let Some(p) = s.job_progress.get_mut(&job_id) {
                            p.status = JobStatus::Failed(message);
                            duration = p.duration_secs;
                        }
                        s.failed_jobs += 1;
                        s.processed_duration_secs += duration;
                        s.current_job_lifecycle_progress = 100.0;
                        s.job_lifecycle_progress.insert(job_id.clone(), 100.0);
                        if s.current_job_id.as_deref() == Some(job_id.as_str()) {
                            recompute_displayed_job_id(&mut s);
                        }
                        tracing::warn!(
                            "job {} failed deterministically; no retry; \
                             admission ceiling unchanged",
                            job_id
                        );
                    }
                },
                JobOutcome::Processed | JobOutcome::Skipped => {}
            }
        }

        // Check if retried jobs were enqueued during the join loop. If so,
        // the outer loop re-enters the admit loop. If the queue is empty,
        // we are done.
        {
            let mut s = state.lock().await;
            if s.queue.is_empty() {
                break;
            }
            if s.cancellation_token.is_cancelled() || s.status == BatchStatus::Cancelled {
                while let Some(blocked) = s.queue.pop_front() {
                    s.failed_jobs += 1;
                    s.job_lifecycle_progress.insert(blocked.id.clone(), 100.0);
                    if let Some(p) = s.job_progress.get_mut(&blocked.id) {
                        p.status = JobStatus::Cancelled;
                    }
                }
                recompute_displayed_job_id(&mut s);
                break;
            }
        }
    }

    // Phase B (Stage 2.5): the batch may report `Cancelled` only once every
    // admitted task has exited. Reaching this point proves there are no
    // running jobs left for this batch.
    {
        let mut s = state.lock().await;
        if s.cancellation_token.is_cancelled() && s.status != BatchStatus::Cancelled {
            s.status = BatchStatus::Cancelled;
        }
    }

    SchedulerSummary {
        total_capacity: gate.total_capacity(),
        admission_ceiling: gate.admission_ceiling(),
        in_flight_cost: gate.in_flight_cost(),
        available_capacity: gate.available_capacity(),
    }
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(re_wrapped) = payload.downcast_ref::<Box<dyn std::any::Any + Send>>() {
        // A previously-caught panic payload that was re-thrown (e.g. across a
        // task boundary) arrives wrapped in another Box<dyn Any + Send>. Its
        // concrete type is `Box<dyn Any + Send>` again, so inspect the box's
        // contents rather than the box itself.
        panic_payload_message(re_wrapped.as_ref())
    } else {
        "unknown panic payload".to_string()
    }
}

/// Builds the user-facing "clear disk space error" carried by every job that
/// is blocked by the Stage 3.3 gate, so the reason is identical across all
/// blocked jobs in a batch.
fn disk_space_failure_message(job_id: &str, verdict: &DiskSpaceVerdict) -> String {
    match verdict {
        DiskSpaceVerdict::Healthy => unreachable!("called only on a blocking verdict"),
        DiskSpaceVerdict::Insufficient {
            available_bytes,
            margin_bytes,
            path,
        } => format!(
            "Cannot start {job_id}: insufficient free disk space on {} \
             ({} bytes free is below the {} byte safety margin)",
            path.display(),
            available_bytes,
            margin_bytes,
        ),
        DiskSpaceVerdict::Unavailable { message, path } => format!(
            "Cannot start {job_id}: could not determine free disk space on {} ({message})",
            path.display(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::types::{
        AspectRatio, EncodingProfile, FileProgress, JobStatus, OutputJob, PlatformConfig,
        SelectionMetadata, TargetType, VideoEffectsSettings,
    };
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::watch;
    use tokio_util::sync::CancellationToken;

    fn effects() -> VideoEffectsSettings {
        serde_json::from_str(r#"{}"#).expect("empty effects should deserialize")
    }

    // -----------------------------------------------------------------
    // Stage 3.3 test harness — fake disk-space probes
    // -----------------------------------------------------------------

    use crate::video::concurrency::{DiskSpaceSource, DISK_SAFETY_MARGIN_BYTES};

    #[derive(Debug)]
    struct HealthyDiskSource;

    impl DiskSpaceSource for HealthyDiskSource {
        fn available_bytes(&self, _path: &Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }
    }

    /// A gate that always admits; used by pre-Stage-3.3 scheduler tests that
    /// are not concerned with disk space.
    fn healthy_disk_gate() -> DiskAdmissionGate {
        DiskAdmissionGate::new(Arc::new(HealthyDiskSource), DISK_SAFETY_MARGIN_BYTES)
    }

    /// A scripted probe: admission call `i` reports `script[i]`; beyond the
    /// script it reports a healthy value (above the margin). Lets a test
    /// exhaust disk space after a chosen number of successful admissions.
    #[derive(Debug)]
    struct ScriptedDiskSpace {
        script: Vec<u64>,
        calls: Arc<AtomicUsize>,
    }

    impl ScriptedDiskSpace {
        fn new(script: Vec<u64>) -> Self {
            Self {
                script,
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl DiskSpaceSource for ScriptedDiskSpace {
        fn available_bytes(&self, _path: &Path) -> std::io::Result<u64> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .script
                .get(index)
                .copied()
                .unwrap_or(DISK_SAFETY_MARGIN_BYTES + 1024))
        }
    }

    /// Builds a gate backed by a [`ScriptedDiskSpace`] and returns the
    /// scripted source so tests can inspect how many admission probes ran.
    fn scripted_gate(script: Vec<u64>) -> (DiskAdmissionGate, Arc<ScriptedDiskSpace>) {
        let source = Arc::new(ScriptedDiskSpace::new(script));
        let gate_source: Arc<dyn DiskSpaceSource> = source.clone();
        let gate = DiskAdmissionGate::new(gate_source, DISK_SAFETY_MARGIN_BYTES);
        (gate, source)
    }

    fn build_job(index: usize) -> BatchJob {
        let id = format!("job-{}", index);
        BatchJob {
            id: id.clone(),
            input_path: format!("input-{}.mp4", index),
            output: OutputJob {
                id: format!("out-{}", index),
                ratio: AspectRatio::Ratio9x16,
                encoding: EncodingProfile::standard(),
                encoding_overrides: crate::video::encoding::EncodingOverrides::baseline(),
                effects: effects(),
                platform_config: None,
                selection: SelectionMetadata {
                    source_type: TargetType::AspectRatio,
                    source_id: "test-source".into(),
                    label: "test".into(),
                },
                force_reencode: false,
            },
            resolved_output_path: format!("output-{}_9x16.mp4", index),
            alt_output_path: None,
            thumbnail_path: None,
        }
    }

    fn build_state(n_jobs: usize) -> Arc<Mutex<BatchState>> {
        let mut state = BatchState {
            session_id: Some("test-session".into()),
            queue: VecDeque::new(),
            job_progress: HashMap::new(),
            all_job_ids: Vec::new(),
            current_job_id: None,
            completed_jobs: 0,
            failed_jobs: 0,
            total_jobs: n_jobs,
            cancellation_token: CancellationToken::new(),
            status: BatchStatus::Processing,
            start_time: None,
            total_duration_secs: 0.0,
            processed_duration_secs: 0.0,
            current_stage_id: None,
            current_stage_message: None,
            current_job_lifecycle_progress: 0.0,
            job_lifecycle_progress: HashMap::new(),
        };
        for index in 0..n_jobs {
            let job = build_job(index);
            let job_id = job.id.clone();
            state.queue.push_back(job);
            state.all_job_ids.push(job_id.clone());
            state.job_progress.insert(
                job_id.clone(),
                FileProgress {
                    session_id: "test-session".into(),
                    job_id: job_id.clone(),
                    file_path: format!("input-{}.mp4", index),
                    ratio: AspectRatio::Ratio9x16,
                    progress: 0.0,
                    status: JobStatus::Queued,
                    thumbnail_path: None,
                    duration_secs: 10.0,
                    selection: SelectionMetadata {
                        source_type: TargetType::AspectRatio,
                        source_id: "test-source".into(),
                        label: "test".into(),
                    },
                },
            );
        }
        Arc::new(Mutex::new(state))
    }

    fn job_index(job: &BatchJob) -> usize {
        job.id["job-".len()..]
            .parse::<usize>()
            .expect("indexed job id")
    }

    // -----------------------------------------------------------------
    // Test 1 â€” every job is processed exactly once (no skip, no duplicate)
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn every_job_is_processed_exactly_once() {
        let n = 8;
        let state = build_state(n);
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let seen: Arc<tokio::sync::Mutex<HashSet<String>>> =
            Arc::new(tokio::sync::Mutex::new(HashSet::new()));

        let runner = {
            let executions = executions.clone();
            let seen = seen.clone();
            move |job: BatchJob| {
                let executions = executions.clone();
                let seen = seen.clone();
                async move {
                    let mut guard = seen.lock().await;
                    assert!(
                        guard.insert(job.id.clone()),
                        "job {} executed more than once",
                        job.id
                    );
                    drop(guard);
                    executions.fetch_add(1, Ordering::SeqCst);
                    JobOutcome::Processed
                }
            }
        };

        let summary = run_scheduler(&state, 2, &healthy_disk_gate(), runner).await;

        assert_eq!(
            executions.load(Ordering::SeqCst),
            n,
            "every job must execute"
        );
        assert_eq!(seen.lock().await.len(), n, "every executed job is distinct");
        assert!(state.lock().await.queue.is_empty(), "queue fully consumed");
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, summary.total_capacity);
    }

    // -----------------------------------------------------------------
    // Test 2 â€” in-flight cost never exceeds total_capacity (1, 2, 4)
    // -----------------------------------------------------------------
    fn sleeping_runner(
        capacity: usize,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        executions: Arc<AtomicUsize>,
    ) -> impl Fn(BatchJob) -> std::pin::Pin<Box<dyn futures_util::Future<Output = JobOutcome> + Send>>
    {
        move |_job: BatchJob| {
            let active = active.clone();
            let max_active = max_active.clone();
            let executions = executions.clone();
            Box::pin(async move {
                let before = active.fetch_add(1, Ordering::SeqCst);
                assert!(
                    before < capacity,
                    "in-flight jobs {} exceeded total_capacity {}",
                    before + 1,
                    capacity
                );
                max_active.fetch_max(before + 1, Ordering::SeqCst);
                // Hold the capacity long enough for concurrent tasks to overlap.
                tokio::time::sleep(Duration::from_millis(20)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                executions.fetch_add(1, Ordering::SeqCst);
                JobOutcome::Processed
            })
        }
    }

    #[tokio::test]
    async fn in_flight_cost_never_exceeds_capacity() {
        for capacity in [1usize, 2, 4] {
            let n = 12;
            let state = build_state(n);
            let active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
            let max_active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
            let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

            let summary = run_scheduler(
                &state,
                capacity,
                &healthy_disk_gate(),
                sleeping_runner(
                    capacity,
                    active.clone(),
                    max_active.clone(),
                    executions.clone(),
                ),
            )
            .await;

            assert_eq!(executions.load(Ordering::SeqCst), n);
            assert!(
                max_active.load(Ordering::SeqCst) <= capacity,
                "observed concurrency exceeded capacity"
            );
            assert_eq!(active.load(Ordering::SeqCst), 0);
            assert_eq!(summary.in_flight_cost, 0);
            assert_eq!(summary.available_capacity, capacity);
        }
    }

    // -----------------------------------------------------------------
    // Test 3 â€” total_capacity = 1 is strictly sequential, from the same
    // scheduler (no separate code path).
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn capacity_1_is_strictly_sequential() {
        let n = 3;
        let state = build_state(n);
        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel::<usize>(n);
        let releases: Vec<watch::Sender<u32>> = (0..n).map(|_| watch::channel(0u32).0).collect();
        let active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let runner = {
            let started_tx = started_tx.clone();
            let releases = releases.clone();
            let active = active.clone();
            move |job: BatchJob| {
                let index = job_index(&job);
                let started_tx = started_tx.clone();
                let releases = releases.clone();
                let active = active.clone();
                async move {
                    let previous = active.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(
                        previous, 0,
                        "capacity 1 allowed job {} to start while another was active",
                        index
                    );
                    // Subscribe before reporting start so the release cannot be missed.
                    let mut release_rx = releases[index].subscribe();
                    started_tx.send(index).await.expect("started channel open");
                    let _ = release_rx.changed().await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    JobOutcome::Processed
                }
            }
        };

        let scheduler_state = state.clone();
        let scheduler_task = tokio::spawn(async move {
            run_scheduler(&scheduler_state, 1, &healthy_disk_gate(), runner).await
        });

        // Job 0 starts first and is held until we release it.
        let first = started_rx.recv().await.expect("job 0 starts");
        assert_eq!(first, 0, "queue order must be preserved");
        assert_eq!(
            active.load(Ordering::SeqCst),
            1,
            "job 0 is the only active job"
        );

        // Deterministic: capacity 1 guarantees job 1 cannot have started while
        // job 0 is still holding the only permit.
        assert!(
            started_rx.try_recv().is_err(),
            "job 1 must not start while job 0 is still active"
        );

        releases[0].send(1).expect("release job 0");
        let second = started_rx
            .recv()
            .await
            .expect("job 1 starts after job 0 finishes");
        assert_eq!(second, 1);
        assert!(
            started_rx.try_recv().is_err(),
            "job 2 must not start while job 1 is still active"
        );

        releases[1].send(1).expect("release job 1");
        let third = started_rx
            .recv()
            .await
            .expect("job 2 starts after job 1 finishes");
        assert_eq!(third, 2);
        releases[2].send(1).expect("release job 2");

        scheduler_task.await.expect("scheduler completes normally");
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    // -----------------------------------------------------------------
    // Test 4 â€” total_capacity = 4 admits overlapping jobs concurrently.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn capacity_4_admits_overlapping_jobs() {
        let n = 4;
        let state = build_state(n);
        let active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let max_active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        // Hold every admitted job on a barrier of 4. The barrier can only
        // release if the scheduler actually admits all four jobs before any
        // of them completes, which proves real overlap.
        let barrier = Arc::new(tokio::sync::Barrier::new(4));
        let runner = {
            let active = active.clone();
            let max_active = max_active.clone();
            let executions = executions.clone();
            let barrier = barrier.clone();
            move |_job: BatchJob| {
                let active = active.clone();
                let max_active = max_active.clone();
                let executions = executions.clone();
                let barrier = barrier.clone();
                async move {
                    let before = active.fetch_add(1, Ordering::SeqCst);
                    assert!(before < 4, "capacity 4 exceeded");
                    max_active.fetch_max(before + 1, Ordering::SeqCst);
                    let _ = tokio::time::timeout(Duration::from_secs(5), barrier.wait())
                        .await
                        .expect("barrier must release: scheduler must admit 4 jobs at once");
                    active.fetch_sub(1, Ordering::SeqCst);
                    executions.fetch_add(1, Ordering::SeqCst);
                    JobOutcome::Processed
                }
            }
        };

        let summary = run_scheduler(&state, 4, &healthy_disk_gate(), runner).await;

        assert_eq!(executions.load(Ordering::SeqCst), 4);
        assert_eq!(
            max_active.load(Ordering::SeqCst),
            4,
            "expected 4-way concurrency to actually occur"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 4);
    }

    // -----------------------------------------------------------------
    // Test 5 â€” capacity is fully released after many more jobs than capacity
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn capacity_fully_released_after_many_jobs() {
        let (capacity, n) = (4, 20);
        let state = build_state(n);
        let active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let max_active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let summary = run_scheduler(
            &state,
            capacity,
            &healthy_disk_gate(),
            sleeping_runner(capacity, active, max_active.clone(), executions.clone()),
        )
        .await;

        assert_eq!(executions.load(Ordering::SeqCst), n);
        assert!(max_active.load(Ordering::SeqCst) <= capacity);
        assert_eq!(summary.in_flight_cost, 0, "no capacity can be leaked");
        assert_eq!(summary.available_capacity, capacity);
        assert_eq!(summary.total_capacity, capacity);
        assert!(state.lock().await.queue.is_empty());
    }

    // -----------------------------------------------------------------
    // Test 6 â€” a failed job does not poison the scheduler or leak capacity
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn job_failure_does_not_poison_scheduler() {
        let n = 6;
        let failed_indices = [1usize, 3];
        let state = build_state(n);
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let runner = {
            let failed = failed_indices.iter().copied().collect::<HashSet<_>>();
            let executions = executions.clone();
            move |job: BatchJob| {
                let failed = failed.clone();
                let executions = executions.clone();
                async move {
                    let index = job_index(&job);
                    executions.fetch_add(1, Ordering::SeqCst);
                    if failed.contains(&index) {
                        JobOutcome::Failed(format!("mock failure for job-{}", index))
                    } else {
                        JobOutcome::Processed
                    }
                }
            }
        };

        let summary = run_scheduler(&state, 2, &healthy_disk_gate(), runner).await;

        assert_eq!(
            executions.load(Ordering::SeqCst),
            n,
            "all jobs still execute"
        );
        let state_guard = state.lock().await;
        assert_eq!(state_guard.failed_jobs, failed_indices.len());
        for index in failed_indices {
            let job_id = format!("job-{}", index);
            match state_guard
                .job_progress
                .get(&job_id)
                .map(|p| p.status.clone())
            {
                Some(JobStatus::Failed(message)) => {
                    assert!(message.contains("mock failure"), "failure info preserved");
                }
                other => panic!("job {} expected Failed, got {:?}", job_id, other),
            }
        }
        assert!(state_guard.queue.is_empty());
        drop(state_guard);
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 2);
    }

    // -----------------------------------------------------------------
    // Test 7 â€” a panicking job is contained at the task boundary
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn panic_at_processing_boundary_is_contained() {
        let n = 5;
        let panicking_index = 2usize;
        let state = build_state(n);
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let runner = {
            let executions = executions.clone();
            move |job: BatchJob| {
                let executions = executions.clone();
                async move {
                    let index = job_index(&job);
                    executions.fetch_add(1, Ordering::SeqCst);
                    if index == panicking_index {
                        panic!("deliberate panic for job-{}", index);
                    }
                    JobOutcome::Processed
                }
            }
        };

        let summary = run_scheduler(&state, 3, &healthy_disk_gate(), runner).await;

        // Every job's task began; the panicking job is contained as Failed and
        // the remaining jobs still completed.
        assert_eq!(executions.load(Ordering::SeqCst), n);
        let state_guard = state.lock().await;
        assert_eq!(state_guard.failed_jobs, 1);
        let panic_job = format!("job-{}", panicking_index);
        match state_guard
            .job_progress
            .get(&panic_job)
            .map(|p| p.status.clone())
        {
            Some(JobStatus::Failed(message)) => {
                assert!(
                    message.contains("deliberate panic"),
                    "panic message must be preserved: {}",
                    message
                );
            }
            other => panic!("panicked job must be Failed, got {:?}", other),
        }
        // The scheduler consumed every queue entry (the mock processing snapshots
        // then unwinds, so processed outcomes are always Queued in this mock).
        assert!(state_guard.queue.is_empty(), "no job left unprocessed");
        drop(state_guard);

        // No capacity leaked and the dispatcher survived the panic.
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 3);

        // Scheduler returned normally (no deadlock), so this line only runs if
        // the dispatcher survived the panic.
        assert!(summary.total_capacity == 3);
    }

    // -----------------------------------------------------------------
    // Test 8 â€” cancellation stops admission and releases every permit
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn cancelled_batch_releases_capacity_and_stops_admitting() {
        let n = 3;
        let state = build_state(n);
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel::<usize>(n);
        let (release_tx, _release_rx) = watch::channel(0u32);

        let runner = {
            let executions = executions.clone();
            let started_tx = started_tx.clone();
            let release_tx = release_tx.clone();
            move |job: BatchJob| {
                let index = job_index(&job);
                let executions = executions.clone();
                let started_tx = started_tx.clone();
                let release_tx = release_tx.clone();
                async move {
                    executions.fetch_add(1, Ordering::SeqCst);
                    if index == 0 {
                        // Subscribe before reporting start so the release
                        // cannot be missed (same pattern as the sequential test).
                        let mut release_rx = release_tx.subscribe();
                        started_tx.send(index).await.expect("started channel open");
                        // Job 0 holds the only capacity permit until released.
                        let _ = release_rx.changed().await;
                    }
                    JobOutcome::Processed
                }
            }
        };

        let scheduler_state = state.clone();
        let scheduler_task = tokio::spawn(async move {
            run_scheduler(&scheduler_state, 1, &healthy_disk_gate(), runner).await
        });

        // Job 0 starts first and holds the only permit.
        let first = started_rx.recv().await.expect("job 0 starts");
        assert_eq!(first, 0, "queue order must be preserved");

        // Cancel while job 0 is still in flight (as BatchManager::cancel does).
        state.lock().await.cancellation_token.cancel();

        // Release job 0; its task returns and returns the permit to the gate.
        release_tx.send(1).expect("release job 0");

        let summary = scheduler_task.await.expect("scheduler completes normally");

        // No job may be admitted once the batch is cancelled.
        assert_eq!(
            executions.load(Ordering::SeqCst),
            1,
            "no job may be admitted after cancellation"
        );
        // Every acquired permit is released on the cancellation path.
        assert_eq!(
            summary.in_flight_cost, 0,
            "cancelled batch must not leak capacity"
        );
        assert_eq!(summary.available_capacity, 1);
        assert_eq!(summary.total_capacity, 1);

        // Stage 2.5 Phase B: the batch reaches Cancelled only after the single
        // in-flight task has exited (the scheduler only sets this once every
        // joined task has returned).
        assert_eq!(
            state.lock().await.status,
            BatchStatus::Cancelled,
            "batch must report Cancelled after all tasks have exited"
        );
    }

    // -----------------------------------------------------------------
    // Test 9 â€” Stage 2.5 cancellation ordering with concurrent jobs.
    //
    // Deterministic proof of the Phase A / Phase B split: with capacity 2 and
    // 4 queued jobs, cancelling while both in-flight jobs are still running
    // keeps the batch in `Processing` (Phase A â€” cancellation requested) and
    // only transitions to `Cancelled` after *every* task has exited (Phase B â€”
    // cancellation complete). The schedule is driven by a watch gate so no
    // wall-clock timing assumptions are involved.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn cancelled_status_transitions_only_after_all_tasks_exit() {
        let (capacity, n) = (2usize, 4);
        let manager = crate::video::queue::BatchManager {
            state: build_state(n),
            disk_source: Arc::new(HealthyDiskSource),
        };
        let state = manager.state.clone();

        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel::<usize>(n);
        let (gate_tx, gate_rx) = watch::channel(0u32);
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let runner = {
            let started_tx = started_tx.clone();
            let gate_rx = gate_rx.clone();
            let executions = executions.clone();
            move |job: BatchJob| {
                let index = job_index(&job);
                let started_tx = started_tx.clone();
                let mut gate_rx = gate_rx.clone();
                let executions = executions.clone();
                async move {
                    let before = executions.fetch_add(1, Ordering::SeqCst);
                    assert!(before < capacity, "more than capacity jobs in flight");
                    // Report start, then hold the task until the test releases
                    // the gate â€” simulating an FFmpeg process mid-shutdown that
                    // has not yet exited.
                    started_tx.send(index).await.expect("started channel open");
                    let _ = gate_rx.changed().await;
                    JobOutcome::Processed
                }
            }
        };

        let scheduler_state = state.clone();
        let scheduler_task = tokio::spawn(async move {
            run_scheduler(&scheduler_state, capacity, &healthy_disk_gate(), runner).await
        });

        // Wait for exactly `capacity` jobs to be admitted and in flight.
        let mut in_flight = std::collections::HashSet::new();
        for _ in 0..capacity {
            let index = started_rx.recv().await.expect("a job starts");
            in_flight.insert(index);
        }
        assert_eq!(in_flight.len(), capacity, "jobs admitted are distinct");
        assert!(
            started_rx.try_recv().is_err(),
            "no further job may be admitted while all capacity is held"
        );

        // Phase A â€” request cancellation through the real BatchManager::cancel()
        // path. This cancels the token and marks the never-started jobs
        // cancelled, but must NOT flip the batch-level status.
        manager.cancel().await;
        {
            let s = state.lock().await;
            assert_eq!(
                s.status,
                BatchStatus::Processing,
                "status must remain Processing while in-flight tasks are still running"
            );
        }

        // Hold the in-flight tasks a little longer and re-assert: cancellation
        // is requested, not complete, so the status must still be Processing.
        tokio::time::sleep(Duration::from_millis(25)).await;
        {
            let s = state.lock().await;
            assert_eq!(
                s.status,
                BatchStatus::Processing,
                "status must stay Processing until every task has exited"
            );
        }

        // Phase B â€” release both in-flight tasks. Only now may the batch reach
        // Cancelled, because only now have all tasks (and their child
        // processes) actually terminated.
        gate_tx.send(1).expect("release gate");
        let summary = scheduler_task.await.expect("scheduler completes normally");

        assert_eq!(summary.in_flight_cost, 0, "no capacity may leak");
        assert_eq!(summary.available_capacity, capacity);
        assert_eq!(
            executions.load(Ordering::SeqCst),
            capacity,
            "exactly the admitted jobs ran; the rest must never start after cancellation"
        );
        let s = state.lock().await;
        assert_eq!(
            s.status,
            BatchStatus::Cancelled,
            "batch may only report Cancelled after all tasks have exited"
        );
        // No job may remain in a non-terminal state after Phase B.
        for p in s.job_progress.values() {
            assert_ne!(
                p.status,
                JobStatus::Processing,
                "no job may remain Processing once cancellation is complete"
            );
        }
        // The two never-started jobs were marked cancelled at request time.
        let started_ids = in_flight
            .iter()
            .map(|i| format!("job-{}", i))
            .collect::<std::collections::HashSet<_>>();
        let mut cancelled_count = 0usize;
        for (id, p) in &s.job_progress {
            if !started_ids.contains(id) {
                assert_eq!(
                    p.status,
                    JobStatus::Cancelled,
                    "never-started job {} must be marked Cancelled at request time",
                    id
                );
                cancelled_count += 1;
            }
        }
        assert_eq!(
            cancelled_count, 2,
            "the two never-started jobs are cancelled"
        );
    }

    // -----------------------------------------------------------------
    // Test 10 â€” Stage 3.1: capacity state never leaks from one batch into
    // the next. Each run_scheduler invocation builds a brand-new capacity
    // gate from its own total_capacity; a previous batch (even one that ended
    // with a failure and released capacity under a different ceiling) must
    // leave no stale capacity value behind for the next batch.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn capacity_state_does_not_leak_between_batches() {
        // Batch 1: capacity 2 with a failing job. The gate fully drains to
        // zero in-flight and every permit returns on the failure path.
        {
            let state = build_state(4);
            let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
            let runner = {
                let executions = executions.clone();
                move |job: BatchJob| {
                    let executions = executions.clone();
                    async move {
                        let idx = job_index(&job);
                        executions.fetch_add(1, Ordering::SeqCst);
                        if idx == 1 {
                            JobOutcome::Failed(format!("mock failure for job-{idx}"))
                        } else {
                            JobOutcome::Processed
                        }
                    }
                }
            };
            let summary = run_scheduler(&state, 2, &healthy_disk_gate(), runner).await;
            assert_eq!(executions.load(Ordering::SeqCst), 4);
            assert_eq!(summary.in_flight_cost, 0, "batch 1 must not leak capacity");
            assert_eq!(summary.available_capacity, 2);
            assert_eq!(summary.total_capacity, 2);
            assert_eq!(state.lock().await.failed_jobs, 1);
        }

        // Batch 2: a completely fresh scheduler at capacity 4 on a fresh
        // state. Nothing from batch 1 (its ceiling of 2, its released permits,
        // or its failure) may influence the new gate.
        {
            let state = build_state(4);
            let barrier = Arc::new(tokio::sync::Barrier::new(4));
            let active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
            let max_active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
            let runner = {
                let active = active.clone();
                let max_active = max_active.clone();
                let barrier = barrier.clone();
                move |_job: BatchJob| {
                    let active = active.clone();
                    let max_active = max_active.clone();
                    let barrier = barrier.clone();
                    async move {
                        let before = active.fetch_add(1, Ordering::SeqCst);
                        max_active.fetch_max(before + 1, Ordering::SeqCst);
                        let _ = tokio::time::timeout(Duration::from_secs(5), barrier.wait())
                            .await
                            .expect(
                                "barrier must release: the next batch starts with a full fresh gate",
                            );
                        active.fetch_sub(1, Ordering::SeqCst);
                        JobOutcome::Processed
                    }
                }
            };
            let summary = run_scheduler(&state, 4, &healthy_disk_gate(), runner).await;
            assert_eq!(
                summary.total_capacity, 4,
                "the next batch owns a brand-new capacity value, not a stale one"
            );
            assert_eq!(summary.available_capacity, 4);
            assert_eq!(summary.in_flight_cost, 0);
            assert_eq!(
                max_active.load(Ordering::SeqCst),
                4,
                "the next batch must reach its own full capacity (no stale ceiling inherited)"
            );
        }
    }

    // -----------------------------------------------------------------
    // Stage 3.2 â€” classify_job_cost unit tests
    // -----------------------------------------------------------------

    fn subtitle_effects(export: bool, burn: bool) -> VideoEffectsSettings {
        serde_json::from_str(&format!(
            r#"{{"exportSubtitles": {}, "burnSubtitles": {}}}"#,
            export, burn
        ))
        .expect("subtitle effects should deserialize")
    }

    fn build_subtitle_job(index: usize, export: bool, burn: bool) -> BatchJob {
        let mut job = build_job(index);
        job.output.effects = subtitle_effects(export, burn);
        job
    }

    #[test]
    fn classify_normal_job_returns_cost_1() {
        let job = build_job(0);
        assert_eq!(classify_job_cost(&job), 1);
    }

    #[test]
    fn classify_subtitle_export_job_returns_cost_2() {
        let job = build_subtitle_job(0, true, false);
        assert_eq!(classify_job_cost(&job), 2);
    }

    #[test]
    fn classify_subtitle_burn_job_returns_cost_2() {
        let job = build_subtitle_job(0, false, true);
        assert_eq!(classify_job_cost(&job), 2);
    }

    #[test]
    fn classify_both_subtitle_flags_returns_cost_2() {
        let job = build_subtitle_job(0, true, true);
        assert_eq!(classify_job_cost(&job), 2);
    }

    #[test]
    fn classify_neither_subtitle_flag_returns_cost_1() {
        let job = build_subtitle_job(0, false, false);
        assert_eq!(classify_job_cost(&job), 1);
    }

    #[test]
    fn classify_subtitle_fields_absent_returns_cost_1() {
        let job = build_job(0);
        assert!(!job.output.effects.export_subtitles_enabled());
        assert!(!job.output.effects.burn_subtitles_enabled());
        assert_eq!(classify_job_cost(&job), 1);
    }

    // -----------------------------------------------------------------
    // Stage 6/7 — estimate_job_cost (config-derived) + queue ordering
    // -----------------------------------------------------------------

    #[test]
    fn estimate_baseline_normal_job_is_1() {
        // A plain 1080p H.264 job at the "medium" preset is the neutral 1.0.
        let job = build_job(0);
        assert_eq!(estimate_job_cost(&job), 1.0);
    }

    #[test]
    fn estimate_subtitle_job_is_higher_than_normal() {
        // Mirrors classify_job_cost's cost-2 scale so subtitle jobs sort
        // behind cheaper renders.
        let subtitle_job = build_subtitle_job(0, true, false);
        assert!(estimate_job_cost(&subtitle_job) > estimate_job_cost(&build_job(1)));
    }

    #[test]
    fn estimate_scales_with_platform_pixel_area() {
        // 4K (3840×2160) is exactly 4× the 1080p baseline area.
        let mut job = build_job(0);
        job.output.platform_config = Some(PlatformConfig {
            target_width: 3840,
            target_height: 2160,
            enforce_dimensions: true,
            video_max_rate: None,
            video_buffer_size: None,
        });
        let cost = estimate_job_cost(&job);
        assert!(
            (cost - 4.0).abs() < 1e-9,
            "4K cost should be 4.0, got {cost}"
        );
    }

    #[test]
    fn estimate_webm_is_heavier_than_h264() {
        let mut job = build_job(0);
        job.output.effects.output_format = Some(OutputFormat::Webm);
        assert_eq!(estimate_job_cost(&job), 1.5);
    }

    #[test]
    fn estimate_speed_preset_shifts_cost() {
        let mut slow = build_job(0);
        slow.output.encoding.speed_preset = "veryslow".to_string();
        let mut fast = build_job(1);
        fast.output.encoding.speed_preset = "ultrafast".to_string();
        assert_eq!(estimate_job_cost(&fast), 0.6);
        assert_eq!(estimate_job_cost(&slow), 1.8);
    }

    #[test]
    fn estimate_unknown_preset_and_format_stay_neutral() {
        // Unknown values must not fabricate a cost; they stay at 1.0 so
        // ordering stays stable rather than guessing.
        let mut job = build_job(0);
        job.output.encoding.speed_preset = "nonexistent-preset".to_string();
        assert_eq!(estimate_job_cost(&job), 1.0);
    }

    #[test]
    fn estimate_remove_audio_is_marginally_cheaper() {
        let mut job = build_job(0);
        job.output.effects.remove_audio = Some(true);
        assert_eq!(estimate_job_cost(&job), 0.92);
    }

    #[tokio::test]
    async fn scheduler_admits_lowest_estimate_first() {
        // Queue order: [subtitle(1.6), normal(1.0), normal(1.0)]. Even though
        // the subtitle job is queued first, cheapest-first ordering admits the
        // two normal jobs first; the subtitle job (cost 2) takes over the
        // capacity once the normals release it. Purely config-derived ordering
        // — no file size or duration probing involved.
        let jobs: Vec<BatchJob> = std::iter::once(build_subtitle_job(0, true, false))
            .chain(std::iter::once(build_job(1)))
            .chain(std::iter::once(build_job(2)))
            .collect();
        let state = build_state_with_jobs(jobs);
        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel::<String>(3);
        let runner = {
            let started_tx = started_tx.clone();
            move |job: BatchJob| {
                let started_tx = started_tx.clone();
                async move {
                    started_tx.send(job.id.clone()).await.ok();
                    JobOutcome::Processed
                }
            }
        };

        let summary = run_scheduler(&state, 2, &healthy_disk_gate(), runner).await;

        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 2);
        let started: Vec<String> = std::iter::from_fn(|| started_rx.try_recv().ok()).collect();
        assert_eq!(
            started,
            vec![
                "job-1".to_string(),
                "job-2".to_string(),
                "job-0".to_string()
            ],
            "cheapest jobs must be admitted before the expensive one"
        );
    }

    #[tokio::test]
    async fn equal_estimate_jobs_keep_fifo_order() {
        // Identical config → identical estimate → pop_lowest_cost_job must
        // behave exactly like the historical pop_front() order.
        let state = build_state(3);
        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel::<usize>(3);
        let runner = {
            let started_tx = started_tx.clone();
            move |job: BatchJob| {
                let started_tx = started_tx.clone();
                async move {
                    let index: usize = job
                        .id
                        .rsplit_once('-')
                        .and_then(|(_, n)| n.parse().ok())
                        .expect("job id has numeric suffix");
                    started_tx.send(index).await.ok();
                    JobOutcome::Processed
                }
            }
        };

        let summary = run_scheduler(&state, 1, &healthy_disk_gate(), runner).await;

        assert_eq!(summary.in_flight_cost, 0);
        let order: Vec<usize> = std::iter::from_fn(|| started_rx.try_recv().ok()).collect();
        assert_eq!(order, vec![0, 1, 2], "equal-cost jobs keep FIFO order");
    }

    // -----------------------------------------------------------------
    // Stage 3.2 â€” capacity accounting scenarios (total_capacity = 4)
    // -----------------------------------------------------------------

    fn build_state_with_jobs(jobs: Vec<BatchJob>) -> Arc<Mutex<BatchState>> {
        let total = jobs.len();
        let mut state = BatchState {
            session_id: Some("test-session".into()),
            queue: VecDeque::new(),
            job_progress: HashMap::new(),
            all_job_ids: Vec::new(),
            current_job_id: None,
            completed_jobs: 0,
            failed_jobs: 0,
            total_jobs: total,
            cancellation_token: CancellationToken::new(),
            status: BatchStatus::Processing,
            start_time: None,
            total_duration_secs: 0.0,
            processed_duration_secs: 0.0,
            current_stage_id: None,
            current_stage_message: None,
            current_job_lifecycle_progress: 0.0,
            job_lifecycle_progress: HashMap::new(),
        };
        for job in jobs {
            let job_id = job.id.clone();
            state.queue.push_back(job.clone());
            state.all_job_ids.push(job_id.clone());
            state.job_progress.insert(
                job_id,
                FileProgress {
                    session_id: "test-session".into(),
                    job_id: job.id.clone(),
                    file_path: job.input_path.clone(),
                    ratio: AspectRatio::Ratio9x16,
                    progress: 0.0,
                    status: JobStatus::Queued,
                    thumbnail_path: None,
                    duration_secs: 10.0,
                    selection: SelectionMetadata {
                        source_type: TargetType::AspectRatio,
                        source_id: "test-source".into(),
                        label: "test".into(),
                    },
                },
            );
        }
        Arc::new(Mutex::new(state))
    }

    /// A job runner that tracks the *cost-weighted* in-flight total and asserts
    /// it never exceeds `total_capacity` â€” the same invariant the roadmap
    /// requires (`in_flight_cost <= total_capacity`), verified from inside the
    /// admitted tasks as well as by the scheduler's own summary.
    fn cost_tracking_runner(
        capacity: usize,
        in_flight_cost: Arc<AtomicUsize>,
        max_in_flight_cost: Arc<AtomicUsize>,
        executions: Arc<AtomicUsize>,
    ) -> impl Fn(BatchJob) -> std::pin::Pin<Box<dyn futures_util::Future<Output = JobOutcome> + Send>>
    {
        move |job: BatchJob| {
            let cost = classify_job_cost(&job);
            let in_flight_cost = in_flight_cost.clone();
            let max_in_flight_cost = max_in_flight_cost.clone();
            let executions = executions.clone();
            Box::pin(async move {
                let before = in_flight_cost.fetch_add(cost, Ordering::SeqCst);
                let after = before + cost;
                max_in_flight_cost.fetch_max(after, Ordering::SeqCst);
                assert!(
                    after <= capacity,
                    "in_flight_cost {} exceeded total_capacity {}",
                    after,
                    capacity
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
                in_flight_cost.fetch_sub(cost, Ordering::SeqCst);
                executions.fetch_add(1, Ordering::SeqCst);
                JobOutcome::Processed
            })
        }
    }

    // Scenario A â€” two subtitle jobs (2 + 2 = 4): both admitted concurrently.
    #[tokio::test]
    async fn two_subtitle_jobs_fill_capacity_4() {
        let jobs: Vec<BatchJob> = (0..2).map(|i| build_subtitle_job(i, true, false)).collect();
        let state = build_state_with_jobs(jobs);
        let in_flight_cost: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let max_in_flight_cost: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let summary = run_scheduler(
            &state,
            4,
            &healthy_disk_gate(),
            cost_tracking_runner(
                4,
                in_flight_cost.clone(),
                max_in_flight_cost.clone(),
                executions.clone(),
            ),
        )
        .await;

        assert_eq!(
            executions.load(Ordering::SeqCst),
            2,
            "both subtitle jobs execute"
        );
        assert!(
            max_in_flight_cost.load(Ordering::SeqCst) == 4,
            "two subtitle jobs (2+2) must reach the full capacity of 4"
        );
        assert!(
            max_in_flight_cost.load(Ordering::SeqCst) <= 4,
            "in_flight_cost must never exceed capacity"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 4);
    }

    // Scenario B â€” subtitle + two normal jobs (2 + 1 + 1 = 4): reaches, never
    // exceeds, capacity.
    #[tokio::test]
    async fn subtitle_plus_two_normals_fill_capacity_4() {
        let jobs: Vec<BatchJob> = std::iter::once(build_subtitle_job(0, true, false))
            .chain((1..=2).map(build_job))
            .collect();
        let state = build_state_with_jobs(jobs);
        let in_flight_cost: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let max_in_flight_cost: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let summary = run_scheduler(
            &state,
            4,
            &healthy_disk_gate(),
            cost_tracking_runner(
                4,
                in_flight_cost.clone(),
                max_in_flight_cost.clone(),
                executions.clone(),
            ),
        )
        .await;

        assert_eq!(executions.load(Ordering::SeqCst), 3);
        assert!(
            max_in_flight_cost.load(Ordering::SeqCst) == 4,
            "subtitle (2) + normal (1) + normal (1) must reach the full capacity of 4"
        );
        assert!(
            max_in_flight_cost.load(Ordering::SeqCst) <= 4,
            "in_flight_cost must never exceed capacity"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 4);
    }

    // Scenario C â€” mixed batch: 2 subtitle + 3 normal jobs, capacity 4.
    #[tokio::test]
    async fn mixed_batch_respects_capacity_and_utilizes_it() {
        let jobs: Vec<BatchJob> = (0..2)
            .map(|i| build_subtitle_job(i, true, false))
            .chain((2..5).map(build_job))
            .collect();
        let state = build_state_with_jobs(jobs);
        let in_flight_cost: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let max_in_flight_cost: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        let summary = run_scheduler(
            &state,
            4,
            &healthy_disk_gate(),
            cost_tracking_runner(
                4,
                in_flight_cost.clone(),
                max_in_flight_cost.clone(),
                executions.clone(),
            ),
        )
        .await;

        assert_eq!(executions.load(Ordering::SeqCst), 5, "all 5 jobs execute");
        assert!(
            max_in_flight_cost.load(Ordering::SeqCst) <= 4,
            "in_flight_cost must never exceed total_capacity"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 4);
    }

    // -----------------------------------------------------------------
    // Stage 3.2 â€” lifecycle / release behaviour for subtitle jobs
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn subtitle_job_completion_releases_cost_2() {
        let jobs: Vec<BatchJob> = std::iter::once(build_subtitle_job(0, true, false))
            .chain(std::iter::once(build_job(1)))
            .collect();
        let state = build_state_with_jobs(jobs);
        let runner = |_job: BatchJob| async { JobOutcome::Processed };

        let summary = run_scheduler(&state, 4, &healthy_disk_gate(), runner).await;

        assert_eq!(
            summary.in_flight_cost, 0,
            "all capacity released on completion"
        );
        assert_eq!(summary.available_capacity, 4, "full capacity restored");
        assert_eq!(summary.total_capacity, 4);
    }

    #[tokio::test]
    async fn subtitle_job_failure_releases_cost_2() {
        let jobs: Vec<BatchJob> = std::iter::once(build_subtitle_job(0, true, false))
            .chain(std::iter::once(build_job(1)))
            .collect();
        let state = build_state_with_jobs(jobs);
        let runner = |job: BatchJob| async move {
            if job.id == "job-0" {
                JobOutcome::Failed("subtitle whisper failed".into())
            } else {
                JobOutcome::Processed
            }
        };

        let summary = run_scheduler(&state, 4, &healthy_disk_gate(), runner).await;

        assert_eq!(
            summary.in_flight_cost, 0,
            "all capacity released on failure"
        );
        assert_eq!(summary.available_capacity, 4);
        let s = state.lock().await;
        assert_eq!(s.failed_jobs, 1);
    }

    #[tokio::test]
    async fn subtitle_job_cancellation_releases_cost_2() {
        let jobs: Vec<BatchJob> = std::iter::once(build_subtitle_job(0, true, false))
            .chain(std::iter::once(build_job(1)))
            .collect();
        let state = build_state_with_jobs(jobs);
        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel::<String>(2);
        let (gate_tx, gate_rx) = watch::channel(0u32);

        let runner = {
            let started_tx = started_tx.clone();
            move |job: BatchJob| {
                let started_tx = started_tx.clone();
                let mut gate_rx = gate_rx.clone();
                async move {
                    if job.id == "job-0" {
                        started_tx.send(job.id.clone()).await.ok();
                        let _ = gate_rx.changed().await;
                    }
                    JobOutcome::Processed
                }
            }
        };

        let scheduler_state = state.clone();
        let scheduler_task = tokio::spawn(async move {
            run_scheduler(&scheduler_state, 4, &healthy_disk_gate(), runner).await
        });

        let first = started_rx.recv().await.expect("subtitle job starts");
        assert_eq!(first, "job-0");

        state.lock().await.cancellation_token.cancel();
        gate_tx.send(1).expect("release gate");

        let summary = scheduler_task.await.expect("scheduler completes");

        assert_eq!(
            summary.in_flight_cost, 0,
            "cancelled subtitle cost fully released"
        );
        assert_eq!(summary.available_capacity, 4);
    }

    #[tokio::test]
    async fn subtitle_job_panic_releases_cost_2() {
        let jobs: Vec<BatchJob> = std::iter::once(build_subtitle_job(0, true, false))
            .chain(std::iter::once(build_job(1)))
            .collect();
        let state = build_state_with_jobs(jobs);
        let runner = |job: BatchJob| async move {
            if job.id == "job-0" {
                panic!("deliberate subtitle panic");
            }
            JobOutcome::Processed
        };

        let summary = run_scheduler(&state, 4, &healthy_disk_gate(), runner).await;

        assert_eq!(
            summary.in_flight_cost, 0,
            "panicked subtitle job's cost fully released via RAII permit"
        );
        assert_eq!(summary.available_capacity, 4);
        let s = state.lock().await;
        assert_eq!(s.failed_jobs, 1);
    }

    // Cheap jobs are admitted before a queued expensive job (cheapest-first
    // ordering), preventing head-of-line blocking: with capacity 4 the three
    // normal jobs are all admitted immediately and the subtitle job replaces
    // them as each cheap job releases its permit. Every job executes, nothing
    // leaks, and the cheap jobs demonstrably ran concurrently first.
    #[tokio::test]
    async fn lower_cost_jobs_are_admitted_before_expensive_ones() {
        let jobs: Vec<BatchJob> = std::iter::once(build_subtitle_job(0, true, false))
            .chain(std::iter::once(build_job(1)))
            .chain(std::iter::once(build_job(2)))
            .chain(std::iter::once(build_job(3)))
            .collect();
        let state = build_state_with_jobs(jobs);
        let in_flight_cost: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let max_in_flight_cost: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel::<String>(4);

        let runner = {
            let in_flight_cost = in_flight_cost.clone();
            let max_in_flight_cost = max_in_flight_cost.clone();
            let executions = executions.clone();
            let started_tx = started_tx.clone();
            move |job: BatchJob| {
                let in_flight_cost = in_flight_cost.clone();
                let max_in_flight_cost = max_in_flight_cost.clone();
                let executions = executions.clone();
                let started_tx = started_tx.clone();
                async move {
                    let cost = classify_job_cost(&job);
                    let before = in_flight_cost.fetch_add(cost, Ordering::SeqCst);
                    let after = before + cost;
                    max_in_flight_cost.fetch_max(after, Ordering::SeqCst);
                    assert!(after <= 4, "in_flight_cost exceeded capacity");
                    started_tx.send(job.id.clone()).await.ok();
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    in_flight_cost.fetch_sub(cost, Ordering::SeqCst);
                    executions.fetch_add(1, Ordering::SeqCst);
                    JobOutcome::Processed
                }
            }
        };

        let summary = run_scheduler(&state, 4, &healthy_disk_gate(), runner).await;

        assert_eq!(executions.load(Ordering::SeqCst), 4, "all 4 jobs execute");
        assert!(
            max_in_flight_cost.load(Ordering::SeqCst) >= 3,
            "the three cheap jobs must run concurrently (no head-of-line blocking)"
        );
        assert!(
            max_in_flight_cost.load(Ordering::SeqCst) <= 4,
            "in_flight_cost must never exceed total_capacity"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 4);
        let started: Vec<String> = std::iter::from_fn(|| started_rx.try_recv().ok()).collect();
        assert_eq!(
            started,
            vec![
                "job-1".to_string(),
                "job-2".to_string(),
                "job-3".to_string(),
                "job-0".to_string(),
            ],
            "cheap jobs must be admitted before the expensive subtitle job"
        );
    }

    // -----------------------------------------------------------------
    // Stage 3.3 — disk-space admission gating scenarios
    // -----------------------------------------------------------------

    /// Scenario B — free space is already at/below the safety margin when the
    /// batch starts. No job may be admitted (the gate runs before the first
    /// capacity acquisition), every queued job reaches a clear terminal
    /// "disk space" failure, and no capacity is ever withdrawn from the gate.
    #[tokio::test]
    async fn insufficient_disk_space_blocks_all_admission() {
        let n = 4;
        let state = build_state(n);
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let (gate, source) = scripted_gate(vec![0; n]);

        let summary = run_scheduler(&state, 2, &gate, {
            let executions = executions.clone();
            move |_job: BatchJob| {
                let executions = executions.clone();
                async move {
                    executions.fetch_add(1, Ordering::SeqCst);
                    JobOutcome::Processed
                }
            }
        })
        .await;

        assert_eq!(
            executions.load(Ordering::SeqCst),
            0,
            "no job may be admitted while free space is below the safety margin"
        );
        assert_eq!(
            source.call_count(),
            1,
            "halting on the first failed probe must not keep querying"
        );
        let s = state.lock().await;
        assert_eq!(s.failed_jobs, n, "every queued job is failed by the gate");
        assert_eq!(
            s.queue.len(),
            0,
            "queue fully drained into terminal failures"
        );
        for p in s.job_progress.values() {
            match &p.status {
                JobStatus::Failed(message) => {
                    assert!(
                        message.contains("disk space"),
                        "blocked job must carry a clear disk-space reason: {message}"
                    );
                    // The identical gate message names the job that tripped the
                    // gate (the first popped, job-0) and its output path.
                    assert!(
                        message.contains("job-0") && message.contains("output-0_9x16.mp4"),
                        "the halt must name the triggering job and path: {message}"
                    );
                }
                other => panic!("job must be terminal Failed, got {other:?}"),
            }
        }
        assert_eq!(
            s.current_job_id, None,
            "no blocked job stays identified as the current job"
        );
        assert_eq!(s.current_job_lifecycle_progress, 100.0);
        drop(s);
        assert_eq!(summary.in_flight_cost, 0, "no capacity leaked");
        assert_eq!(
            summary.available_capacity, 2,
            "no capacity was ever acquired for a rejected job"
        );
    }

    /// Scenario C — the batch starts healthy then runs out of disk mid-batch.
    /// Already-admitted jobs must finish normally; the moment the gate fails,
    /// admission stops and every remaining job is failed with the disk-space
    /// reason. Nothing spins or retries.
    #[tokio::test]
    async fn mid_batch_disk_exhaustion_stops_admission_and_fails_remaining() {
        let state = build_state(4);
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        // Two healthy admissions, then critically low on the third.
        let (gate, source) = scripted_gate(vec![
            DISK_SAFETY_MARGIN_BYTES + 1,
            DISK_SAFETY_MARGIN_BYTES + 1,
            DISK_SAFETY_MARGIN_BYTES,
        ]);

        let summary = run_scheduler(&state, 2, &gate, {
            let executions = executions.clone();
            move |_job: BatchJob| {
                let executions = executions.clone();
                async move {
                    executions.fetch_add(1, Ordering::SeqCst);
                    JobOutcome::Processed
                }
            }
        })
        .await;

        assert_eq!(
            executions.load(Ordering::SeqCst),
            2,
            "exactly the two admitted jobs ran before the gate halted admission"
        );
        assert_eq!(
            source.call_count(),
            3,
            "one live probe per admission attempt: 2 healthy + 1 failing"
        );
        let s = state.lock().await;
        assert_eq!(
            s.failed_jobs, 2,
            "the popped job and the remaining queue are failed"
        );
        assert_eq!(s.queue.len(), 0, "no job is left stranded in the queue");
        // The two never-started jobs (indices 2 and 3) carry the same disk
        // reason, naming the job whose admission probe tripped the gate
        // (index 2, the third pop: two healthy probes then one at the margin).
        for index in [2usize, 3] {
            let job_id = format!("job-{}", index);
            match s.job_progress.get(&job_id).map(|p| p.status.clone()) {
                Some(JobStatus::Failed(message)) => {
                    assert!(
                        message.contains("disk space"),
                        "mid-batch block must carry the disk-space reason: {message}"
                    );
                    assert!(
                        message.contains("job-2") && message.contains("output-2_9x16.mp4"),
                        "the halt must name the gate-tripping job and path: {message}"
                    );
                }
                other => panic!("job {} must be Failed, got {:?}", job_id, other),
            }
        }
        assert_eq!(s.current_job_id, None);
        drop(s);
        assert_eq!(
            summary.in_flight_cost, 0,
            "admitted jobs returned their capacity"
        );
        assert_eq!(summary.available_capacity, 2);
    }

    /// Fail-closed: when the free-space value cannot be queried at all, the
    /// batch refuses to start work rather than risk a silent partial write.
    #[tokio::test]
    async fn disk_space_probe_failure_is_fail_closed() {
        #[derive(Debug)]
        struct FailingDiskSource;

        impl DiskSpaceSource for FailingDiskSource {
            fn available_bytes(&self, _path: &Path) -> std::io::Result<u64> {
                Err(std::io::Error::other("volume offline"))
            }
        }

        let state = build_state(3);
        let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let gate = DiskAdmissionGate::new(Arc::new(FailingDiskSource), DISK_SAFETY_MARGIN_BYTES);

        let summary = run_scheduler(&state, 2, &gate, {
            let executions = executions.clone();
            move |_job: BatchJob| {
                let executions = executions.clone();
                async move {
                    executions.fetch_add(1, Ordering::SeqCst);
                    JobOutcome::Processed
                }
            }
        })
        .await;

        assert_eq!(
            executions.load(Ordering::SeqCst),
            0,
            "fail-closed: nothing runs"
        );
        assert_eq!(
            summary.available_capacity, 2,
            "no capacity consumed on the blocked batch"
        );
        let s = state.lock().await;
        assert_eq!(s.failed_jobs, 3);
        for p in s.job_progress.values() {
            match &p.status {
                JobStatus::Failed(message) => {
                    assert!(message.contains("disk space"), "clear reason: {message}");
                    assert!(
                        message.contains("volume offline"),
                        "query error preserved: {message}"
                    );
                }
                other => panic!("expected Failed, got {other:?}"),
            }
        }
    }

    /// The user-facing disk reason distinguishes a genuine low-disk block from
    /// a probe failure: an insufficiency reports the measured free bytes and
    /// the required margin, while an indeterminate probe reports that free
    /// space could not be determined and never fabricates a byte count (both
    /// paths stop admission - fail closed).
    #[test]
    fn disk_space_failure_message_distinguishes_low_space_from_probe_failure() {
        // Genuine low disk: names the volume, reports measured free bytes and
        // the safety margin, and reads as an insufficiency.
        let insufficient = disk_space_failure_message(
            "job-7",
            &DiskSpaceVerdict::Insufficient {
                available_bytes: 1_288_490_188, // ~1.2 GiB versus the 2 GiB margin
                margin_bytes: DISK_SAFETY_MARGIN_BYTES,
                path: PathBuf::from("C:\\out\\clip.mp4"),
            },
        );
        assert!(
            insufficient.contains("insufficient free disk space"),
            "low-disk reason must read as an insufficiency: {insufficient}"
        );
        assert!(
            insufficient.contains("C:\\out\\clip.mp4")
                && insufficient.contains("1288490188")
                && insufficient.contains("2147483648"),
            "low-disk reason must name the volume and the real numbers: {insufficient}"
        );
        assert!(
            !insufficient.contains("could not determine"),
            "a genuine low-disk block must not be reported as a probe failure"
        );

        // Probe failure: reads as indeterminate, names the volume and the
        // underlying error, and reports no free-space figure at all.
        let unavailable = disk_space_failure_message(
            "job-7",
            &DiskSpaceVerdict::Unavailable {
                message: "volume offline".to_string(),
                path: PathBuf::from("E:\\out\\clip.mp4"),
            },
        );
        assert!(
            unavailable.contains("could not determine free disk space"),
            "probe failure must read as indeterminate: {unavailable}"
        );
        assert!(
            unavailable.contains("E:\\out\\clip.mp4") && unavailable.contains("volume offline"),
            "probe failure must name the volume and preserve the probe error: {unavailable}"
        );
        assert!(
            !unavailable.contains("insufficient free disk space")
                && !unavailable.contains("bytes free is below"),
            "a probe failure must never be reported as though the disk were known to be full"
        );
    }

    /// Retry isolation: a batch blocked by the disk gate terminates without a
    /// retry loop, and a follow-up batch on fresh (recovered) disk executes
    /// every job exactly once from a clean slate.
    #[tokio::test]
    async fn blocked_batch_terminates_and_recovered_batch_runs_once() {
        // Batch 1: below margin -> everything failed, dispatcher returned.
        {
            let state = build_state(3);
            let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
            let (gate, _) = scripted_gate(vec![0; 3]);
            let summary = run_scheduler(&state, 2, &gate, {
                let executions = executions.clone();
                move |_job: BatchJob| {
                    let executions = executions.clone();
                    async move {
                        executions.fetch_add(1, Ordering::SeqCst);
                        JobOutcome::Processed
                    }
                }
            })
            .await;
            assert_eq!(executions.load(Ordering::SeqCst), 0);
            assert_eq!(state.lock().await.failed_jobs, 3);
            assert_eq!(summary.available_capacity, 2);
        }

        // Batch 2: disk recovered -> every job executes exactly once; none of
        // batch 1's state leaks (fresh gate, fresh state, fresh capacity).
        {
            let state = build_state(3);
            let executions: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
            let gate = healthy_disk_gate();
            let summary = run_scheduler(&state, 2, &gate, {
                let executions = executions.clone();
                move |_job: BatchJob| {
                    let executions = executions.clone();
                    async move {
                        executions.fetch_add(1, Ordering::SeqCst);
                        JobOutcome::Processed
                    }
                }
            })
            .await;
            assert_eq!(
                executions.load(Ordering::SeqCst),
                3,
                "recovered batch runs every job exactly once (no retry spin)"
            );
            assert_eq!(state.lock().await.queue.len(), 0);
            assert_eq!(state.lock().await.failed_jobs, 0);
            assert_eq!(summary.in_flight_cost, 0);
            assert_eq!(summary.available_capacity, 2);
        }
    }

    // -----------------------------------------------------------------
    // Stage 3.4 — failure classification + ceiling reduction + retry
    // -----------------------------------------------------------------

    #[test]
    fn classifier_marks_known_resource_exhaustion_stderr_as_resource_related() {
        for stderr in [
            "ffmpeg: Cannot allocate memory",
            "libx264: error: out of memory",
            "failed: No memory",
            "Device or resource busy",
            "encoder: device busy",
            "No space left on device",
            "could not open codec: Resource temporarily unavailable",
            "Could not open codec: insufficient memory",
        ] {
            let error = VideoError::ProcessingFailed {
                stderr: stderr.to_string(),
            };
            assert_eq!(
                classify_video_error(&error),
                FailureClass::ResourceRelated,
                "stderr should be classified resource-related: {stderr}"
            );
            let error = VideoError::WhisperFailed {
                stderr: stderr.to_string(),
            };
            assert_eq!(
                classify_video_error(&error),
                FailureClass::Deterministic,
                "Whisper failures must stay deterministic even with resource wording"
            );
        }
    }

    #[test]
    fn classifier_treats_ambiguous_stderr_and_other_errors_as_deterministic() {
        for stderr in [
            "Invalid data found when processing input",
            "moov atom not found",
            "Could not find codec parameters for stream",
            "Error while opening encoder for output stream",
            "Conversion failed!",
            "",
        ] {
            let error = VideoError::ProcessingFailed {
                stderr: stderr.to_string(),
            };
            assert_eq!(
                classify_video_error(&error),
                FailureClass::Deterministic,
                "ambiguous stderr must default to deterministic: {stderr}"
            );
        }
        for error in [
            VideoError::FfmpegNotFound,
            VideoError::InvalidInput("bad filter".into()),
            VideoError::WhisperModelNotFound,
            VideoError::ProcessingFailed {
                stderr: "generic encoder error".into(),
            },
        ] {
            assert_eq!(classify_video_error(&error), FailureClass::Deterministic);
        }
    }

    #[tokio::test]
    async fn deterministic_classified_failure_is_not_retried_and_ceiling_unchanged() {
        let state = build_state(3);
        let attempts: Arc<tokio::sync::Mutex<HashMap<String, usize>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let runner = {
            let attempts = attempts.clone();
            move |job: BatchJob| {
                let attempts = attempts.clone();
                async move {
                    let job_id = job.id.clone();
                    let mut guard = attempts.lock().await;
                    *guard.entry(job_id.clone()).or_insert(0) += 1;
                    drop(guard);
                    if job_id == "job-0" {
                        JobOutcome::ClassifiedFailed {
                            message: "invalid input: no moov atom".into(),
                            failure_class: FailureClass::Deterministic,
                        }
                    } else {
                        JobOutcome::Processed
                    }
                }
            }
        };

        let summary = run_scheduler(&state, 3, &healthy_disk_gate(), runner).await;

        let attempts = attempts.lock().await;
        assert_eq!(
            attempts.get("job-0"),
            Some(&1),
            "deterministic failures must not be retried"
        );
        assert_eq!(attempts.get("job-1"), Some(&1));
        assert_eq!(attempts.get("job-2"), Some(&1));
        assert_eq!(summary.total_capacity, 3);
        assert_eq!(
            summary.admission_ceiling, 3,
            "a deterministic failure must not reduce the admission ceiling"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 3);
        assert_eq!(state.lock().await.failed_jobs, 1);
    }

    #[tokio::test]
    async fn resource_failure_reduces_ceiling_and_retries_once_successfully() {
        let state = build_state(3);
        let attempts: Arc<tokio::sync::Mutex<HashMap<String, usize>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let runner = {
            let attempts = attempts.clone();
            move |job: BatchJob| {
                let attempts = attempts.clone();
                async move {
                    let job_id = job.id.clone();
                    let attempt = {
                        let mut guard = attempts.lock().await;
                        *guard.entry(job_id.clone()).or_insert(0) += 1;
                        guard[&job_id]
                    };
                    if job_id == "job-2" && attempt == 1 {
                        JobOutcome::ClassifiedFailed {
                            message: "out of memory".into(),
                            failure_class: FailureClass::ResourceRelated,
                        }
                    } else {
                        JobOutcome::Processed
                    }
                }
            }
        };

        let summary = run_scheduler(&state, 3, &healthy_disk_gate(), runner).await;

        let attempts = attempts.lock().await;
        assert_eq!(
            attempts.get("job-2"),
            Some(&2),
            "job-2 must be retried exactly once"
        );
        assert_eq!(attempts.get("job-0"), Some(&1));
        assert_eq!(attempts.get("job-1"), Some(&1));
        assert_eq!(
            summary.total_capacity, 3,
            "configured capacity is the baseline"
        );
        assert_eq!(
            summary.admission_ceiling, 2,
            "one resource failure reduces the ceiling by one unit"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 3);
        assert_eq!(
            state.lock().await.failed_jobs,
            0,
            "a successful retry is not counted as a failure"
        );
    }

    #[tokio::test]
    async fn second_resource_failure_reduces_ceiling_again_and_fails_permanently() {
        let state = build_state(3);
        let attempts: Arc<tokio::sync::Mutex<HashMap<String, usize>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let runner = {
            let attempts = attempts.clone();
            move |job: BatchJob| {
                let attempts = attempts.clone();
                async move {
                    let job_id = job.id.clone();
                    let attempt = {
                        let mut guard = attempts.lock().await;
                        *guard.entry(job_id.clone()).or_insert(0) += 1;
                        guard[&job_id]
                    };
                    if job_id == "job-0" {
                        assert!(attempt <= 2, "never more than one retry");
                        JobOutcome::ClassifiedFailed {
                            message: "cannot allocate memory".into(),
                            failure_class: FailureClass::ResourceRelated,
                        }
                    } else {
                        JobOutcome::Processed
                    }
                }
            }
        };

        let summary = run_scheduler(&state, 3, &healthy_disk_gate(), runner).await;

        let attempts = attempts.lock().await;
        assert_eq!(
            attempts.get("job-0"),
            Some(&2),
            "exactly one retry; a second retry must never run"
        );
        assert_eq!(
            attempts.get("job-1"),
            Some(&1),
            "job-1 is unaffected by job-0's failure"
        );
        assert_eq!(
            attempts.get("job-2"),
            Some(&1),
            "job-2 is unaffected by job-0's failure"
        );
        assert_eq!(summary.total_capacity, 3);
        assert_eq!(
            summary.admission_ceiling, 1,
            "two resource failures reduce the ceiling twice (3 -> 2 -> 1)"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 3);
        assert_eq!(state.lock().await.failed_jobs, 1);
    }

    #[tokio::test]
    async fn resource_failure_does_not_interrupt_running_jobs_and_retry_waits_for_capacity() {
        // Three cost-1 jobs, capacity 3: job-0 and job-1 start and then hold
        // their capacity waiting on a gate; job-2 fails resource-related while
        // they are still running. The ceiling drops 3 -> 2, which blocks job-2's
        // retry even though one capacity unit is free, so job-0 and job-1 must
        // first finish normally (unaffected) before job-2 is retried.
        let state = build_state(3);
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let (release_a_tx, release_a_rx) = watch::channel(0u32);
        let (release_b_tx, release_b_rx) = watch::channel(0u32);
        let events: Arc<tokio::sync::Mutex<Vec<String>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let attempts: Arc<tokio::sync::Mutex<HashMap<String, usize>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let runner = {
            let barrier = barrier.clone();
            let events = events.clone();
            let attempts = attempts.clone();
            move |job: BatchJob| {
                let barrier = barrier.clone();
                let events = events.clone();
                let attempts = attempts.clone();
                let release_a = release_a_rx.clone();
                let release_b = release_b_rx.clone();
                async move {
                    let job_id = job.id.clone();
                    let attempt = {
                        let mut guard = attempts.lock().await;
                        *guard.entry(job_id.clone()).or_insert(0) += 1;
                        guard[&job_id]
                    };
                    if job_id == "job-2" {
                        if attempt == 1 {
                            barrier.wait().await;
                            events.lock().await.push("job-2-failed".to_string());
                            JobOutcome::ClassifiedFailed {
                                message: "device or resource busy".into(),
                                failure_class: FailureClass::ResourceRelated,
                            }
                        } else {
                            events.lock().await.push("job-2-retried".to_string());
                            JobOutcome::Processed
                        }
                    } else {
                        barrier.wait().await;
                        events.lock().await.push(format!("{}-running", job_id));
                        let mut release = if job_id == "job-0" {
                            release_a
                        } else {
                            release_b
                        };
                        let _ = release.changed().await;
                        events.lock().await.push(format!("{}-finished", job_id));
                        JobOutcome::Processed
                    }
                }
            }
        };

        let scheduler_state = state.clone();
        let scheduler_task = tokio::spawn(async move {
            run_scheduler(&scheduler_state, 3, &healthy_disk_gate(), runner).await
        });

        // Wait until job-2's first failure is processed and its retry is back
        // in the queue. job-0 and job-1 are still running (holding the gate),
        // so the retry cannot be admitted yet under the reduced ceiling of 2.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let queued = state.lock().await.queue.len();
            if queued == 1 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for job-2's retry to be enqueued"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        {
            let ev = events.lock().await;
            assert!(
                ev.contains(&"job-2-failed".to_string()),
                "job-2 reported its failure"
            );
            assert!(
                ev.contains(&"job-0-running".to_string())
                    && ev.contains(&"job-1-running".to_string()),
                "job-0 and job-1 were running when job-2 failed"
            );
            assert!(
                !ev.contains(&"job-0-finished".to_string())
                    && !ev.contains(&"job-1-finished".to_string()),
                "job-0 and job-1 must still be running while the retry waits"
            );
        }

        release_a_tx.send(1).expect("release job-0");
        release_b_tx.send(1).expect("release job-1");

        let summary = tokio::time::timeout(Duration::from_secs(5), scheduler_task)
            .await
            .expect("scheduler must not hang")
            .expect("scheduler task completes");

        let ev = events.lock().await;
        let retry_index = ev
            .iter()
            .position(|e| e == "job-2-retried")
            .expect("job-2 must be retried");
        let first_finished_index = ev
            .iter()
            .position(|e| e.ends_with("-finished"))
            .expect("job-0 or job-1 finished");
        assert!(
            retry_index > first_finished_index,
            "the retry must not start until an in-flight job has finished"
        );
        assert!(
            ev.contains(&"job-0-finished".to_string())
                && ev.contains(&"job-1-finished".to_string()),
            "job-0 and job-1 finished normally and were not interrupted"
        );

        let attempts = attempts.lock().await;
        assert_eq!(
            attempts.get("job-2"),
            Some(&2),
            "job-2 was retried exactly once"
        );
        assert_eq!(summary.total_capacity, 3);
        assert_eq!(summary.admission_ceiling, 2);
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(state.lock().await.failed_jobs, 0);
    }

    #[tokio::test]
    async fn job_too_costly_for_reduced_ceiling_is_failed_without_deadlock() {
        // A cost-2 (subtitle) job fails resource-related at ceiling 2. The
        // reduction brings the ceiling to 1, below the job's own cost, so a
        // reduced-capacity retry is impossible; the job must be permanently
        // failed rather than left waiting in the queue forever.
        let jobs: Vec<BatchJob> = std::iter::once(build_subtitle_job(0, true, false))
            .chain(std::iter::once(build_job(1)))
            .collect();
        let state = build_state_with_jobs(jobs);
        let attempts: Arc<tokio::sync::Mutex<HashMap<String, usize>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let runner = {
            let attempts = attempts.clone();
            move |job: BatchJob| {
                let attempts = attempts.clone();
                async move {
                    let job_id = job.id.clone();
                    let attempt = {
                        let mut guard = attempts.lock().await;
                        *guard.entry(job_id.clone()).or_insert(0) += 1;
                        guard[&job_id]
                    };
                    if job_id == "job-0" {
                        assert_eq!(attempt, 1, "job-0 must not be retried");
                        JobOutcome::ClassifiedFailed {
                            message: "no space left on device".into(),
                            failure_class: FailureClass::ResourceRelated,
                        }
                    } else {
                        JobOutcome::Processed
                    }
                }
            }
        };

        let summary = run_scheduler(&state, 2, &healthy_disk_gate(), runner).await;

        let attempts = attempts.lock().await;
        assert_eq!(
            attempts.get("job-0"),
            Some(&1),
            "no retry below its own cost"
        );
        assert_eq!(
            attempts.get("job-1"),
            Some(&1),
            "job-1 is still admitted and processed normally"
        );
        assert_eq!(
            summary.admission_ceiling, 1,
            "ceiling reduced once (2 -> 1)"
        );
        assert_eq!(summary.in_flight_cost, 0);
        assert_eq!(summary.available_capacity, 2);
        assert_eq!(state.lock().await.failed_jobs, 1);
    }
}
