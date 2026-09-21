//! Stage 0.5 regression matrix: exercises the real runtime pipeline
//! (real Tauri app, tauri-plugin-shell sidecars, real ffmpeg) against the
//! v0.1.2 behavior locked in by Stages 0.1-0.4. No parallel processing is
//! introduced here; each batch is strictly sequential.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use aspectshift_htov_lib::video::batch_processor::{get_batch_status, start_batch};
use aspectshift_htov_lib::video::queue::BatchManager;
use aspectshift_htov_lib::video::types::{
    AspectRatio, BatchJobSettings, BatchProgress, BatchStatus, EncodingProfile, FileProgress,
    JobStatus, OutputJob, SelectionMetadata, SubtitleOverlaySettings, TargetType,
    VideoEffectsSettings,
};
use tauri::{Listener, Manager};

// ---------------------------------------------------------------------------
// Harness helpers
// ---------------------------------------------------------------------------

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
/// encode for every 9:16 target, since horizontal input is never passthrough).
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

/// Audio-only mp4 (no video stream): an "invalid/incompatible" input that
/// fails the probe stage but must not abort the rest of the batch.
fn make_audio_only(path: &Path) {
    let args = [
        "-y",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=220:duration=3",
        "-c:a",
        "aac",
        &path.to_string_lossy(),
    ];
    ffmpeg_ok(&args);
}

/// Renders a pre-existing, valid output for skip-existing tests.
fn pre_render_output(input: &Path, output: &Path) {
    let args = [
        "-y",
        "-i",
        &input.to_string_lossy(),
        "-vf",
        "scale=720:1280",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        &output.to_string_lossy(),
    ];
    ffmpeg_ok(&args);
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
    let root = std::env::temp_dir().join(format!("stage0_5_{label}_{}", uuid::Uuid::new_v4()));
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

fn effects_with_subtitles() -> VideoEffectsSettings {
    let mut e = base_effects();
    e.export_subtitles = Some(true);
    e.burn_subtitles = Some(true);
    e
}

fn effects_with_skip() -> VideoEffectsSettings {
    let mut e = base_effects();
    e.skip_existing = Some(true);
    e
}

#[allow(clippy::too_many_arguments)]
fn output_job(id: &str, ratio: AspectRatio, effects: VideoEffectsSettings) -> OutputJob {
    OutputJob {
        id: id.to_string(),
        ratio,
        encoding: EncodingProfile::standard(),
        effects,
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

fn assert_completed(progress: &BatchProgress, expected_total: usize) {
    assert_eq!(progress.status, BatchStatus::Completed);
    assert_eq!(progress.completed_jobs, expected_total);
    assert_eq!(progress.failed_jobs, 0);
}

fn collect_outputs(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    out.sort();
    out
}

fn file_bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// A: single video renders correctly (real ffmpeg encode), progress reaches 100,
// and batch://progress / batch://file-status events are emitted.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn single_video_renders_with_progress_and_events() {
    let app = app();
    let root = test_dir("single");
    let input = root.join("in.mp4");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    make_test_video(&input, 3, true);

    let progress_events: Arc<StdMutex<Vec<BatchProgress>>> = Arc::new(StdMutex::new(Vec::new()));
    let file_events: Arc<StdMutex<Vec<FileProgress>>> = Arc::new(StdMutex::new(Vec::new()));
    {
        let cap = progress_events.clone();
        app.listen("batch://progress", move |e| {
            if let Ok(p) = serde_json::from_str::<BatchProgress>(e.payload()) {
                cap.lock().unwrap().push(p);
            }
        });
        let cap = file_events.clone();
        app.listen("batch://file-status", move |e| {
            if let Ok(p) = serde_json::from_str::<FileProgress>(e.payload()) {
                cap.lock().unwrap().push(p);
            }
        });
    }

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-a", AspectRatio::Ratio9x16, base_effects())];
    start_batch(
        app.handle().clone(),
        manager,
        vec![input.to_string_lossy().to_string()],
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(120)).await;
    assert_eq!(terminal.status, BatchStatus::Completed);
    assert_eq!(terminal.completed_jobs, 1);
    assert_eq!(terminal.failed_jobs, 0);

    let expected = out_dir.join("in_9x16.mp4");
    assert!(
        expected.exists(),
        "expected output at {}",
        expected.display()
    );
    assert!(std::fs::metadata(&expected).unwrap().len() > 0);
    let probe = probe_video(&expected);
    assert!(
        probe.contains("duration"),
        "output is not a valid video: {}",
        probe
    );

    {
        let snaps = progress_events.lock().unwrap();
        assert!(!snaps.is_empty(), "no batch://progress events captured");
        let last = snaps.last().unwrap();
        assert_eq!(last.status, BatchStatus::Completed);
        assert_eq!(last.percentage, 100.0);
        assert!(
            snaps
                .iter()
                .any(|s| s.percentage > 0.0 && s.percentage < 100.0),
            "expected at least one intermediate progress value"
        );
        assert!(
            snaps.iter().any(|s| s.percentage >= 50.0),
            "progress never reached 50%"
        );
    }
    {
        let files = file_events.lock().unwrap();
        let completed = files
            .iter()
            .filter(|f| matches!(f.status, JobStatus::Completed))
            .count();
        assert_eq!(completed, 1, "expected exactly one Completed file-status");
    }

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// B: multiple videos process correctly with tallies, progress and outputs
// valid, and the number of simultaneously-Processing jobs never exceeds the
// machine's computed concurrency capacity. Before Stage 2.3 capacity was
// forced to 1, so this also verified strict sequentiality; Stage 2.3 enables
// real concurrency, so the invariant is the capacity bound, not "<= 1".
// ---------------------------------------------------------------------------
#[tokio::test]
async fn multiple_videos_process_without_exceeding_capacity() {
    use aspectshift_htov_lib::video::concurrency::{
        calculate_safe_concurrency, detect_system_resources,
    };
    let capacity = calculate_safe_concurrency(&detect_system_resources()).total_capacity;

    let app = app();
    let root = test_dir("multi");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut inputs = Vec::new();
    let durations = [2u32, 3, 4];
    for (i, d) in durations.iter().enumerate() {
        let p = root.join(format!("clip{i}.mp4"));
        make_test_video(&p, *d, true);
        inputs.push(p);
    }

    let progress_events: Arc<StdMutex<Vec<BatchProgress>>> = Arc::new(StdMutex::new(Vec::new()));
    {
        let cap = progress_events.clone();
        app.listen("batch://progress", move |e| {
            if let Ok(p) = serde_json::from_str::<BatchProgress>(e.payload()) {
                cap.lock().unwrap().push(p);
            }
        });
    }

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-b", AspectRatio::Ratio9x16, base_effects())];
    let input_strs: Vec<String> = inputs
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    start_batch(
        app.handle().clone(),
        manager,
        input_strs,
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(240)).await;
    assert_completed(&terminal, 3);

    let outputs = collect_outputs(&out_dir);
    assert_eq!(outputs.len(), 3, "expected three outputs: {outputs:?}");
    for out in &outputs {
        let probe = probe_video(out);
        assert!(
            probe.contains("duration"),
            "invalid output {}",
            out.display()
        );
    }

    {
        let snaps = progress_events.lock().unwrap();
        assert!(!snaps.is_empty());
        let last = snaps.last().unwrap();
        assert_eq!(last.status, BatchStatus::Completed);
        assert_eq!(last.percentage, 100.0);
        // Capacity bound: no snapshot may show more Processing jobs than the
        // machine's computed total_capacity permits.
        for snap in snaps.iter() {
            let processing = snap
                .queue
                .iter()
                .filter(|f| matches!(f.status, JobStatus::Processing))
                .count();
            assert!(
                processing <= capacity,
                "more than {capacity} jobs Processing in a single snapshot ({processing})"
            );
        }
        assert_eq!(last.completed_jobs, 3);
    }

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// C: cancellation mid-render cancels the batch cleanly and the manager can run
// a fresh batch afterwards.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn cancellation_mid_render_allows_subsequent_batch() {
    let app = app();
    let root = test_dir("cancel");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let long_path = root.join("long.mp4");
    make_test_video(&long_path, 300, false);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-c", AspectRatio::Ratio9x16, base_effects())];
    start_batch(
        app.handle().clone(),
        manager,
        vec![long_path.to_string_lossy().to_string()],
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    // Wait until the render has actually started, then cancel.
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        let manager = app.state::<BatchManager>();
        let progress = get_batch_status(manager).await.unwrap();
        if progress.status == BatchStatus::Processing && progress.current_job_id.is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "batch never entered Processing with an active job"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let manager = app.state::<BatchManager>();
    manager.cancel().await;

    let terminal = wait_for_terminal(&app, Duration::from_secs(30)).await;
    assert_eq!(terminal.status, BatchStatus::Cancelled);

    // Manager must accept and complete a fresh batch afterwards.
    let small = root.join("small.mp4");
    make_test_video(&small, 2, false);
    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-c2", AspectRatio::Ratio9x16, base_effects())];
    start_batch(
        app.handle().clone(),
        manager,
        vec![small.to_string_lossy().to_string()],
        settings(&out_dir, targets),
    )
    .await
    .expect("second start_batch failed");

    let terminal2 = wait_for_terminal(&app, Duration::from_secs(120)).await;
    assert_completed(&terminal2, 1);

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// D1: invalid/incompatible input (audio-only file) does not abort the batch;
// it is reported as Failed and valid jobs still complete.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn invalid_input_does_not_abort_batch() {
    let app = app();
    let root = test_dir("invalid");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let valid = root.join("valid.mp4");
    let audio_only = root.join("audio_only.mp4");
    make_test_video(&valid, 2, true);
    make_audio_only(&audio_only);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-d", AspectRatio::Ratio9x16, base_effects())];
    let inputs = [
        valid.to_string_lossy().to_string(),
        audio_only.to_string_lossy().to_string(),
    ];
    start_batch(
        app.handle().clone(),
        manager,
        inputs.to_vec(),
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(120)).await;
    assert_completed(&terminal, 1);
    assert_eq!(
        terminal.failed_jobs, 0,
        "probe-rejected inputs must not count as failed jobs (baseline behavior)"
    );

    assert!(
        out_dir.join("valid_9x16.mp4").exists(),
        "valid input should have produced an output"
    );
    assert!(
        !out_dir.join("audio_only_9x16.mp4").exists(),
        "audio-only input must not be converted"
    );

    // Baseline behavior: probe-rejected inputs are not surfaced in the queue
    // (never added to all_job_ids). Only D2 tests the surfaced-failure path.
    let manager = app.state::<BatchManager>();
    let progress = get_batch_status(manager).await.unwrap();
    assert_eq!(
        progress.queue.len(),
        1,
        "queue should only contain the valid job"
    );

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// D2: a genuine render failure counts toward failed_jobs and the batch ends in
// the Failed state (output directory is actually a file -> render error).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn render_failure_marks_batch_failed() {
    let app = app();
    let root = test_dir("renderfail");
    let input = root.join("in.mp4");
    make_test_video(&input, 2, false);

    // Make the "output directory" a regular file so rendering cannot write.
    let out_blocker = root.join("out");
    std::fs::write(&out_blocker, b"not a directory").unwrap();

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-d2", AspectRatio::Ratio9x16, base_effects())];
    start_batch(
        app.handle().clone(),
        manager,
        vec![input.to_string_lossy().to_string()],
        settings(&out_blocker, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(120)).await;
    assert_eq!(terminal.status, BatchStatus::Failed);
    assert_eq!(terminal.completed_jobs, 0);
    assert_eq!(terminal.failed_jobs, 1);

    let manager = app.state::<BatchManager>();
    let progress = get_batch_status(manager).await.unwrap();
    assert!(
        progress
            .queue
            .iter()
            .any(|f| matches!(&f.status, JobStatus::Failed(reason) if !reason.is_empty())),
        "expected a job with a non-empty failure reason"
    );

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// E: skip_existing leaves the existing valid output untouched and continues
// with the remaining jobs.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn skip_existing_preserves_output_and_continues() {
    let app = app();
    let root = test_dir("skip");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let existing_input = root.join("existing.mp4");
    let render_input = root.join("render.mp4");
    make_test_video(&existing_input, 2, true);
    make_test_video(&render_input, 2, true);

    let pre_existing_output = out_dir.join("existing_9x16.mp4");
    pre_render_output(&existing_input, &pre_existing_output);
    let before_bytes = file_bytes(&pre_existing_output);
    let before_len = std::fs::metadata(&pre_existing_output).unwrap().len();
    assert!(before_len > 0);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job(
        "job-e",
        AspectRatio::Ratio9x16,
        effects_with_skip(),
    )];
    let inputs = [
        existing_input.to_string_lossy().to_string(),
        render_input.to_string_lossy().to_string(),
    ];
    start_batch(
        app.handle().clone(),
        manager,
        inputs.to_vec(),
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(120)).await;
    assert_completed(&terminal, 2);

    // Existing output untouched; the new one was actually rendered.
    assert_eq!(
        file_bytes(&pre_existing_output),
        before_bytes,
        "existing output was modified despite skip_existing"
    );
    assert!(
        out_dir.join("render_9x16.mp4").exists(),
        "non-existing input should still be rendered"
    );

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// F: subtitle export (SRT) and burn-in run against the real pipeline with a
// stubbed whisper transcription (real ffmpeg audio extraction + ASS burn-in).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn subtitles_export_and_burn_with_stubbed_whisper() {
    let app = app();
    let root = test_dir("subs");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    install_whisper_stub(&app)
        .await
        .expect("whisper stub install");

    let input = root.join("speech.mp4");
    make_test_video(&input, 4, true);

    let manager = app.state::<BatchManager>();
    let targets = vec![output_job(
        "job-f",
        AspectRatio::Ratio9x16,
        effects_with_subtitles(),
    )];
    start_batch(
        app.handle().clone(),
        manager,
        vec![input.to_string_lossy().to_string()],
        settings(&out_dir, targets),
    )
    .await
    .expect("start_batch failed");

    let terminal = wait_for_terminal(&app, Duration::from_secs(240)).await;
    assert_completed(&terminal, 1);

    // SRT export next to the output.
    let srt = out_dir.join("speech.srt");
    assert!(srt.exists(), "expected SRT export at {}", srt.display());
    let srt_text = std::fs::read_to_string(&srt).unwrap();
    assert!(
        srt_text.contains("Hello world from the stub transcriber"),
        "SRT missing stubbed transcription: {srt_text}"
    );
    assert!(srt_text.contains("-->"), "malformed SRT: {srt_text}");

    // Burned-in video present and valid.
    let output = out_dir.join("speech_9x16_subtitles.mp4");
    assert!(
        output.exists(),
        "expected burned-in output at {}; dir contains: {:?}",
        output.display(),
        collect_outputs(&out_dir)
    );
    let probe = probe_video(&output);
    assert!(
        probe.contains("duration"),
        "invalid burned-in output: {probe}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

async fn install_whisper_stub(app: &tauri::App) -> Result<(), String> {
    let runtime =
        aspectshift_htov_lib::runtime_paths::RuntimePaths::from_app(&app.handle().clone())
            .map_err(|e| e.to_string())?;
    let bin_dir = runtime.dependency_current_dir("whisper");
    let model_dir = runtime.model_current_dir("whisper");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let stub = std::path::PathBuf::from(env!("CARGO_BIN_EXE_whisper_stub"));
    let dest = bin_dir
        .join(aspectshift_htov_lib::runtime_paths::RuntimePaths::whisper_binary_default_filename());
    std::fs::copy(&stub, &dest).map_err(|e| e.to_string())?;

    let model = model_dir.join("ggml-medium.en.bin");
    std::fs::write(&model, b"stub-model").map_err(|e| e.to_string())?;
    Ok(())
}
