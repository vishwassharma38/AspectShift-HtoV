//! Roadmap Stage 3.3 — live disk-space admission gating (Test L from the
//! Testing & Validation Matrix, plus the supporting A–D checks).
//!
//! The Stage 3.3 gate checks free space against a conservative safety margin
//! (`DISK_SAFETY_MARGIN_BYTES`) BEFORE the scheduler dispatches a new job.
//! Simulated low/insufficient disk must therefore produce: a clear "disk
//! space" error, no corrupted final outputs, no stranded temporary files, and
//! no infinite retry loop. Already-admitted jobs are left to finish; the
//! moment the gate fails, admission stops and every remaining queued job is
//! failed with the identical disk-space reason.
//!
//! Disk space is injected through `BatchManager::with_disk_source`, so these
//! tests run the REAL production seam (`start_batch` → scheduler dispatcher →
//! `DiskAdmissionGate`) with real FFmpeg while keeping the reported free space
//! fully deterministic:
//!
//! - **Test A** — healthy disk: a normal batch completes end to end against the
//!   real `SystemDiskSpaceSource`, finals are valid, no temp leftovers.
//! - **Test B** — low disk at batch start: nothing is admitted, every job is
//!   failed with the clear disk-space reason, zero outputs exist, no temp files
//!   appear, the batch terminates immediately (no retry spin).
//! - **Test C** — mid-batch exhaustion: exactly the two admitted jobs finish
//!   as valid finals, the remaining jobs fail with the disk-space reason, one
//!   live probe per pop (2 healthy + 1 failing = 3 probes, no re-probing, no
//!   re-running), no temp leftovers.
//! - **Test D** — `finalize_temp_output` must not strand a completed temp
//!   artifact when the final-directory rename fails (covers the Stage 3.3
//!   "no corrupted final output files" + "temp files are cleaned up" promise
//!   at the failure boundary).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use aspectshift_htov_lib::video::batch_processor::{get_batch_status, start_batch};
use aspectshift_htov_lib::video::concurrency::{DiskSpaceSource, DISK_SAFETY_MARGIN_BYTES};
use aspectshift_htov_lib::video::paths::resolve_temp_output_path;
use aspectshift_htov_lib::video::queue::BatchManager;
use aspectshift_htov_lib::video::types::{
    AspectRatio, BatchJobSettings, BatchProgress, BatchStatus, EncodingProfile, FileProgress,
    JobStatus, OutputJob, SelectionMetadata, SubtitleOverlaySettings, TargetType,
    VideoEffectsSettings,
};
use tauri::{Listener, Manager};

const GB: u64 = 1024 * 1024 * 1024;
const FIVE_GB: u64 = 5 * GB;

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

/// 640x360 landscape clip (forces a real libx264 encode, since horizontal
/// input is never passthrough for portrait targets).
fn make_test_video(path: &Path, duration_secs: u32) {
    let args: Vec<String> = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("testsrc=duration={}:size=640x360:rate=30", duration_secs),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        path.to_string_lossy().to_string(),
    ];
    let joined: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    ffmpeg_ok(&joined);
}

/// Injected Stage 3.3 disk source: admission call `i` reports `script[i]`;
/// past the end of the script it reports a healthy value (above the margin).
/// The call counter proves exactly how many live probes the gate performed.
#[derive(Debug)]
struct ScriptedDiskSource {
    script: Vec<u64>,
    calls: Arc<AtomicUsize>,
}

impl ScriptedDiskSource {
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

impl DiskSpaceSource for ScriptedDiskSource {
    fn available_bytes(&self, _path: &Path) -> std::io::Result<u64> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .script
            .get(index)
            .copied()
            .unwrap_or(DISK_SAFETY_MARGIN_BYTES + 1))
    }
}

/// A manager pre-wired with a scripted disk source, returned alongside the
/// source so tests can read its live probe count.
fn manager_with_script(script: Vec<u64>) -> (BatchManager, Arc<ScriptedDiskSource>) {
    let source = Arc::new(ScriptedDiskSource::new(script));
    let gate_source: Arc<dyn DiskSpaceSource> = source.clone();
    (BatchManager::with_disk_source(gate_source), source)
}

fn app_with_manager(manager: BatchManager) -> tauri::App {
    tauri::Builder::default()
        .any_thread()
        .plugin(tauri_plugin_shell::init())
        .manage(manager)
        .build(tauri::generate_context!())
        .expect("failed to build tauri app")
}

fn app() -> tauri::App {
    app_with_manager(BatchManager::new())
}

fn test_dir(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("stage3_3_{label}_{}", uuid::Uuid::new_v4()));
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

fn output_job(id: &str) -> OutputJob {
    OutputJob {
        id: id.to_string(),
        ratio: AspectRatio::Ratio9x16,
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

/// Occupies `path` with a non-empty directory so a later attempt to treat it
/// as a file (e.g. the post-encode rename target in `finalize_temp_output`)
/// fails.
fn occupy_dir(path: &Path) {
    std::fs::create_dir(path).unwrap_or_else(|e| panic!("create_dir {}: {e}", path.display()));
    std::fs::write(path.join(".keep"), b"").unwrap();
}

/// Tracks the max number of jobs simultaneously `Processing` across every
/// `batch://progress` snapshot.
fn record_processing_snapshots(app: &tauri::App) -> Arc<StdMutex<usize>> {
    let recorder: Arc<StdMutex<usize>> = Arc::new(StdMutex::new(0));
    {
        let cap = recorder.clone();
        app.listen("batch://progress", move |e| {
            if let Ok(p) = serde_json::from_str::<BatchProgress>(e.payload()) {
                let processing = p
                    .queue
                    .iter()
                    .filter(|f| matches!(&f.status, JobStatus::Processing))
                    .count();
                let mut guard = cap.lock().unwrap();
                if processing > *guard {
                    *guard = processing;
                }
            }
        });
    }
    recorder
}

/// Timestamped per-job `batch://file-status` events; used to prove each
/// admitted job was processed exactly once (no retry loop).
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

fn no_temp_leftovers(out_dir: &Path) -> Vec<String> {
    let mut leftover: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(out_dir).expect("read out dir") {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        if name.contains(".tmp.") {
            leftover.push(name);
        }
    }
    leftover
}

fn assert_no_temp_leftovers(out_dir: &Path) {
    let leftover = no_temp_leftovers(out_dir);
    assert!(
        leftover.is_empty(),
        "temporary artifacts must be cleaned up, found: {leftover:?}"
    );
}

fn assert_out_dir_empty(out_dir: &Path) {
    let names: Vec<String> = std::fs::read_dir(out_dir)
        .expect("read out dir")
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(names.is_empty(), "no outputs may exist: {names:?}");
}

/// Collects the failed reasons carried by `progress.queue` and asserts they all
/// name the disk-space gate (the "clear error" half of Test L).
fn assert_all_failed_for_disk_space(progress: &BatchProgress, expected_failed: usize) {
    let mut failed_reasons: Vec<String> = Vec::new();
    for f in &progress.queue {
        if let JobStatus::Failed(reason) = &f.status {
            failed_reasons.push(reason.clone());
        }
    }
    assert_eq!(
        failed_reasons.len(),
        expected_failed,
        "expected {expected_failed} per-job disk failures"
    );
    for reason in &failed_reasons {
        assert!(
            reason.contains("disk space"),
            "blocked job must carry a clear disk-space reason: {reason}"
        );
    }
}

/// Test A — healthy disk, real system source.
#[tokio::test]
async fn test_a_healthy_batch_completes_with_system_disk_source() {
    let app = app();
    let root = test_dir("disk_a");
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let a1 = root.join("a1.mp4");
    let a2 = root.join("a2.mp4");
    make_test_video(&a1, 3);
    make_test_video(&a2, 3);
    let recorder = record_processing_snapshots(&app);

    let manager = app.state::<BatchManager>();
    start_batch(
        app.handle().clone(),
        manager,
        vec![
            a1.to_string_lossy().to_string(),
            a2.to_string_lossy().to_string(),
        ],
        settings(&out, vec![output_job("job-a")]),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(180)).await;
    assert_eq!(terminal.status, BatchStatus::Completed);
    assert_eq!(terminal.completed_jobs, 2);
    assert_eq!(terminal.failed_jobs, 0);
    assert_valid_video(&out.join("a1_9x16.mp4"));
    assert_valid_video(&out.join("a2_9x16.mp4"));
    assert_no_temp_leftovers(&out);
    assert!(
        *recorder.lock().unwrap() >= 1,
        "healthy batch must actually admit and run work"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Test B — free space at/below the margin when the batch starts. Nothing may
/// be admitted: zero executions, zero outputs, zero temp files, an immediate
/// terminal state with a clear disk-space reason on every job, and exactly one
/// probe (no repeated probing / no retry spin).
#[tokio::test]
async fn test_b_low_disk_at_start_fails_all_with_clear_error() {
    let (manager, source) = manager_with_script(vec![0; 8]);
    let app = app_with_manager(manager);
    let root = test_dir("disk_b");
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let vb = [
        root.join("b1.mp4"),
        root.join("b2.mp4"),
        root.join("b3.mp4"),
    ];
    for f in &vb {
        make_test_video(f, 3);
    }
    let recorder = record_processing_snapshots(&app);

    let manager = app.state::<BatchManager>();
    start_batch(
        app.handle().clone(),
        manager,
        vb.iter()
            .map(|f| f.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        settings(&out, vec![output_job("job-b")]),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(60)).await;
    assert_eq!(terminal.status, BatchStatus::Failed);
    assert_eq!(terminal.completed_jobs, 0, "nothing may complete");
    assert_eq!(terminal.failed_jobs, 3, "every queued job is failed");
    assert_eq!(
        source.call_count(),
        1,
        "halting on the first failed probe must not keep querying"
    );
    assert_eq!(
        *recorder.lock().unwrap(),
        0,
        "no job may reach Processing while free space is below the safety margin"
    );
    assert_out_dir_empty(&out);
    assert_no_temp_leftovers(&out);
    assert_all_failed_for_disk_space(&terminal, 3);

    let _ = std::fs::remove_dir_all(&root);
}

/// Test C — the batch starts healthy, then free space collapses mid-batch.
/// Exactly the two already-admitted jobs finish as valid finals; the rest are
/// failed with the disk-space reason. Admission ran one live probe per pop
/// (2 healthy + 1 failing = 3) and no job ever started more than once.
#[tokio::test]
async fn test_c_mid_batch_exhaustion_admitted_jobs_finish_and_rest_fail() {
    let (manager, source) = manager_with_script(vec![
        FIVE_GB,                  // job 1 admitted
        FIVE_GB,                  // job 2 admitted
        DISK_SAFETY_MARGIN_BYTES, // job 3 fails the gate (== margin is insufficient)
    ]);
    let app = app_with_manager(manager);
    let root = test_dir("disk_c");
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let vc = [
        root.join("c1.mp4"),
        root.join("c2.mp4"),
        root.join("c3.mp4"),
        root.join("c4.mp4"),
    ];
    for f in &vc {
        make_test_video(f, 3);
    }
    let events = record_file_status_events(&app);

    let manager = app.state::<BatchManager>();
    start_batch(
        app.handle().clone(),
        manager,
        vc.iter()
            .map(|f| f.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        settings(&out, vec![output_job("job-c")]),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(180)).await;
    assert_eq!(terminal.status, BatchStatus::Failed);
    assert_eq!(
        terminal.completed_jobs, 2,
        "exactly the two admitted jobs must finish"
    );
    assert_eq!(terminal.failed_jobs, 2, "the gate fails the rest");
    assert_eq!(
        source.call_count(),
        3,
        "one live probe per admission pop: 2 healthy + 1 failing; no re-probing"
    );
    assert_all_failed_for_disk_space(&terminal, 2);

    // Exactly two final outputs, both valid video.
    let finals: Vec<String> = std::fs::read_dir(&out)
        .expect("read out dir")
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with("_9x16.mp4"))
        .collect();
    assert_eq!(
        finals.len(),
        2,
        "only the admitted jobs may leave finals: {finals:?}"
    );
    for f in &finals {
        assert_valid_video(&out.join(f));
    }
    // No corrupted/partial/temp artifacts anywhere.
    assert_no_temp_leftovers(&out);

    // Retry-isolation proof: each input was Processing exactly once.
    let events_guard = events.lock().unwrap();
    let mut processing_counts: HashMap<String, usize> = HashMap::new();
    for (p, _) in events_guard.iter() {
        if matches!(&p.status, JobStatus::Processing) {
            *processing_counts.entry(p.file_path.clone()).or_insert(0) += 1;
        }
    }
    assert_eq!(
        processing_counts.len(),
        2,
        "exactly the two admitted jobs started: {processing_counts:?}"
    );
    assert!(
        processing_counts.values().all(|&count| count == 1),
        "no job may be started more than once (no retry loop): {processing_counts:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Test D — `finalize_temp_output` must not strand a completed temp artifact:
/// when the rename onto the final path fails, the temp file is removed rather
/// than silently left to consume disk / masquerade as a completed output.
#[tokio::test]
async fn test_d_failed_rename_strand_cleans_up_temp_output() {
    let app = app();
    let root = test_dir("disk_d");
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let d = root.join("d.mp4");
    make_test_video(&d, 3);

    // Occupy the FINAL output path: FFmpeg renders the temp successfully, then
    // `finalize_temp_output`'s rename fails because the target is a directory.
    let final_path = out.join("d_9x16.mp4");
    occupy_dir(&final_path);
    let _temp_marker = resolve_temp_output_path(&final_path);

    let manager = app.state::<BatchManager>();
    start_batch(
        app.handle().clone(),
        manager,
        vec![d.to_string_lossy().to_string()],
        settings(&out, vec![output_job("job-d")]),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(180)).await;
    assert_eq!(terminal.status, BatchStatus::Failed);
    assert_eq!(terminal.failed_jobs, 1);
    assert_eq!(terminal.completed_jobs, 0);
    // The temp file that FFmpeg produced must have been cleaned up on the
    // failed rename (it would otherwise be a full-length stray left behind).
    assert_no_temp_leftovers(&out);

    let _ = std::fs::remove_dir_all(&root);
}

/// Test L (matrix) — simulated low/no disk space end to end: clear error, no
/// corrupt output, temp files cleaned, and the batch terminates with no
/// infinite retry loop (the strict per-job single-admission + probe-count
/// proofs live in Tests B/C above; this walks the full matrix expectations in
/// one place).
#[tokio::test]
async fn test_l_simulated_low_disk_surfaces_clear_error_and_terminates_cleanly() {
    let (manager, source) = manager_with_script(vec![0, 0, 0]);
    let app = app_with_manager(manager);
    let root = test_dir("disk_l");
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let vl = [
        root.join("l1.mp4"),
        root.join("l2.mp4"),
        root.join("l3.mp4"),
    ];
    for f in &vl {
        make_test_video(f, 3);
    }

    let manager = app.state::<BatchManager>();
    let started_at = std::time::Instant::now();
    start_batch(
        app.handle().clone(),
        manager,
        vl.iter()
            .map(|f| f.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        settings(&out, vec![output_job("job-l")]),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(60)).await;
    assert_eq!(terminal.status, BatchStatus::Failed);
    assert_eq!(terminal.failed_jobs, 3);
    assert_eq!(terminal.completed_jobs, 0);
    assert_eq!(
        source.call_count(),
        1,
        "a single failed probe halts admission"
    );
    assert_all_failed_for_disk_space(&terminal, 3);
    assert!(
        started_at.elapsed() < Duration::from_secs(30),
        "a fully blocked batch must terminate promptly, not spin"
    );
    assert_out_dir_empty(&out);
    assert_no_temp_leftovers(&out);

    let _ = std::fs::remove_dir_all(&root);
}
