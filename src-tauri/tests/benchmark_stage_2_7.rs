//! Roadmap Stage 2.7 — Benchmark representative workloads before finalizing
//! Stage 1.3 defaults and the `ffmpeg_threads_per_job` hint.
//!
//! The benchmark measures **total batch wall-clock time** (start of the batch
//! call through the terminal batch state) for the real production pipeline
//! (`start_batch` -> Stage 2.1 scheduler -> `process_batch_job` ->
//! `render_single` -> FFmpeg) at forced `total_capacity = 1, 2, 3, 4` on
//! representative workloads executed at their native resolutions (720p,
//! 1080p, 4K). Only the capacity is forced — via the benchmark-only
//! `ASPECTSHIFT_BENCH_TOTAL_CAPACITY` env hook — everything else is the exact
//! path a normal user batch takes.
//!
//! Synthetic media generation time is excluded from every measurement; the
//! exact generation command is documented in the Stage 2.7 report.
//!
//! # Running
//!
//! The full matrix is slow (many real FFmpeg encodes per cell), and the
//! capacity override env var is process-global, so the benchmark below is
//! `#[ignore]`d. Run it explicitly:
//!
//! ```text
//! cargo test --test benchmark_stage_2_7 -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! Optional controls: `BENCH_RUNS` (repetitions per cell, default 1) and
//! `BENCH_CAPACITIES` (comma list, default `1,2,3,4`).
//!
//! The non-ignored tests
//! (`forced_capacities_are_accepted_and_respected_by_the_real_path` and
//! `invalid_benchmark_override_values_fall_back_safely`) run with the normal
//! suite and serialize on a process-global lock because every test here reads
//! the same process-global override env var.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use aspectshift_htov_lib::video::batch_processor::{get_batch_status, start_batch};
use aspectshift_htov_lib::video::concurrency::{
    calculate_safe_concurrency, detect_system_resources, MAX_TOTAL_CAPACITY,
};
use aspectshift_htov_lib::video::queue::BatchManager;
use aspectshift_htov_lib::video::types::{
    AspectRatio, BatchJobSettings, BatchProgress, BatchStatus, EncodingProfile, JobStatus,
    OutputJob, SelectionMetadata, SubtitleOverlaySettings, TargetType, VideoEffectsSettings,
};
use tauri::{Listener, Manager};

const CAPACITY_ENV: &str = "ASPECTSHIFT_BENCH_TOTAL_CAPACITY";
const RUNS_ENV: &str = "BENCH_RUNS";
const CAPACITIES_ENV: &str = "BENCH_CAPACITIES";

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
        "benchmark_stage2_7_{label}_{}",
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

/// Synthetic clip at its native resolution with a real H.264/AAC encode (sine
/// audio forces the real `-map 0:a? -c:a aac` path through every conversion).
fn make_clip(path: &Path, width: u32, height: u32, duration_secs: u32) {
    let mut args: Vec<String> = Vec::new();
    args.push("-y".into());
    args.push("-f".into());
    args.push("lavfi".into());
    args.push("-i".into());
    args.push(format!(
        "testsrc2=duration={}:size={}x{}:rate=30",
        duration_secs, width, height
    ));
    args.push("-f".into());
    args.push("lavfi".into());
    args.push("-i".into());
    args.push(format!("sine=frequency=440:duration={}", duration_secs));
    args.push("-c:v".into());
    args.push("libx264".into());
    args.push("-preset".into());
    args.push("veryfast".into());
    args.push("-c:a".into());
    args.push("aac".into());
    args.push("-shortest".into());
    args.push("-movflags".into());
    args.push("+faststart".into());
    args.push(path.to_string_lossy().to_string());
    let joined: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let stderr = ffmpeg_ok(&joined);
    assert!(
        path.exists() && std::fs::metadata(path).unwrap().len() > 0,
        "generated clip missing after: {}\n{stderr}",
        path.display()
    );
}

/// RAII guard for the process-global benchmark capacity override. Restores the
/// previous value on drop so a run can never leak the override into the next.
struct CapacityOverride(Option<std::ffi::OsString>);

impl CapacityOverride {
    fn set(capacity: usize) -> Self {
        Self::set_raw(&capacity.to_string())
    }

    /// Sets the override to an arbitrary raw environment value, including
    /// invalid ones, to exercise the safe-fallback path in `start_batch`.
    fn set_raw(raw: &str) -> Self {
        let prev = std::env::var_os(CAPACITY_ENV);
        std::env::set_var(CAPACITY_ENV, raw);
        CapacityOverride(prev)
    }
}

impl Drop for CapacityOverride {
    fn drop(&mut self) {
        match &self.0 {
            Some(v) => std::env::set_var(CAPACITY_ENV, v),
            None => std::env::remove_var(CAPACITY_ENV),
        }
    }
}

/// Serializes access to the process-global `ASPECTSHIFT_BENCH_TOTAL_CAPACITY`
/// override across every test in this binary. The env var is process-global,
/// so two tests touching it concurrently would otherwise race each other (the
/// ignored sweep additionally needs `--test-threads=1` for a deterministic
/// printed matrix). Each `#[tokio::test]` runs on its own thread/runtime, so
/// an async-aware mutex lets a waiter block without pinning any executor.
static CAPACITY_OVERRIDE_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Tracks the maximum number of jobs simultaneously `Processing` across every
/// `batch://progress` snapshot, i.e. the scheduler concurrency actually
/// exercised by the real batch path.
fn record_processing_snapshots(app: &tauri::App) -> Arc<StdMutex<usize>> {
    let recorder: Arc<StdMutex<usize>> = Arc::new(StdMutex::new(0));
    {
        let cap = recorder.clone();
        app.listen("batch://progress", move |e| {
            if let Ok(p) = serde_json::from_str::<BatchProgress>(e.payload()) {
                let processing = p
                    .queue
                    .iter()
                    .filter(|f| matches!(f.status, JobStatus::Processing))
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

#[derive(Clone, Copy)]
struct Workload {
    label: &'static str,
    width: u32,
    height: u32,
    clips: usize,
    duration_secs: u32,
}

const REPRESENTATIVE_WORKLOADS: &[Workload] = &[
    Workload {
        label: "720p",
        width: 1280,
        height: 720,
        clips: 4,
        duration_secs: 12,
    },
    Workload {
        label: "1080p",
        width: 1920,
        height: 1080,
        clips: 4,
        duration_secs: 12,
    },
    Workload {
        label: "4k",
        width: 3840,
        height: 2160,
        clips: 2,
        duration_secs: 10,
    },
];

/// Target output dimensions for a landscape 9:16 conversion, mirroring
/// `render_layout::calculate_render_layout` (max height 1920, height-limited
/// width 9:16, both rounded to even). Used only to document the workload.
fn target_output_resolution(_source_width: u32, source_height: u32) -> (u32, u32) {
    const MAX_HEIGHT: u32 = 1920;
    let display_h = source_height.min(MAX_HEIGHT);
    let h = (display_h as f32 / 2.0).round() as u32 * 2;
    let w = (h as f32 * (9.0 / 16.0) / 2.0).round() as u32 * 2;
    (w, h)
}

/// Runs one measured batch at a forced capacity through the real path and
/// returns (elapsed seconds, max concurrent Processing, terminal status).
#[allow(clippy::too_many_arguments)]
async fn run_measured_batch(
    label: &str,
    capacity: usize,
    inputs: &[String],
    out_dir: &Path,
    timeout: Duration,
) -> (f64, usize, BatchStatus) {
    let _lock = CAPACITY_OVERRIDE_LOCK.lock().await;
    let app = app();
    let snapshots = record_processing_snapshots(&app);
    let manager = app.state::<BatchManager>();
    let targets = vec![output_job("job-bench", AspectRatio::Ratio9x16)];

    let _override = CapacityOverride::set(capacity);

    let started = std::time::Instant::now();
    start_batch(
        app.handle().clone(),
        manager,
        inputs.to_vec(),
        settings(out_dir, targets),
    )
    .await
    .expect("start_batch failed");
    let terminal = wait_for_terminal(&app, timeout).await;
    let elapsed = started.elapsed().as_secs_f64();

    assert!(
        terminal.status == BatchStatus::Completed,
        "{} @ capacity {capacity}: batch ended {:?} (completed {}, failed {})",
        label,
        terminal.status,
        terminal.completed_jobs,
        terminal.failed_jobs
    );
    assert_eq!(
        terminal.completed_jobs,
        inputs.len(),
        "{} @ capacity {capacity}: expected every input to complete",
        label
    );
    assert_eq!(terminal.failed_jobs, 0, "{label} @ capacity {capacity}");

    for entry in &terminal.queue {
        assert!(
            matches!(entry.status, JobStatus::Completed),
            "{} @ capacity {capacity}: job {} ended {:?}",
            label,
            entry.job_id,
            entry.status
        );
        assert_eq!(entry.progress, 100.0);
    }

    let outputs = std::fs::read_dir(out_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "mp4").unwrap_or(false))
        .collect::<Vec<_>>();
    assert_eq!(
        outputs.len(),
        inputs.len(),
        "{label} @ capacity {capacity}: expected {} outputs, got {}",
        inputs.len(),
        outputs.len()
    );
    for out in &outputs {
        assert_valid_video(out);
    }

    let max_processing = *snapshots.lock().unwrap();
    assert!(
        max_processing <= capacity,
        "{label} @ capacity {capacity}: observed {max_processing} simultaneous \
         Processing jobs, exceeding the forced capacity"
    );
    let expected_active = inputs.len().min(capacity);
    assert!(
        max_processing >= expected_active,
        "{label} @ capacity {capacity}: the real batch path never reached the forced \
         concurrency (expected >= {expected_active} simultaneous, observed {max_processing})"
    );

    (elapsed, max_processing, terminal.status)
}

// ---------------------------------------------------------------------------
// Requirement 2.7.5 — Verify that total_capacity = 2, 3, and 4 are actually
// accepted and exercised by the real batch path (and capacity 1 stays
// strictly sequential). Runs with the normal suite.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn forced_capacities_are_accepted_and_respected_by_the_real_path() {
    let root = test_dir("accepted");
    let out_dir = root.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut inputs = Vec::new();
    for i in 0..4 {
        let p = root.join(format!("clip{i}.mp4"));
        make_clip(&p, 480, 270, 3);
        inputs.push(p.to_string_lossy().to_string());
    }

    for capacity in 1..=4 {
        let cell_out = out_dir.join(format!("cap{capacity}"));
        std::fs::create_dir_all(&cell_out).unwrap();
        let (elapsed, max_processing, status) = run_measured_batch(
            "acceptance",
            capacity,
            &inputs,
            &cell_out,
            Duration::from_secs(180),
        )
        .await;
        assert_eq!(status, BatchStatus::Completed);
        println!(
            "forced capacity {capacity}: batch completed in {elapsed:.2}s, \
             max simultaneous Processing = {max_processing}"
        );
        assert!(
            max_processing <= capacity,
            "scheduler must respect the forced capacity {capacity}, observed {max_processing}"
        );
        if capacity == 1 {
            assert_eq!(max_processing, 1, "capacity 1 must be strictly sequential");
        } else {
            assert!(
                max_processing >= 2,
                "capacity {capacity} must admit at least 2 jobs concurrently,\
                 observed {max_processing}"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Requirement 2.7 audit (benchmark override isolation) — invalid values of
// `ASPECTSHIFT_BENCH_TOTAL_CAPACITY` must never admit more than the hard
// safety ceiling. Values that parse as a number are clamped to 1..=4
// (0 -> sequential, 5 -> the ceiling); values that cannot be parsed (negative,
// non-numeric, empty) fall back to the normal Stage 1.3 planner, leaving the
// default planner authoritative. Runs with the normal suite.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn invalid_benchmark_override_values_fall_back_safely() {
    let _lock = CAPACITY_OVERRIDE_LOCK.lock().await;

    let natural = calculate_safe_concurrency(&detect_system_resources());
    assert!(natural.total_capacity >= 1);

    // The fallback cases were previously expected to reach `natural.total_capacity`
    // sampled once before the loop. Stage 3.1 (live RAM gating at batch start)
    // makes the standard planner re-observe current resources at *every* batch
    // start, so on a machine hovering near the 4 GiB gate the capacity actually
    // used by a fallback batch can legitimately differ from an earlier sample.
    // The cases below therefore compute each fallback expectation from live
    // resources immediately before the batch under test — mirroring exactly what
    // the production planner does at that batch's own start. No safety assertion
    // is weakened: every "never exceed the ceiling/effective capacity" check
    // below is unchanged.
    let cases: &[(&str, Option<usize>)] = &[
        ("0", Some(1)),                  // parses -> clamped to sequential
        ("5", Some(MAX_TOTAL_CAPACITY)), // parses -> clamped to the hard ceiling
        ("-1", None),                    // unparseable -> standard planner (fresh at batch start)
        ("abc", None),                   // non-numeric -> standard planner
        ("", None),                      // empty -> standard planner
    ];

    let root = test_dir("invalid-override");
    let out_root = root.join("out");
    std::fs::create_dir_all(&out_root).unwrap();

    let mut inputs = Vec::new();
    for i in 0..4 {
        let p = root.join(format!("clip{i}.mp4"));
        make_clip(&p, 480, 270, 3);
        inputs.push(p.to_string_lossy().to_string());
    }

    for (raw, expected) in cases {
        // Parseable overrides use their clamped value; unparseable ones fall
        // back to the standard planner, so resolve the expectation from live
        // resources right now (the batch under test will do the same at its
        // own start — Stage 3.1 freshness).
        let expected_capacity = match expected {
            Some(value) => *value,
            None => calculate_safe_concurrency(&detect_system_resources()).total_capacity,
        };
        let cell_out = out_root.join(format!("raw_{}", raw.replace(['-', ' '], "_")));
        std::fs::create_dir_all(&cell_out).unwrap();

        let app = app();
        let snapshots = record_processing_snapshots(&app);
        let manager = app.state::<BatchManager>();
        let targets = vec![output_job("job-bench", AspectRatio::Ratio9x16)];

        let _override = CapacityOverride::set_raw(raw);

        let started = std::time::Instant::now();
        start_batch(
            app.handle().clone(),
            manager,
            inputs.clone(),
            settings(&cell_out, targets),
        )
        .await
        .expect("start_batch failed");
        let terminal = wait_for_terminal(&app, Duration::from_secs(180)).await;
        let _elapsed = started.elapsed().as_secs_f64();

        assert!(
            terminal.status == BatchStatus::Completed
                && terminal.failed_jobs == 0
                && terminal.completed_jobs == inputs.len(),
            "override {:?}: batch ended {:?} (completed {}, failed {})",
            raw,
            terminal.status,
            terminal.completed_jobs,
            terminal.failed_jobs
        );
        for entry in &terminal.queue {
            assert!(
                matches!(entry.status, JobStatus::Completed),
                "override {:?}: job {} ended {:?}",
                raw,
                entry.job_id,
                entry.status
            );
        }

        let outputs = std::fs::read_dir(&cell_out)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().map(|e| e == "mp4").unwrap_or(false))
            .collect::<Vec<_>>();
        assert_eq!(outputs.len(), inputs.len(), "override {:?}", raw);
        for out in &outputs {
            assert_valid_video(out);
        }

        let max_processing = *snapshots.lock().unwrap();
        let effective = expected_capacity.min(inputs.len());
        assert!(
            max_processing <= expected_capacity,
            "override {:?}: observed {max_processing} jobs, above the effective \
             capacity {expected_capacity}",
            raw
        );
        assert!(
            max_processing <= MAX_TOTAL_CAPACITY,
            "override {:?}: exceeded the hard ceiling {MAX_TOTAL_CAPACITY}",
            raw
        );
        assert!(
            max_processing >= 1,
            "override {:?}: batch never ran (max_processing = 0)",
            raw
        );
        if effective > 1 {
            assert!(
                max_processing == effective,
                "override {:?}: expected the scheduler to reach its effective \
                 capacity {effective}, observed {max_processing}",
                raw
            );
        } else {
            assert_eq!(
                max_processing, 1,
                "override {:?}: effective capacity 1 must be strictly sequential",
                raw
            );
        }
    }

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Stage 2.7 benchmark — the full representative-workload x capacity matrix.
// Measures total batch wall-clock time; see the module docs for run command.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn benchmark_representative_workloads_across_capacities() {
    let machine = detect_system_resources();
    let natural = calculate_safe_concurrency(&machine);
    println!("===== Stage 2.7 benchmark start =====");
    println!(
        "logical cpu threads: {} | available RAM: {:.1} GiB",
        machine.logical_cpu_threads,
        machine.available_memory_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    println!(
        "natural plan (Stage 1.3 defaults): capacity = {}, mode = {:?}, \
         ffmpeg_threads_per_job = {}",
        natural.total_capacity, natural.mode, natural.ffmpeg_threads_per_job
    );

    let capacities = parse_capacities();
    let runs = parse_runs();
    println!(
        "matrix: {} capacities [{}] x {} workloads x {} run(s) each = {} cells",
        capacities.len(),
        capacities
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(","),
        REPRESENTATIVE_WORKLOADS.len(),
        runs,
        capacities.len() * REPRESENTATIVE_WORKLOADS.len() * runs
    );

    let root = test_dir("bench");
    let overall_start = std::time::Instant::now();

    println!("\n| workload | capacity | run | wall_clock_s | max_concurrency |");

    for workload in REPRESENTATIVE_WORKLOADS {
        let media_dir = root.join(format!("media_{}", workload.label));
        std::fs::create_dir_all(&media_dir).unwrap();

        // Media generation: excluded from every measurement by construction
        // (happens before any timer starts).
        let mut inputs = Vec::new();
        for i in 0..workload.clips {
            let p = media_dir.join(format!(
                "{}_{}x{}_clip{i}.mp4",
                workload.label, workload.width, workload.height
            ));
            make_clip(&p, workload.width, workload.height, workload.duration_secs);
            inputs.push(p.to_string_lossy().to_string());
        }
        let (out_w, out_h) = target_output_resolution(workload.width, workload.height);
        println!(
            "\n[workload {}]: {} native {}x{} clips of {}s each (time not measured); \
             9:16 target output is ~{}x{} (render_layout, max height 1920)",
            workload.label,
            workload.clips,
            workload.width,
            workload.height,
            workload.duration_secs,
            out_w,
            out_h
        );

        for &capacity in &capacities {
            for run in 0..runs {
                let out_dir = root.join(format!("out_{}_cap{}_{run}", workload.label, capacity));
                std::fs::create_dir_all(&out_dir).unwrap();

                // Wall-clock timeout: worst case is 4K at capacity 1 where
                // encodes take longest. 30 minutes leaves wide margin.
                let (elapsed, max_processing, _status) = run_measured_batch(
                    workload.label,
                    capacity,
                    &inputs,
                    &out_dir,
                    Duration::from_secs(1800),
                )
                .await;
                println!(
                    "| {} | {} | {} | {:.2} | {} |",
                    workload.label,
                    capacity,
                    run + 1,
                    elapsed,
                    max_processing
                );
            }
        }
    }

    let total = overall_start.elapsed().as_secs_f64();
    println!("\n===== Stage 2.7 benchmark done in {total:.2}s =====");
    println!(
        "machine natural plan was capacity {} (threads {}); all four capacities were forced \
         through the real path for comparison.",
        natural.total_capacity, natural.ffmpeg_threads_per_job
    );
    println!(
        "All cells completed; media root kept for re-inspection: {}",
        root.display()
    );
    println!("(remove manually after results are recorded: it is outside the repo)");
}

fn parse_runs() -> usize {
    std::env::var(RUNS_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map(|v| v.max(1))
        .unwrap_or(1)
}

fn parse_capacities() -> Vec<usize> {
    let mut values: Vec<usize> = std::env::var(CAPACITIES_ENV)
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|p| p.trim().parse::<usize>().ok())
                .collect()
        })
        .unwrap_or_else(|| vec![1, 2, 3, 4]);
    values.retain(|c| (1..=4).contains(c));
    values.dedup();
    if values.is_empty() {
        vec![1, 2, 3, 4]
    } else {
        values
    }
}
