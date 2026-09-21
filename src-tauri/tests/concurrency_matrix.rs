//! Roadmap Testing & Validation Matrix — Phase 2 concurrency rows
//! (Tests B, C, D, I, I-Ffmpeg, J, K).
//!
//! These exercise the real runtime pipeline (real Tauri app, real ffmpeg
//! sidecars) against the Stage 2.3 behaviour: the batch now runs through the
//! Stage 2.1 capacity-based scheduler with the actual Phase 1 ConcurrencyPlan,
//! so `total_capacity > 1` on this machine means multiple FFmpeg jobs may be
//! in flight at once.
//!
//! Each test derives the machine's real capacity via
//! `calculate_safe_concurrency(detect_system_resources())` rather than
//! assuming a fixed value, so the assertions hold on any hardware tier.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use aspectshift_htov_lib::video::batch_processor::{get_batch_status, start_batch};
use aspectshift_htov_lib::video::concurrency::{
    calculate_safe_concurrency, detect_system_resources,
};
use aspectshift_htov_lib::video::paths::resolve_temp_output_path;
use aspectshift_htov_lib::video::queue::BatchManager;
use aspectshift_htov_lib::video::types::{
    AspectRatio, BatchJobSettings, BatchProgress, BatchStatus, EncodingProfile, FileProgress,
    JobStatus, OutputJob, SelectionMetadata, SubtitleOverlaySettings, TargetType,
    VideoEffectsSettings,
};
use tauri::{Listener, Manager};

fn machine_capacity() -> usize {
    calculate_safe_concurrency(&detect_system_resources()).total_capacity
}

fn ffmpeg_bin() -> PathBuf {
    let deps = std::env::current_exe().unwrap();
    deps.parent().unwrap().parent().unwrap().join("ffmpeg.exe")
}

fn ffprobe_bin() -> PathBuf {
    let deps = std::env::current_exe().unwrap();
    deps.parent().unwrap().parent().unwrap().join("ffprobe.exe")
}

fn run_capture(program: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute {}: {}", program.display(), e))
}

fn ffmpeg_ok(args: &[&str]) -> String {
    let out = run_capture(&ffmpeg_bin(), args);
    assert!(
        out.status.success(),
        "ffmpeg failed: {}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn probe_video(path: &Path) -> String {
    let out = run_capture(
        &ffprobe_bin(),
        &[
            "-v",
            "error",
            "-show_entries",
            "format=duration,format_name",
            "-of",
            "default=noprint_wrappers=1",
            &path.to_string_lossy(),
        ],
    );
    assert!(
        out.status.success(),
        "ffprobe failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// 640x360 landscape clip with optional sine audio (forces a real libx264
/// encode for every portrait target, since horizontal input is never
/// passthrough).
fn make_test_video(path: &Path, duration_secs: u32, with_audio: bool) {
    let mut args: Vec<String> = Vec::new();
    args.push("-y".into());
    args.push("-f".into());
    args.push("lavfi".into());
    args.push("-i".into());
    args.push(format!(
        "testsrc=duration={}:size=640x360:rate=30",
        duration_secs
    ));
    if with_audio {
        args.push("-f".into());
        args.push("lavfi".into());
        args.push("-i".into());
        args.push(format!("sine=frequency=440:duration={}", duration_secs));
    }
    args.push("-c:v".into());
    args.push("libx264".into());
    args.push("-preset".into());
    args.push("ultrafast".into());
    if with_audio {
        args.push("-c:a".into());
        args.push("aac".into());
    }
    args.push("-shortest".into());
    args.push(path.to_string_lossy().to_string());
    let joined: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    ffmpeg_ok(&joined);
}

fn app() -> tauri::App {
    tauri::Builder::default()
        .any_thread()
        .plugin(tauri_plugin_shell::init())
        .manage(BatchManager::new())
        .build(tauri::generate_context!())
        .expect("failed to build tauri app")
}

fn test_dir(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "concurrency_matrix_{label}_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn base_effects() -> VideoEffectsSettings {
    VideoEffectsSettings {
        blur: None,
        white_background: None,
        overlays: None,
        subtitles: None,
        color_filter: None,
        blur_sigma: None,
        remove_audio: None,
        export_subtitles: None,
        burn_subtitles: None,
        skip_existing: None,
        output_format: None,
        logo: None,
        text_overlay: Default::default(),
        subtitle_overlay: SubtitleOverlaySettings::default(),
        transform: None,
    }
}

fn output_job(id: &str, ratio: AspectRatio) -> OutputJob {
    OutputJob {
        id: id.to_string(),
        ratio,
        encoding: EncodingProfile::standard(),
        effects: base_effects(),
        platform_config: None,
        selection: SelectionMetadata {
            source_type: TargetType::AspectRatio,
            source_id: "9:16".to_string(),
            label: "9:16".to_string(),
        },
    }
}

fn settings(output_dir: &Path, targets: Vec<OutputJob>) -> BatchJobSettings {
    BatchJobSettings {
        targets,
        output_dir: output_dir.to_string_lossy().to_string(),
        enable_subfolders: false,
    }
}

fn is_terminal(status: &BatchStatus) -> bool {
    matches!(
        status,
        BatchStatus::Completed | BatchStatus::Failed | BatchStatus::Cancelled
    )
}

async fn wait_for_terminal(app: &tauri::App, timeout: Duration) -> BatchProgress {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let manager = app.state::<BatchManager>();
        let progress = get_batch_status(manager).await.expect("status read failed");
        if is_terminal(&progress.status) {
            return progress;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "batch did not reach terminal state within {}s (current: {:?})",
            timeout.as_secs(),
            progress.status
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn assert_valid_video(path: &Path) {
    assert!(path.exists(), "expected output at {}", path.display());
    assert!(std::fs::metadata(path).unwrap().len() > 0);
    let probe = probe_video(path);
    assert!(
        probe.contains("duration"),
        "output is not a valid video: {}",
        probe
    );
}

/// Occupies `path` with a non-empty directory so any later attempt to treat it
/// as a file (an FFmpeg temp output, or a finalize rename target) fails.
fn occupy_dir(path: &Path) {
    std::fs::create_dir(path).unwrap_or_else(|e| panic!("create_dir {}: {e}", path.display()));
    std::fs::write(path.join(".keep"), b"").unwrap();
}

/// Asserts a `Failed` reason originated inside the ffmpeg child process rather
/// than the Rust-side finalize step. When FFmpeg cannot open its output it
/// prints `Error opening output file <temp path>` (and `Error opening output
/// files: Permission denied`) to stderr and exits non-zero; `run_ffmpeg`
/// surfaces that stderr in the per-job reason, so the reason must reference the
/// job's temp output file. The finalize/rename failure path never references it.
fn assert_ffmpeg_process_failure(reason: &str, temp_output_file: &str) {
    assert!(
        !reason.is_empty(),
        "Failed status must carry a real failure reason"
    );
    assert!(
        reason
            .to_lowercase()
            .contains(&temp_output_file.to_lowercase()),
        "reason must prove a genuine FFmpeg-process failure by referencing the temp output \
         path {temp_output_file} from FFmpeg's stderr; got: {reason:?}"
    );
}

struct ProgressRecorder {
    max_processing: usize,
}

/// Timestamped per-job `batch://file-status` events, used to prove ordering
/// guarantees in the failure-isolation tests (e.g. a queued job may only start
/// once a failed job has released its capacity slot).
fn record_file_status_events(
    app: &tauri::App,
) -> Arc<StdMutex<Vec<(FileProgress, std::time::Instant)>>> {
    let events: Arc<StdMutex<Vec<(FileProgress, std::time::Instant)>>> =
        Arc::new(StdMutex::new(Vec::new()));
    {
        let cap = events.clone();
        app.listen("batch://file-status", move |e| {
            if let Ok(p) = serde_json::from_str::<FileProgress>(e.payload()) {
                cap.lock().unwrap().push((p, std::time::Instant::now()));
            }
        });
    }
    events
}

/// Reduces the timestamped `batch://file-status` stream into per-input
/// Processing intervals `(start, end)`: the interval opens on the job's first
/// Processing event and closes on its first terminal event.
///
/// The queue admission order in `start_batch` is the (nondeterministic)
/// completion order of the parallel probe/thumbnail preparation tasks, so the
/// *identity* of "the job admitted right after the failing one" cannot be
/// predicted. The failure-isolation proofs are therefore expressed as interval
/// relationships that hold regardless of queue order (see the two tests below).
fn busy_intervals(
    events: &[(FileProgress, std::time::Instant)],
) -> std::collections::HashMap<String, (std::time::Instant, std::time::Instant)> {
    let mut starts: std::collections::HashMap<String, std::time::Instant> =
        std::collections::HashMap::new();
    let mut intervals: std::collections::HashMap<String, (std::time::Instant, std::time::Instant)> =
        std::collections::HashMap::new();
    for (p, t) in events {
        let key = p.file_path.clone();
        match p.status {
            JobStatus::Processing => {
                starts.entry(key).or_insert(*t);
            }
            JobStatus::Completed | JobStatus::Failed(_) | JobStatus::Cancelled => {
                if let Some(start) = starts.remove(&key) {
                    intervals.entry(key).or_insert((start, *t));
                }
            }
            _ => {}
        }
    }
    intervals
}

fn intervals_overlap(
    a: (std::time::Instant, std::time::Instant),
    b: (std::time::Instant, std::time::Instant),
) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Shared snapshot watcher that tracks how many jobs appeared Processing
/// simultaneously across every `batch://progress` event.
fn record_processing_snapshots(
    app: &tauri::App,
) -> Arc<StdMutex<(Vec<BatchProgress>, ProgressRecorder)>> {
    let recorder: Arc<StdMutex<(Vec<BatchProgress>, ProgressRecorder)>> = Arc::new(StdMutex::new(
        (Vec::new(), ProgressRecorder { max_processing: 0 }),
    ));
    {
        let cap = recorder.clone();
        app.listen("batch://progress", move |e| {
            if let Ok(p) = serde_json::from_str::<BatchProgress>(e.payload()) {
                let mut guard = cap.lock().unwrap();
                guard.0.push(p.clone());
                let processing = p
                    .queue
                    .iter()
                    .filter(|f| matches!(f.status, JobStatus::Processing))
                    .count();
                guard.1.max_processing = guard.1.max_processing.max(processing);
            }
        });
    }
    recorder
}

// ---------------------------------------------------------------------------
// Test B — two videos concurrently.
// Acceptance: correct output for both; independent progress per job; no
// cross-talk; no lock collision. On machines whose computed capacity is >= 2,
// the two jobs must genuinely run at the same time.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn two_videos_run_concurrently_with_independent_progress() {
    let capacity = machine_capacity();

    let app = app();
    let root = test_dir("two");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let a = root.join("a.mp4");
    let b = root.join("b.mp4");
    make_test_video(&a, 6, true);
    make_test_video(&b, 6, true);

    let recorder = record_processing_snapshots(&app);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-b1", AspectRatio::Ratio9x16)];
    let inputs = [
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    ];
    start_batch(
        app.handle().clone(),
        manager,
        inputs.to_vec(),
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(240)).await;
    assert_eq!(terminal.status, BatchStatus::Completed);
    assert_eq!(terminal.completed_jobs, 2);
    assert_eq!(terminal.failed_jobs, 0);

    let out_a = out_dir.join("a_9x16.mp4");
    let out_b = out_dir.join("b_9x16.mp4");
    assert_valid_video(&out_a);
    assert_valid_video(&out_b);

    // Independent progress / no cross-talk: both jobs reached their own
    // terminal per-file status, and every completed job shows 100% on its
    // own FileProgress entry.
    for entry in &terminal.queue {
        match &entry.status {
            JobStatus::Completed => assert_eq!(entry.progress, 100.0),
            other => panic!("expected only Completed per-file statuses, got {:?}", other),
        }
    }

    {
        let guard = recorder.lock().unwrap();
        assert!(!guard.0.is_empty(), "no batch://progress events captured");
        let last = guard.0.last().unwrap();
        assert_eq!(last.status, BatchStatus::Completed);
        assert_eq!(last.percentage, 100.0);

        if capacity >= 2 {
            assert!(
                guard.1.max_processing >= 2,
                "expected the two jobs to overlap in the Processing state (real concurrency); \
                 observed max simultaneous Processing = {}, capacity = {}",
                guard.1.max_processing,
                capacity
            );
        } else {
            // Documented environment limitation: with computed total_capacity
            // == 1 the scheduler is sequential by design. Outputs and tallies
            // still verified above.
            println!("SKIP overlap assertion: machine capacity is {capacity} (sequential)");
        }
    }

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Test C — 10–20 videos.
// Acceptance: capacity stays capped (simultaneous Processing never exceeds
// total_capacity); queue drains correctly; no job skipped or duplicated.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn twelve_videos_drain_within_capacity_cap() {
    let capacity = machine_capacity();
    assert!(capacity >= 1);
    let n = 12;

    let app = app();
    let root = test_dir("drain");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut inputs = Vec::new();
    for i in 0..n {
        let p = root.join(format!("clip{i}.mp4"));
        make_test_video(&p, 2, false);
        inputs.push(p.to_string_lossy().to_string());
    }

    let recorder = record_processing_snapshots(&app);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-c", AspectRatio::Ratio9x16)];
    start_batch(
        app.handle().clone(),
        manager,
        inputs,
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(420)).await;
    assert_eq!(terminal.status, BatchStatus::Completed);
    assert_eq!(terminal.completed_jobs, n);
    assert_eq!(terminal.failed_jobs, 0);

    // Queue fully drained: every job id reached terminal Completed status.
    assert_eq!(
        terminal.queue.len(),
        n,
        "every queued job present exactly once"
    );
    for entry in &terminal.queue {
        assert!(
            matches!(entry.status, JobStatus::Completed),
            "job {} did not complete: {:?}",
            entry.job_id,
            entry.status
        );
    }

    // Exactly n valid outputs (no skipped or duplicated files).
    let outputs = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "mp4").unwrap_or(false))
        .collect::<Vec<_>>();
    assert_eq!(
        outputs.len(),
        n,
        "expected {n} output files, got {}",
        outputs.len()
    );
    for out in &outputs {
        assert_valid_video(out);
    }

    {
        let guard = recorder.lock().unwrap();
        assert!(!guard.0.is_empty());
        let last = guard.0.last().unwrap();
        assert_eq!(last.status, BatchStatus::Completed);
        assert_eq!(last.percentage, 100.0);
        // Capacity stays capped at every snapshot.
        assert!(
            guard.1.max_processing <= capacity,
            "observed {} simultaneous Processing jobs, exceeding capacity {}",
            guard.1.max_processing,
            capacity
        );
    }

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Test D — same source, multiple targets (9:16, 1:1, 16:9).
// Acceptance: these run concurrently (validates Stage 0.1's revised,
// target-scoped lock) with no false collision, while a true duplicate (same
// input + same output target queued twice) is still blocked.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn same_source_multiple_targets_run_concurrently_and_duplicate_blocked() {
    let capacity = machine_capacity();

    // --- Part 1: same source, three different targets run concurrently ---
    let app1 = app();
    let root = test_dir("targets");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let input = root.join("source.mp4");
    make_test_video(&input, 6, true);

    let recorder = record_processing_snapshots(&app1);

    let manager = app1.state::<BatchManager>();
    let targets = vec![
        output_job("t-9x16", AspectRatio::Ratio9x16),
        output_job("t-1x1", AspectRatio::Ratio1x1),
        output_job("t-16x9", AspectRatio::Ratio16x9),
    ];
    start_batch(
        app1.handle().clone(),
        manager,
        vec![input.to_string_lossy().to_string()],
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app1, Duration::from_secs(240)).await;
    assert_eq!(terminal.status, BatchStatus::Completed);
    assert_eq!(terminal.completed_jobs, 3);
    assert_eq!(terminal.failed_jobs, 0);

    // All three target-specific outputs produced (no false lock collision).
    for name in ["source_9x16.mp4", "source_1x1.mp4", "source_16x9.mp4"] {
        assert_valid_video(&out_dir.join(name));
    }

    if capacity >= 2 {
        {
            let guard = recorder.lock().unwrap();
            assert!(
                guard.1.max_processing >= 2,
                "expected multiple targets of the same source to run concurrently; \
                 observed max simultaneous Processing = {}, capacity = {}",
                guard.1.max_processing,
                capacity
            );
        }
    } else {
        println!("SKIP overlap assertion: machine capacity is {capacity} (sequential)");
    }

    let _ = std::fs::remove_dir_all(&root);

    // --- Part 2: a true duplicate (same input + same output target queued
    // twice) is still blocked by the Stage 0.1 lock ---
    if capacity >= 2 {
        let app2 = app();
        let root2 = test_dir("dup");
        let out_dir2 = root2.join("out");
        std::fs::create_dir_all(&out_dir2).unwrap();
        let dup_input = root2.join("dup.mp4");
        make_test_video(&dup_input, 8, false);

        let manager = app2.state::<BatchManager>();
        let dup_target = output_job("job-dup", AspectRatio::Ratio9x16);
        // The same target enqueued twice against the same source: both jobs
        // resolve to the exact same output path, so the second must be
        // rejected by the (input, target)-scoped lock while the first holds it.
        start_batch(
            app2.handle().clone(),
            manager,
            vec![dup_input.to_string_lossy().to_string()],
            settings(&out_dir2, vec![dup_target.clone(), dup_target]),
        )
        .await
        .expect("start_batch failed");

        let terminal = wait_for_terminal(&app2, Duration::from_secs(240)).await;
        assert_eq!(terminal.status, BatchStatus::Failed);
        assert_eq!(terminal.completed_jobs, 1);
        assert_eq!(terminal.failed_jobs, 1);
        let dup = terminal
            .queue
            .iter()
            .filter(|f| matches!(f.status, JobStatus::Failed(_)))
            .collect::<Vec<_>>();
        assert_eq!(dup.len(), 1, "exactly one duplicate must fail");
        match &dup[0].status {
            JobStatus::Failed(reason) => assert!(
                reason.to_lowercase().contains("already processing"),
                "duplicate should be blocked by the lock, got {:?}",
                reason
            ),
            other => panic!("expected Failed duplicate, got {:?}", other),
        }

        // The surviving output is a valid, untouched video.
        assert_valid_video(&out_dir2.join("dup_9x16.mp4"));

        let _ = std::fs::remove_dir_all(&root2);
    } else {
        println!(
            "SKIP duplicate-block assertion: capacity {capacity} serialises the two jobs, \
             so the second is not blocked by the lock"
        );
    }
}

// ---------------------------------------------------------------------------
// Test K — cancel while 2–4 FFmpeg jobs are active (roadmap Stage 2.5).
// Acceptance: cancellation is *requested* the moment the token fires and the
// dispatcher stops admitting work, but the batch does NOT report `Cancelled`
// until every in-flight task has exited and its FFmpeg child is confirmed
// terminated. The test asserts *ordering*, not just the final status:
//   - the batch is still `Processing` (not prematurely `Cancelled`) while
//     active jobs remain,
//   - no queued job starts after cancellation is requested,
//   - every per-job `Cancelled` file-status event precedes the batch-level
//     `Cancelled` progress event,
//   - the final `Cancelled` snapshot carries zero Processing jobs,
//   - no FFmpeg process remains running against this batch afterwards.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn cancel_with_multiple_active_ffmpeg_jobs_waits_for_termination() {
    use std::time::Instant;

    let capacity = machine_capacity();
    let n = 4;

    if capacity < 2 {
        println!(
            "SKIP: computed machine capacity is {capacity} (sequential) — Test K requires \
             ≥2 genuinely concurrent FFmpeg jobs"
        );
        return;
    }

    let app = app();
    let root = test_dir("cancel_k");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    // Long synthetic clips so several FFmpeg encodes are genuinely in flight
    // when cancellation is requested.
    let mut inputs = Vec::new();
    for i in 0..n {
        let p = root.join(format!("clip{i}.mp4"));
        make_test_video(&p, 120, false);
        inputs.push(p.to_string_lossy().to_string());
    }

    // Processing-event snapshots (used to wait for real concurrency).
    let recorder = record_processing_snapshots(&app);

    // Timestamped per-job status transitions and batch progress events, so the
    // guaranteed cross-event ordering can be asserted.
    let file_events: Arc<StdMutex<Vec<(FileProgress, Instant)>>> =
        Arc::new(StdMutex::new(Vec::new()));
    {
        let cap = file_events.clone();
        app.listen("batch://file-status", move |e| {
            if let Ok(p) = serde_json::from_str::<FileProgress>(e.payload()) {
                cap.lock().unwrap().push((p, Instant::now()));
            }
        });
    }
    let progress_events: Arc<StdMutex<Vec<(BatchProgress, Instant)>>> =
        Arc::new(StdMutex::new(Vec::new()));
    {
        let cap = progress_events.clone();
        app.listen("batch://progress", move |e| {
            if let Ok(p) = serde_json::from_str::<BatchProgress>(e.payload()) {
                cap.lock().unwrap().push((p, Instant::now()));
            }
        });
    }

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-k", AspectRatio::Ratio9x16)];
    start_batch(
        app.handle().clone(),
        manager,
        inputs,
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    // Wait until at least two jobs are genuinely active simultaneously.
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        {
            let guard = recorder.lock().unwrap();
            if guard.1.max_processing >= 2 {
                break;
            }
        }
        let manager = app.state::<BatchManager>();
        let progress = get_batch_status(manager).await.expect("status read failed");
        assert!(
            matches!(progress.status, BatchStatus::Processing),
            "batch ended before two jobs became active: {:?}",
            progress.status
        );
        assert!(
            std::time::Instant::now() < deadline,
            "two jobs never became active in time (max observed = {})",
            recorder.lock().unwrap().1.max_processing
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let max_active_before_cancel = recorder.lock().unwrap().1.max_processing;
    assert!(
        max_active_before_cancel >= 2,
        "test precondition: needed ≥2 concurrent jobs, observed {max_active_before_cancel}"
    );
    drop(recorder);

    // Snapshot the jobs that are active (Processing) at cancel time.
    let pre_cancel = {
        let manager = app.state::<BatchManager>();
        get_batch_status(manager)
            .await
            .expect("pre-cancel status read failed")
    };
    let active_before: Vec<String> = pre_cancel
        .queue
        .iter()
        .filter(|f| matches!(f.status, JobStatus::Processing))
        .map(|f| f.job_id.clone())
        .collect();
    assert!(
        active_before.len() >= 2,
        "expected ≥2 active jobs at cancel time, got {}",
        active_before.len()
    );

    // Request cancellation (Phase A).
    let cancel_requested_at = Instant::now();
    let manager = app.state::<BatchManager>();
    manager.cancel().await;

    // Read the batch status immediately: it must still be Processing while the
    // in-flight jobs have not all exited. (If shutdown completed inside the
    // cancel() call this observation may already be terminal; the event
    // ordering assertions below still prove the guarantee.)
    let post_cancel = {
        let manager = app.state::<BatchManager>();
        get_batch_status(manager)
            .await
            .expect("post-cancel status read failed")
    };

    // Wait for the batch to reach its terminal state (should be Cancelled).
    let terminal = wait_for_terminal(&app, Duration::from_secs(120)).await;
    assert_eq!(
        terminal.status,
        BatchStatus::Cancelled,
        "batch must end Cancelled after a user cancellation"
    );

    // Terminal snapshot invariants.
    let terminal_processing = terminal
        .queue
        .iter()
        .filter(|f| matches!(f.status, JobStatus::Processing))
        .count();
    assert_eq!(
        terminal_processing, 0,
        "no job may remain Processing once the batch reports Cancelled"
    );
    for entry in &terminal.queue {
        assert!(
            matches!(
                entry.status,
                JobStatus::Cancelled | JobStatus::Completed | JobStatus::Failed(_)
            ),
            "job {} left non-terminal status {:?}",
            entry.job_id,
            entry.status
        );
    }

    // ------------------------------------------------------------------
    // Ordering assertion 1 — no premature Cancelled.
    // Either we observed the shutdown window directly (status still
    // Processing with active jobs right after cancel) or the shutdown was so
    // fast that the terminal observation came first; in the latter case every
    // recorded batch://progress event must still respect the ordering below.
    // ------------------------------------------------------------------
    if post_cancel.status != BatchStatus::Cancelled {
        assert_eq!(
            post_cancel.status,
            BatchStatus::Processing,
            "unexpected post-cancel status {:?}",
            post_cancel.status
        );
        assert!(
            post_cancel
                .queue
                .iter()
                .any(|f| matches!(f.status, JobStatus::Processing)),
            "cancellation reported complete before the active jobs finished: \
             expected some jobs still Processing right after cancel"
        );
    } else {
        println!(
            "SKIP direct shutdown-window observation: cancellation completed \
             within the cancel() call; event-ordering assertions still verify the stage"
        );
    }

    // ------------------------------------------------------------------
    // Ordering assertion 2 — no queued job starts after cancellation.
    // A job "starts" when its per-job status first becomes Processing, which
    // is emitted as a batch://file-status event. No such event may occur after
    // the cancel request.
    // ------------------------------------------------------------------
    {
        let guard = file_events.lock().unwrap();
        let starts_after_cancel: Vec<&str> = guard
            .iter()
            .filter(|(p, t)| matches!(p.status, JobStatus::Processing) && *t > cancel_requested_at)
            .map(|(p, _)| p.job_id.as_str())
            .collect();
        assert!(
            starts_after_cancel.is_empty(),
            "jobs {} started after cancellation was requested",
            starts_after_cancel.join(", ")
        );
    }

    // ------------------------------------------------------------------
    // Ordering assertion 3 — all per-job Cancelled file-status events are
    // emitted strictly before the batch-level Cancelled progress event. Task
    // exit (and thus its child's termination) precedes the batch transition by
    // construction; this is its observable projection.
    // ------------------------------------------------------------------
    {
        let progress_guard = progress_events.lock().unwrap();
        let cancelled_idx = progress_guard
            .iter()
            .position(|(p, _)| p.status == BatchStatus::Cancelled)
            .expect("a batch://progress event with Cancelled status was emitted");
        let cancelled_at = progress_guard[cancelled_idx].1;
        let cancelled_payload = &progress_guard[cancelled_idx].0;
        assert_eq!(
            cancelled_payload
                .queue
                .iter()
                .filter(|f| matches!(f.status, JobStatus::Processing))
                .count(),
            0,
            "final Cancelled progress event must report zero Processing jobs"
        );

        let file_guard = file_events.lock().unwrap();
        let late_cancelled: Vec<&str> = file_guard
            .iter()
            .filter(|(p, t)| matches!(p.status, JobStatus::Cancelled) && *t > cancelled_at)
            .map(|(p, _)| p.job_id.as_str())
            .collect();
        assert!(
            late_cancelled.is_empty(),
            "per-job Cancelled events must precede the batch-level Cancelled event; {} came after: {}",
            late_cancelled.len(),
            late_cancelled.join(", ")
        );
    }

    // ------------------------------------------------------------------
    // Ordering assertion 4 — no FFmpeg child remains running against this
    // batch once it reports Cancelled.
    // ------------------------------------------------------------------
    assert_no_ffmpeg_for_output_dir(&out_dir);

    let _ = std::fs::remove_dir_all(&root);

    // Document the concurrency level that was actually exercised.
    println!(
        "Test K exercised cancellation with {max_active_before_cancel} concurrent jobs \
         (machine capacity {capacity}) and verified ordering: all children/tasks \
         terminated before the batch reported Cancelled"
    );
}

// ---------------------------------------------------------------------------
// Test I — one deliberate FFmpeg failure in a concurrent batch.
// Acceptance (roadmap): the failing job is isolated to itself (per-job
// `Failed`, `failed_jobs == 1`), every other in-flight/queued job continues and
// completes, capacity consumed by the failed job is returned to the scheduler
// (a queued job is started after the failure), no deadlock occurs, and the
// final batch tally is exactly `Completed: N, Failed: 1`.
//
// The failure is induced through the *real* render boundary: `render_single`
// renders to `<output>.tmp.mp4`, then `finalize_temp_output` renames the temp
// onto the final path. If that final path is pre-occupied by a directory, the
// rename fails and the whole job fails through the ordinary FFmpeg/render error
// path — no mocked processing, no special-casing. (This is the finalize-time
// failure; the Test I-Ffmpeg variant below covers a genuine FFmpeg-process
// failure.)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn single_failure_is_isolated_and_capacity_is_released() {
    let capacity = machine_capacity();
    if capacity < 2 {
        println!(
            "SKIP: computed machine capacity is {capacity} (sequential) — Test I requires \
             >=2 genuinely concurrent FFmpeg jobs to validate failure isolation under real \
             concurrency"
        );
        return;
    }

    let app = app();
    let root = test_dir("fail_i");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let a = root.join("a.mp4");
    let b = root.join("b.mp4");
    let c = root.join("c.mp4");
    let d = root.join("d.mp4");
    make_test_video(&a, 6, true);
    make_test_video(&b, 6, true);
    make_test_video(&c, 6, true);
    make_test_video(&d, 6, true);

    // Pre-occupy job "a"'s resolved output path with a directory. The render
    // itself succeeds (temp file is written and validated); only the final
    // rename onto the directory fails, which is the intended per-job failure.
    let blocked = out_dir.join("a_9x16.mp4");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::write(blocked.join(".keep"), b"").unwrap();
    let a_str = a.to_string_lossy().to_string();

    let snapshots = record_processing_snapshots(&app);
    let file_events = record_file_status_events(&app);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-i", AspectRatio::Ratio9x16)];
    let inputs = [
        a_str.clone(),
        b.to_string_lossy().to_string(),
        c.to_string_lossy().to_string(),
        d.to_string_lossy().to_string(),
    ];
    start_batch(
        app.handle().clone(),
        manager,
        inputs.to_vec(),
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    // Bounded terminal wait: a scheduler locked up by the failure would trip
    // this timeout and fail the test (deadlock detection).
    let terminal = wait_for_terminal(&app, Duration::from_secs(240)).await;

    // The batch ends Failed because exactly one job failed (roadmap batch
    // tally semantics: failed_jobs > 0 => Failed).
    assert_eq!(terminal.status, BatchStatus::Failed);

    // --- Failure accounting + per-job status isolation --------------------
    assert_eq!(
        terminal.failed_jobs, 1,
        "failed_jobs must equal exactly one failed job, got {}",
        terminal.failed_jobs
    );
    assert_eq!(
        terminal.completed_jobs, 3,
        "the three healthy jobs must complete, got {}",
        terminal.completed_jobs
    );
    assert_eq!(
        terminal.queue.len(),
        4,
        "all four jobs present in the tally"
    );

    let mut failed_entries = 0usize;
    let mut completed_entries = 0usize;
    for f in &terminal.queue {
        match &f.status {
            JobStatus::Failed(reason) => {
                failed_entries += 1;
                assert_eq!(
                    f.file_path, a_str,
                    "only the deliberately failing job may be Failed"
                );
                assert!(
                    !reason.is_empty(),
                    "per-job Failed must carry a real failure reason"
                );
            }
            JobStatus::Completed => completed_entries += 1,
            other => panic!("job {} left non-terminal status {:?}", f.job_id, other),
        }
    }
    assert_eq!(
        failed_entries, 1,
        "exactly one Failed entry in per-job statuses"
    );
    assert_eq!(completed_entries, 3, "three Completed per-job entries");

    // The surviving jobs produced valid, complete outputs.
    for name in ["b_9x16.mp4", "c_9x16.mp4", "d_9x16.mp4"] {
        assert_valid_video(&out_dir.join(name));
    }

    // --- Real concurrency actually occurred -------------------------------
    {
        let guard = snapshots.lock().unwrap();
        assert!(!guard.0.is_empty(), "no batch::progress snapshots captured");
        assert!(
            guard.1.max_processing >= 2,
            "expected ≥2 jobs Processing simultaneously (real concurrency), \
             observed max {}. The failure must not serialise the batch.",
            guard.1.max_processing
        );
    }

    // --- Failure isolation under real concurrency -------------------------
    // Job a must actually have entered the pipeline (Processing) before it
    // emitted Failed — proving the failure went through the real render
    // boundary rather than being cancelled, skipped, or rejected up front.
    {
        let guard = file_events.lock().unwrap();
        let a_started = guard
            .iter()
            .filter(|(p, _)| p.file_path == a_str)
            .filter(|(p, _)| matches!(p.status, JobStatus::Processing))
            .map(|(_, t)| *t)
            .min()
            .expect("job a must emit a Processing event");
        let a_failed = guard
            .iter()
            .filter(|(p, _)| p.file_path == a_str)
            .find(|(p, _)| matches!(p.status, JobStatus::Failed(_)))
            .map(|(_, t)| *t)
            .expect("job a must emit a Failed event");
        assert!(
            a_failed > a_started,
            "job a must be Processing before it reports Failed"
        );
    }

    // The failing job must be in flight concurrently with other work: its
    // Processing interval overlaps at least one other job's interval. At
    // capacity 2 the failing job is either admitted in the first wave
    // (overlapping the other first-wave job) or admitted into the slot released
    // by an earlier finisher (while the remaining first-wave job is still
    // rendering), so this holds for every queue order. This is the observable
    // proof that per-job failure isolation was exercised under genuine
    // concurrency — not sequentially.
    {
        let intervals = busy_intervals(&file_events.lock().unwrap());
        let a_int = *intervals
            .get(&a_str)
            .expect("failing job must yield a Processing interval");
        let overlaps_other_work = intervals.iter().any(|(path, other)| {
            path.as_str() != a_str.as_str() && intervals_overlap(a_int, *other)
        });
        assert!(
            overlaps_other_work,
            "the failing job must fail while another job is concurrently in flight"
        );
    }

    // Universal capacity proof at every capacity: the queue fully drained with
    // exactly 3 Completed + 1 Failed within the bounded terminal wait. If the
    // failed job had permanently held a slot, the queued jobs after it could not
    // all have been admitted (or the batch would still be running), so the
    // timeout already folded into wait_for_terminal would have fired instead.
    assert!(
        terminal.queue.iter().all(|f| f.status != JobStatus::Queued
            && f.status != JobStatus::Pending
            && f.status != JobStatus::Processing),
        "every job must have reached a terminal state; the batch must not leave \
         queued jobs stranded behind a failed job"
    );

    // No orphaned FFmpeg children left behind.
    assert_no_ffmpeg_for_output_dir(&out_dir);

    let _ = std::fs::remove_dir_all(&root);

    println!(
        "Test I verified single-failure isolation at capacity {capacity}: failed_jobs=1, \
         completed=3, failed job's slot released, batch completed without deadlock"
    );
}

// ---------------------------------------------------------------------------
// Test I-Ffmpeg — single GENUINE FFmpeg-process failure.
// The existing Test I fails a job at the Rust-side `finalize_temp_output`
// boundary (the render itself succeeds and only the rename is blocked). This
// variant instead occupies the job's *temporary* output path, so a real FFmpeg
// child is spawned with fully normal arguments and the FFmpeg process itself
// fails to open its output ("Error opening output file .../a_9x16.tmp.mp4:
// Permission denied", non-zero exit). The failure therefore goes through
// `run_ffmpeg`'s `ProcessingFailed` error path — genuine FFmpeg-process
// failure coverage — while every isolation assertion stays identical to
// Test I (one failed, three completed, capacity released, no deadlock, no
// orphaned children, failure overlapped real concurrent work).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn single_ffmpeg_process_failure_is_isolated_and_capacity_is_released() {
    let capacity = machine_capacity();
    if capacity < 2 {
        println!(
            "SKIP: computed machine capacity is {capacity} (sequential) — Test I-Ffmpeg requires \
             >=2 genuinely concurrent FFmpeg jobs to validate failure isolation under real \
             concurrency"
        );
        return;
    }

    let app = app();
    let root = test_dir("fail_if");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let a = root.join("a.mp4");
    let b = root.join("b.mp4");
    let c = root.join("c.mp4");
    let d = root.join("d.mp4");
    make_test_video(&a, 6, true);
    make_test_video(&b, 6, true);
    make_test_video(&c, 6, true);
    make_test_video(&d, 6, true);

    // Pre-occupy job "a"'s TEMP output path with a directory. `render_single`
    // only attempts `remove_file(temp_output_path)` (which silently fails on a
    // directory), so the blocking directory survives and the FFmpeg child
    // itself fails when it tries to open its output file — a genuine
    // FFmpeg-process failure, distinct from Test I's finalize-time failure.
    occupy_dir(&resolve_temp_output_path(&out_dir.join("a_9x16.mp4")));
    let a_str = a.to_string_lossy().to_string();

    let snapshots = record_processing_snapshots(&app);
    let file_events = record_file_status_events(&app);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-if", AspectRatio::Ratio9x16)];
    let inputs = [
        a_str.clone(),
        b.to_string_lossy().to_string(),
        c.to_string_lossy().to_string(),
        d.to_string_lossy().to_string(),
    ];
    start_batch(
        app.handle().clone(),
        manager,
        inputs.to_vec(),
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    // Bounded terminal wait: a scheduler locked up by the failure would trip
    // this timeout and fail the test (deadlock detection).
    let terminal = wait_for_terminal(&app, Duration::from_secs(240)).await;

    // The batch ends Failed because exactly one job failed (roadmap batch
    // tally semantics: failed_jobs > 0 => Failed).
    assert_eq!(terminal.status, BatchStatus::Failed);

    // --- Failure accounting + per-job status isolation --------------------
    assert_eq!(
        terminal.failed_jobs, 1,
        "failed_jobs must equal exactly one failed job, got {}",
        terminal.failed_jobs
    );
    assert_eq!(
        terminal.completed_jobs, 3,
        "the three healthy jobs must complete, got {}",
        terminal.completed_jobs
    );
    assert_eq!(
        terminal.queue.len(),
        4,
        "all four jobs present in the tally"
    );

    let mut failed_entries = 0usize;
    let mut completed_entries = 0usize;
    for f in &terminal.queue {
        match &f.status {
            JobStatus::Failed(reason) => {
                failed_entries += 1;
                assert_eq!(
                    f.file_path, a_str,
                    "only the deliberately failing job may be Failed"
                );
                assert_ffmpeg_process_failure(reason, "a_9x16.tmp.mp4");
            }
            JobStatus::Completed => completed_entries += 1,
            other => panic!("job {} left non-terminal status {:?}", f.job_id, other),
        }
    }
    assert_eq!(
        failed_entries, 1,
        "exactly one Failed entry in per-job statuses"
    );
    assert_eq!(completed_entries, 3, "three Completed per-job entries");

    // The surviving jobs produced valid, complete outputs. The failing job's
    // final path must NOT exist (FFmpeg never wrote any output).
    for name in ["b_9x16.mp4", "c_9x16.mp4", "d_9x16.mp4"] {
        assert_valid_video(&out_dir.join(name));
    }
    assert!(
        !out_dir.join("a_9x16.mp4").exists(),
        "a genuine FFmpeg failure must not produce any final output for the failing job"
    );

    // --- Real concurrency actually occurred -------------------------------
    {
        let guard = snapshots.lock().unwrap();
        assert!(!guard.0.is_empty(), "no batch::progress snapshots captured");
        assert!(
            guard.1.max_processing >= 2,
            "expected ≥2 jobs Processing simultaneously (real concurrency), \
             observed max {}. The failure must not serialise the batch.",
            guard.1.max_processing
        );
    }

    // --- Failure isolation under real concurrency -------------------------
    // Job a must actually have entered the pipeline (Processing) before it
    // emitted Failed — proving the failure went through the real render
    // boundary rather than being cancelled, skipped, or rejected up front.
    {
        let guard = file_events.lock().unwrap();
        let a_started = guard
            .iter()
            .filter(|(p, _)| p.file_path == a_str)
            .filter(|(p, _)| matches!(p.status, JobStatus::Processing))
            .map(|(_, t)| *t)
            .min()
            .expect("job a must emit a Processing event");
        let a_failed = guard
            .iter()
            .filter(|(p, _)| p.file_path == a_str)
            .find(|(p, _)| matches!(p.status, JobStatus::Failed(_)))
            .map(|(_, t)| *t)
            .expect("job a must emit a Failed event");
        assert!(
            a_failed > a_started,
            "job a must be Processing before it reports Failed"
        );
    }

    // The failing job must be in flight concurrently with other work: its
    // Processing interval overlaps at least one other job's interval (see the
    // `busy_intervals` doc comment for why this holds at every queue order).
    {
        let intervals = busy_intervals(&file_events.lock().unwrap());
        let a_int = *intervals
            .get(&a_str)
            .expect("failing job must yield a Processing interval");
        let overlaps_other_work = intervals.iter().any(|(path, other)| {
            path.as_str() != a_str.as_str() && intervals_overlap(a_int, *other)
        });
        assert!(
            overlaps_other_work,
            "the failing job must fail while another job is concurrently in flight"
        );
    }

    // Universal capacity proof at every capacity: the queue fully drained with
    // exactly 3 Completed + 1 Failed within the bounded terminal wait. If the
    // failed job had permanently held a slot, the queued jobs after it could not
    // all have been admitted (or the batch would still be running), so the
    // timeout already folded into wait_for_terminal would have fired instead.
    assert!(
        terminal.queue.iter().all(|f| f.status != JobStatus::Queued
            && f.status != JobStatus::Pending
            && f.status != JobStatus::Processing),
        "every job must have reached a terminal state; the batch must not leave \
         queued jobs stranded behind a failed job"
    );

    // No orphaned FFmpeg children left behind.
    assert_no_ffmpeg_for_output_dir(&out_dir);

    let _ = std::fs::remove_dir_all(&root);

    println!(
        "Test I-Ffmpeg verified single genuine FFmpeg-process failure isolation at capacity \
         {capacity}: failed_jobs=1, completed=3, failed job's slot released, batch completed \
         without deadlock"
    );
}

// ---------------------------------------------------------------------------
// Test J — multiple simultaneous failures across different in-flight jobs.
// Acceptance (roadmap): every failing job executes, fails through the real
// boundary, receives `Failed`, is counted exactly once in `failed_jobs`, and
// releases its capacity; no deadlock, no leaked capacity, no stuck workers;
// the remaining queue keeps processing; final tally is exactly
// `Completed: N, Failed: 2`; at least one failure is a *genuine FFmpeg-process
// failure* and the failures validate multiple distinct failure types.
//
// Two independent jobs (a, b) are in flight at the same time (capacity >= 2).
// Job a fails as a genuine FFmpeg-process failure (its temp output path is
// occupied by a directory, so the FFmpeg child itself cannot open its output
// and exits non-zero). Job b hits the render finalize/rename failure (the
// existing Test I mechanism), so the two failures occur through different
// real code paths on simultaneously in-flight jobs. Both failures overlap in
// real time, not merely sequentially (proven by the interval assertions below,
// which hold for every queue order).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn multiple_simultaneous_failures_no_deadlock_no_capacity_leak() {
    let capacity = machine_capacity();
    if capacity < 2 {
        println!(
            "SKIP: computed machine capacity is {capacity} (sequential) — Test J requires \
             >=2 genuinely concurrent FFmpeg jobs to trigger simultaneous failures across \
             different in-flight jobs"
        );
        return;
    }

    let app = app();
    let root = test_dir("fail_j");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    // Six jobs: a & b fail (through different mechanisms), c..f succeed. The
    // admission order is the nondeterministic completion order of the parallel
    // probe tasks, so genuine simultaneous failures are proven by the interval
    // assertions below rather than by assuming a fixed admission order.
    let names = ["a", "b", "c", "d", "e", "f"];
    let mut inputs = Vec::new();
    let mut paths = std::collections::HashMap::new();
    for name in names {
        let p = root.join(format!("{name}.mp4"));
        make_test_video(&p, 8, true);
        paths.insert(name, p.to_string_lossy().to_string());
        inputs.push(paths[name].clone());
    }

    // Job a fails as a *genuine FFmpeg-process failure*: its temp output path
    // is occupied by a directory, so the FFmpeg child itself fails to open its
    // output and exits non-zero. Job b keeps failing at the Rust-side
    // finalize/rename boundary (final output path blocked). Together the two
    // failures validate multiple distinct failure types occurring on
    // simultaneously in-flight jobs.
    occupy_dir(&resolve_temp_output_path(&out_dir.join("a_9x16.mp4")));
    occupy_dir(&out_dir.join("b_9x16.mp4"));

    let snapshots = record_processing_snapshots(&app);
    let file_events = record_file_status_events(&app);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-j", AspectRatio::Ratio9x16)];
    start_batch(
        app.handle().clone(),
        manager,
        inputs.clone(),
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    // Bounded terminal wait (deadlock => timeout => test failure).
    let terminal = wait_for_terminal(&app, Duration::from_secs(300)).await;
    assert_eq!(terminal.status, BatchStatus::Failed);

    // --- Tally: Completed: 4, Failed: 2 -----------------------------------
    assert_eq!(
        terminal.failed_jobs, 2,
        "failed_jobs must count exactly the two failing jobs, got {}",
        terminal.failed_jobs
    );
    assert_eq!(
        terminal.completed_jobs, 4,
        "four healthy jobs must complete, got {}",
        terminal.completed_jobs
    );
    assert_eq!(terminal.queue.len(), 6, "all six jobs present exactly once");

    let mut failed_entries = 0usize;
    for f in &terminal.queue {
        match &f.status {
            JobStatus::Failed(reason) => {
                failed_entries += 1;
                assert!(
                    f.file_path.as_str() == paths["a"].as_str()
                        || f.file_path.as_str() == paths["b"].as_str(),
                    "only the deliberately failing jobs (a, b) may be Failed, got {}",
                    f.file_path
                );
                if f.file_path.as_str() == paths["a"].as_str() {
                    // Job a must be a genuine FFmpeg-process failure: its
                    // blocked temp output made the FFmpeg child itself fail to
                    // open its output before exiting non-zero.
                    assert_ffmpeg_process_failure(reason, "a_9x16.tmp.mp4");
                } else {
                    // Job b fails at the finalize/rename boundary (existing
                    // Test I mechanism); its reason must still be present, and
                    // must not masquerade as the FFmpeg failure — proving the
                    // two failing jobs failed through different mechanisms.
                    assert!(!reason.is_empty(), "Failed status must carry a reason");
                    assert!(
                        !reason.to_lowercase().contains("a_9x16.tmp.mp4"),
                        "job b's finalize failure must not be reported as job a's FFmpeg \
                         failure (reason: {reason:?})"
                    );
                }
            }
            JobStatus::Completed => {}
            other => panic!("job {} left non-terminal status {:?}", f.job_id, other),
        }
    }
    assert_eq!(failed_entries, 2, "exactly two per-job Failed entries");
    // Healthy jobs must each be exactly Completed (no cross-contamination).
    for name in ["c", "d", "e", "f"] {
        assert!(
            terminal
                .queue
                .iter()
                .any(|f| f.file_path.as_str() == paths[name].as_str()
                    && matches!(f.status, JobStatus::Completed)),
            "job {name} must be Completed, not affected by the failures"
        );
    }
    for name in ["c", "d", "e", "f"] {
        assert_valid_video(&out_dir.join(format!("{name}_9x16.mp4")));
    }

    // --- Both failing jobs were genuinely admitted and concurrency occurred ---
    {
        let guard = snapshots.lock().unwrap();
        assert!(!guard.0.is_empty(), "no batch::progress snapshots captured");
        assert!(
            guard.1.max_processing >= 2,
            "expected ≥2 simultaneous Processing jobs, observed max {}",
            guard.1.max_processing
        );
        // Each failing job must actually have reached the Processing stage
        // (proving it was admitted and ran through the real pipeline with the
        // rest of the concurrent batch) rather than failing before admission.
        for name in ["a", "b"] {
            let p = paths[name].clone();
            let admitted = guard.0.iter().any(|snap| {
                snap.queue
                    .iter()
                    .any(|f| f.file_path == p && matches!(f.status, JobStatus::Processing))
            });
            assert!(
                admitted,
                "failing job {name} must be admitted and Processing in at least one snapshot"
            );
        }
    }

    // --- Simultaneous failures across in-flight jobs ----------------------
    // Each failing job must have entered the pipeline (Processing) before it
    // emitted Failed, so both failures went through the real render boundary.
    // And each failing job's Processing interval must overlap *some other*
    // job's interval — proving the two failures occurred across genuinely
    // concurrent in-flight jobs (not sequentially), for any queue order.
    {
        let intervals = busy_intervals(&file_events.lock().unwrap());
        for name in ["a", "b"] {
            let p = &paths[name];
            let int = intervals
                .get(p.as_str())
                .unwrap_or_else(|| panic!("failing job {name} must yield a Processing interval"));
            assert!(
                int.1 > int.0,
                "failing job {name} must be Processing before it reports Failed"
            );
        }
        // Both failing jobs overlap other in-flight work.
        for name in ["a", "b"] {
            let p = &paths[name];
            let int = *intervals.get(p.as_str()).expect("failing-job interval");
            let overlaps_other = intervals
                .iter()
                .any(|(path, other)| path.as_str() != p.as_str() && intervals_overlap(int, *other));
            assert!(
                overlaps_other,
                "failing job {name} must fail while another job is concurrently in flight"
            );
        }
    }

    // Universal capacity proof at every capacity: the queue fully drained with
    // exactly 4 Completed + 2 Failed within the bounded terminal wait. If either
    // failed job had permanently held a slot, some of c..f could not have been
    // admitted and the batch would still be running when wait_for_terminal timed
    // out.
    assert!(
        terminal.queue.iter().all(|f| f.status != JobStatus::Queued
            && f.status != JobStatus::Pending
            && f.status != JobStatus::Processing),
        "every job must have reached a terminal state; no job may remain stranded \
         behind a failed job"
    );

    // No deadlock / no stuck workers: every job reached a terminal state above;
    // no orphaned FFmpeg children remain.
    assert_no_ffmpeg_for_output_dir(&out_dir);

    let _ = std::fs::remove_dir_all(&root);

    println!(
        "Test J verified {failed_entries} simultaneous failures (1 genuine FFmpeg-process + \
         1 finalize/rename) at capacity {capacity}: completed=4, failed=2, all failed slots \
         released, queue drained, no deadlock"
    );
}

/// Enumerates command lines of every running `ffmpeg.exe`.
fn running_ffmpeg_commandlines() -> Vec<String> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name = 'ffmpeg.exe'\" | ForEach-Object { $_.CommandLine }",
        ])
        .output()
        .expect("failed to enumerate ffmpeg processes via PowerShell");
    assert!(
        out.status.success(),
        "PowerShell process enumeration failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Asserts that no running FFmpeg process has this batch's output directory in
/// its command line (case-insensitively), verifying no child was orphaned by
/// cancellation. The check filters on the test's own directory so it stays
/// correct even when other tests run FFmpeg concurrently.
fn assert_no_ffmpeg_for_output_dir(out_dir: &std::path::Path) {
    let needle = out_dir.to_string_lossy().to_lowercase();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let residue: Vec<String> = running_ffmpeg_commandlines()
            .into_iter()
            .filter(|cmd| cmd.to_lowercase().contains(&needle))
            .collect();
        if residue.is_empty() {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "FFmpeg children still running against the cancelled batch: {residue:#?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
