//! Roadmap Stage 3.1 — live RAM gating at every batch start + post-failure
//! subsequent-batch replanning: a failed batch must leave no stale
//! plan/capacity behind for the next one. There is deliberately **no** unsafe
//! mid-batch RAM re-planning or resize of an already-admitted, running batch.
//!
//! Stage 3.1's central guarantee is *freshness*: every batch start re-reads
//! current system resources and derives a brand-new `ConcurrencyPlan` from
//! them (Stage 1.3 CPU tier → RAM gate → hard cap). Nothing — resource
//! snapshot, capacity value, or plan — is cached or shared between batches.
//!
//! This file validates that guarantee at three levels:
//!
//! 1. **Deterministic canned-profile tests** (Scenario A/B/C from the Testing
//!    & Validation Matrix) against the real production seam
//!    `resolve_batch_start_plan`, plus the Stage 2.7 benchmark override
//!    routing. No environment or RAM mutation; these run with the normal
//!    suite.
//! 2. **Real-pipeline cross-batch tests**: consecutive real batches prove each
//!    one replans at its *own* start (a capacity override set *between* two
//!    batches is honored by the second and only the second), and that a batch
//!    containing a real per-job failure leaves no stale capacity for the next
//!    batch (Stage 2.6 failure isolation + Stage 3.1 freshness end to end).
//! 3. **Real-system RAM pressure test** (`#[ignore]`d): pushes actual
//!    available RAM below the 4 GiB gate with a large touched working set,
//!    re-resolves the plan for a second batch, and asserts capacity collapses
//!    to Sequential — then releases and asserts recovery. This mutates machine
//!    memory, so it runs only on request:
//!
//!    ```text
//!    cargo test --test stage3_1_ram_gating -- --ignored --test-threads=1 --nocapture
//!    ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use aspectshift_htov_lib::video::batch_processor::{get_batch_status, start_batch};
use aspectshift_htov_lib::video::concurrency::{
    calculate_safe_concurrency, detect_system_resources, resolve_batch_start_plan, ExecutionMode,
    MAX_TOTAL_CAPACITY,
};
use aspectshift_htov_lib::video::paths::resolve_temp_output_path;
use aspectshift_htov_lib::video::queue::BatchManager;
use aspectshift_htov_lib::video::types::{
    AspectRatio, BatchJobSettings, BatchProgress, BatchStatus, EncodingProfile, JobStatus,
    OutputJob, SelectionMetadata, SubtitleOverlaySettings, TargetType, VideoEffectsSettings,
};
use tauri::{Listener, Manager};

const CAPACITY_ENV: &str = "ASPECTSHIFT_BENCH_TOTAL_CAPACITY";
const GB: u64 = 1024 * 1024 * 1024;
const FOUR_GB: u64 = 4 * GB;

/// Serializes every test in this binary. Several tests touch process-global or
/// machine-global state (the `ASPECTSHIFT_BENCH_TOTAL_CAPACITY` env var, and
/// the RAM-pressure test mutates the machine's available memory). Cargo runs
/// this file's `#[tokio::test]` functions in parallel on separate runtimes, so
/// the lock prevents them from corrupting each other's observations.
static TEST_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// RAII guard clearing the process-global benchmark capacity override, and
/// restoring the previous value on drop so nothing leaks between tests.
struct NoCapacityOverride(Option<std::ffi::OsString>);

impl NoCapacityOverride {
    fn acquire() -> Self {
        let previous = std::env::var_os(CAPACITY_ENV);
        std::env::remove_var(CAPACITY_ENV);
        NoCapacityOverride(previous)
    }
}

/// RAII guard setting the process-global benchmark capacity override to `raw`
/// (e.g. a valid digit, `"0"`, or `"abc"` to exercise the safe fallback).
struct CapacityOverride(Option<std::ffi::OsString>);

impl CapacityOverride {
    fn set_raw(raw: &str) -> Self {
        let previous = std::env::var_os(CAPACITY_ENV);
        std::env::set_var(CAPACITY_ENV, raw);
        CapacityOverride(previous)
    }
}

impl Drop for NoCapacityOverride {
    fn drop(&mut self) {
        restore(&self.0);
    }
}

impl Drop for CapacityOverride {
    fn drop(&mut self) {
        restore(&self.0);
    }
}

fn restore(previous: &Option<std::ffi::OsString>) {
    match previous {
        Some(v) => std::env::set_var(CAPACITY_ENV, v),
        None => std::env::remove_var(CAPACITY_ENV),
    }
}

fn ffmpeg_bin() -> PathBuf {
    let deps = std::env::current_exe().unwrap();
    deps.parent().unwrap().parent().unwrap().join("ffmpeg.exe")
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

/// 640x360 landscape clip with optional sine audio (forces a real libx264
/// encode, since horizontal input is never passthrough for portrait targets).
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
    let root = std::env::temp_dir().join(format!("stage3_1_{label}_{}", uuid::Uuid::new_v4()));
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

/// Occupies `path` with a non-empty directory so FFmpeg's *rename* onto it (or
/// a later attempt to treat it as a file) fails.
fn occupy_dir(path: &Path) {
    std::fs::create_dir(path).unwrap_or_else(|e| panic!("create_dir {}: {e}", path.display()));
    std::fs::write(path.join(".keep"), b"").unwrap();
}

/// Tracks the max number of jobs simultaneously `Processing` across every
/// `batch://progress` snapshot — the scheduler concurrency the real batch
/// actually exercised.
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

fn machine_capacity() -> usize {
    calculate_safe_concurrency(&detect_system_resources()).total_capacity
}

// ---------------------------------------------------------------------------
// Layer 1 — deterministic canned-profile tests (Testing matrix Scenario A/B/C)
// through the real production planning seam `resolve_batch_start_plan`.
// ---------------------------------------------------------------------------

/// Scenario A — healthy available RAM at batch start: the CPU tier value is
/// used, subject only to the hard cap.
#[tokio::test]
async fn scenario_a_healthy_ram_uses_cpu_tier_plan() {
    let _l = TEST_LOCK.lock().await;
    let _env = NoCapacityOverride::acquire();

    let p = aspectshift_htov_lib::video::concurrency::ResourceProfile {
        logical_cpu_threads: 8,
        available_memory_bytes: 16 * GB,
    };
    let plan = resolve_batch_start_plan(|| p);
    assert_eq!(plan.total_capacity, 2, "8 threads + healthy RAM -> tier 2");
    assert_eq!(plan.mode, ExecutionMode::Parallel);

    let p = aspectshift_htov_lib::video::concurrency::ResourceProfile {
        logical_cpu_threads: 33,
        available_memory_bytes: 32 * GB,
    };
    let plan = resolve_batch_start_plan(|| p);
    assert_eq!(
        plan.total_capacity, MAX_TOTAL_CAPACITY,
        "33 threads + healthy RAM -> cap 4"
    );
}

/// Scenario B — low available RAM at batch start: the RAM gate overrides the
/// CPU tier (below 4 GiB forces Sequential; 4–8 GiB caps at 2).
#[tokio::test]
async fn scenario_b_low_ram_gates_down_to_sequential() {
    let _l = TEST_LOCK.lock().await;
    let _env = NoCapacityOverride::acquire();

    let p = aspectshift_htov_lib::video::concurrency::ResourceProfile {
        logical_cpu_threads: 64,
        available_memory_bytes: 3 * GB,
    };
    let plan = resolve_batch_start_plan(|| p);
    assert_eq!(
        plan.total_capacity, 1,
        "64 threads but < 4 GiB available must force sequential"
    );
    assert_eq!(plan.mode, ExecutionMode::Sequential);

    let p = aspectshift_htov_lib::video::concurrency::ResourceProfile {
        logical_cpu_threads: 64,
        available_memory_bytes: 6 * GB,
    };
    let plan = resolve_batch_start_plan(|| p);
    assert_eq!(
        plan.total_capacity, 2,
        "4–8 GiB caps even a huge CPU tier at 2"
    );
}

/// Scenario C — RAM changes *between* batch starts. Each resolution calls the
/// live provider exactly once and derives a fresh plan; Batch B must observe
/// RAM state B (never A), and Batch C must observe recovered RAM (both
/// directions: A->B drops, B->C rises).
#[tokio::test]
async fn scenario_c_ram_change_between_batch_starts_replans_each() {
    let _l = TEST_LOCK.lock().await;
    let _env = NoCapacityOverride::acquire();

    let states = [
        aspectshift_htov_lib::video::concurrency::ResourceProfile {
            logical_cpu_threads: 8,
            available_memory_bytes: 5 * GB,
        },
        aspectshift_htov_lib::video::concurrency::ResourceProfile {
            logical_cpu_threads: 8,
            available_memory_bytes: 3 * GB,
        },
        aspectshift_htov_lib::video::concurrency::ResourceProfile {
            logical_cpu_threads: 8,
            available_memory_bytes: 12 * GB,
        },
    ];
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let detect = {
        let calls = calls.clone();
        move || {
            let idx = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert!(
                idx < states.len(),
                "detection must run exactly once per batch start"
            );
            states[idx]
        }
    };

    let batch_a = resolve_batch_start_plan(&detect);
    let batch_b = resolve_batch_start_plan(&detect);
    let batch_c = resolve_batch_start_plan(&detect);

    assert_eq!(
        batch_a.total_capacity, 2,
        "Batch A sees RAM state A (5 GiB -> capped 2)"
    );
    assert_eq!(batch_a.mode, ExecutionMode::Parallel);
    assert_eq!(
        batch_b.total_capacity, 1,
        "Batch B must see RAM state B (3 GiB) and go Sequential"
    );
    assert_eq!(batch_b.mode, ExecutionMode::Sequential);
    assert_eq!(
        batch_c.total_capacity, 2,
        "Batch C must see recovered RAM state C (12 GiB) and replan upward"
    );
    assert!(
        batch_b.total_capacity < batch_a.total_capacity,
        "A -> B drops"
    );
    assert!(
        batch_c.total_capacity > batch_b.total_capacity,
        "B -> C recovers"
    );
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "each batch start must run one fresh resource detection"
    );
}

/// The Stage 2.7 benchmark override must flow through the same batch-start
/// seam (it moved here from `start_batch`), with the same safety semantics:
/// valid digits clamp `1..=MAX_TOTAL_CAPACITY`, unparseable values fall back
/// to the standard Stage 1.3 plan.
#[tokio::test]
async fn benchmark_override_is_routed_through_batch_start_seam() {
    let _l = TEST_LOCK.lock().await;

    let p = aspectshift_htov_lib::video::concurrency::ResourceProfile {
        logical_cpu_threads: 8,
        available_memory_bytes: 16 * GB,
    };

    let _override_zero = CapacityOverride::set_raw("0");
    let plan = resolve_batch_start_plan(|| p);
    assert_eq!(plan.total_capacity, 1, "0 clamps up to 1");

    drop(_override_zero);
    let _override_huge = CapacityOverride::set_raw("99");
    let plan = resolve_batch_start_plan(|| p);
    assert_eq!(
        plan.total_capacity, MAX_TOTAL_CAPACITY,
        "99 clamps down to the global ceiling"
    );

    drop(_override_huge);
    let _override_bad = CapacityOverride::set_raw("abc");
    let plan = resolve_batch_start_plan(|| p);
    assert_eq!(
        plan.total_capacity,
        calculate_safe_concurrency(&p).total_capacity,
        "unparseable override falls back to the standard planner (no custom cap)"
    );
}

// ---------------------------------------------------------------------------
// Layer 2 — real-pipeline cross-batch freshness + post-failure replanning
// checks.
// ---------------------------------------------------------------------------

/// Every batch must resolve its own plan at its *own* start. Proof: batch A
/// runs at the machine's natural capacity; an override is then set *between*
/// the two batches; batch B alone obeys it (max 1 concurrent), which is
/// impossible if B had inherited A's plan. Requires natural capacity >= 2 to
/// observe the A-side difference; otherwise the B-side strict check still
/// verifies a fresh plan resolution (documented limitation below).
#[tokio::test]
async fn each_batch_resolves_its_own_plan_at_its_own_start() {
    let _l = TEST_LOCK.lock().await;
    let _env = NoCapacityOverride::acquire();

    let natural = machine_capacity();
    assert!(natural >= 1);

    // Batch A: natural plan, nothing special.
    let app_a = app();
    let root_a = test_dir("fresh_a");
    let out_a = root_a.join("out");
    std::fs::create_dir_all(&out_a).unwrap();
    let a1 = root_a.join("a1.mp4");
    let a2 = root_a.join("a2.mp4");
    make_test_video(&a1, 12, false);
    make_test_video(&a2, 12, false);
    let recorder_a = record_processing_snapshots(&app_a);
    let manager_a = app_a.state::<BatchManager>();
    start_batch(
        app_a.handle().clone(),
        manager_a,
        vec![
            a1.to_string_lossy().to_string(),
            a2.to_string_lossy().to_string(),
        ],
        settings(&out_a, vec![output_job("job-a", AspectRatio::Ratio9x16)]),
    )
    .await
    .expect("batch A start failed");
    let terminal_a = wait_for_terminal(&app_a, Duration::from_secs(180)).await;
    assert_eq!(terminal_a.status, BatchStatus::Completed);
    assert_eq!(terminal_a.completed_jobs, 2);
    let max_a = *recorder_a.lock().unwrap();

    // Between A and B, change the plan input. If the planner were cached or
    // reused from batch A's start, batch B could never see this change.
    let _override_one = CapacityOverride::set_raw("1");

    // Batch B: 2 inputs, forced capacity 1 -> strictly sequential.
    let app_b = app();
    let root_b = test_dir("fresh_b");
    let out_b = root_b.join("out");
    std::fs::create_dir_all(&out_b).unwrap();
    let b1 = root_b.join("b1.mp4");
    let b2 = root_b.join("b2.mp4");
    make_test_video(&b1, 12, false);
    make_test_video(&b2, 12, false);
    let recorder_b = record_processing_snapshots(&app_b);
    let manager_b = app_b.state::<BatchManager>();
    start_batch(
        app_b.handle().clone(),
        manager_b,
        vec![
            b1.to_string_lossy().to_string(),
            b2.to_string_lossy().to_string(),
        ],
        settings(&out_b, vec![output_job("job-b", AspectRatio::Ratio9x16)]),
    )
    .await
    .expect("batch B start failed");
    let terminal_b = wait_for_terminal(&app_b, Duration::from_secs(180)).await;
    assert_eq!(terminal_b.status, BatchStatus::Completed);
    assert_eq!(terminal_b.completed_jobs, 2);
    let max_b = *recorder_b.lock().unwrap();

    assert_eq!(
        max_b, 1,
        "batch B was planned at ITS start with the fresh capacity 1 override; \
         a reused plan from batch A would have let both jobs overlap"
    );
    if natural >= 2 {
        assert!(
            max_a >= 2,
            "batch A ran at natural capacity {natural}, so its two jobs must have \
             overlapped (observed max {max_a})"
        );
    } else {
        println!(
            "documented limitation: natural machine capacity is {natural} (Sequential), \
             so the A-side overlap difference is not observable here; batch B's strict \
             max==1 still proves a fresh plan resolution at its own start"
        );
    }

    let _ = std::fs::remove_dir_all(&root_a);
    let _ = std::fs::remove_dir_all(&root_b);
}

/// Stage 3.1 post-failure subsequent-batch replanning check end to end: a
/// batch that contains a real per-job failure (Stage 2.6) must (a) isolate the
/// failure to that job and (b) leave no stale capacity or plan behind. The
/// next batch starts from a completely fresh plan at its own capacity.
#[tokio::test]
async fn failed_batch_leaves_no_stale_capacity_for_next_batch() {
    let _l = TEST_LOCK.lock().await;
    let _env = NoCapacityOverride::acquire();

    let natural = machine_capacity();

    // Batch 1: a genuine FFmpeg-process failure (temp output path occupied by
    // a directory, so the FFmpeg child itself fails to open its output) in a
    // batch of three.
    let app_1 = app();
    let root_1 = test_dir("failthen");
    let out_1 = root_1.join("out");
    std::fs::create_dir_all(&out_1).unwrap();
    let fa = root_1.join("fa.mp4");
    let fb = root_1.join("fb.mp4");
    let fc = root_1.join("fc.mp4");
    make_test_video(&fa, 6, false);
    make_test_video(&fb, 6, false);
    make_test_video(&fc, 6, false);
    occupy_dir(&resolve_temp_output_path(&out_1.join("fa_9x16.mp4")));
    let manager_1 = app_1.state::<BatchManager>();
    start_batch(
        app_1.handle().clone(),
        manager_1,
        vec![
            fa.to_string_lossy().to_string(),
            fb.to_string_lossy().to_string(),
            fc.to_string_lossy().to_string(),
        ],
        settings(&out_1, vec![output_job("job-1", AspectRatio::Ratio9x16)]),
    )
    .await
    .expect("batch 1 start failed");
    let terminal_1 = wait_for_terminal(&app_1, Duration::from_secs(240)).await;
    assert_eq!(terminal_1.status, BatchStatus::Failed);
    assert_eq!(terminal_1.failed_jobs, 1, "exactly one job failed");
    assert_eq!(terminal_1.completed_jobs, 2, "healthy jobs completed");

    // Batch 2: a fresh batch, same healthy shape. It must not inherit any
    // capacity released or failed in batch 1: on capacity >= 2 machines the
    // two jobs overlap again (full capacity available), and both complete.
    let app_2 = app();
    let root_2 = test_dir("failthen2");
    let out_2 = root_2.join("out");
    std::fs::create_dir_all(&out_2).unwrap();
    let ga = root_2.join("ga.mp4");
    let gb = root_2.join("gb.mp4");
    make_test_video(&ga, 12, false);
    make_test_video(&gb, 12, false);
    let recorder_2 = record_processing_snapshots(&app_2);
    let manager_2 = app_2.state::<BatchManager>();
    start_batch(
        app_2.handle().clone(),
        manager_2,
        vec![
            ga.to_string_lossy().to_string(),
            gb.to_string_lossy().to_string(),
        ],
        settings(&out_2, vec![output_job("job-2", AspectRatio::Ratio9x16)]),
    )
    .await
    .expect("batch 2 start failed");
    let terminal_2 = wait_for_terminal(&app_2, Duration::from_secs(240)).await;
    assert_eq!(
        terminal_2.status,
        BatchStatus::Completed,
        "a fresh batch after a failed one must be completely unaffected"
    );
    assert_eq!(terminal_2.completed_jobs, 2);
    if natural >= 2 {
        let max_2 = *recorder_2.lock().unwrap();
        assert!(
            max_2 >= 2,
            "batch 2 must run at full fresh capacity after batch 1's failure \
             (observed max {max_2}, natural capacity {natural})"
        );
    } else {
        println!(
            "documented limitation: natural machine capacity is {natural} (Sequential), \
             so full-capacity overlap for batch 2 is not observable; tallies still prove \
             the failed batch left no stale plan behind"
        );
    }

    let _ = std::fs::remove_dir_all(&root_1);
    let _ = std::fs::remove_dir_all(&root_2);
}

// ---------------------------------------------------------------------------
// Layer 3 — real-system RAM pressure (explicit-only: mutates machine memory).
// ---------------------------------------------------------------------------

/// Writes a byte to the first cache line of every page, forcing the whole
/// buffer into the physical working set so the OS reports it as used.
fn touch_pages(buf: &mut [u8]) {
    const PAGE: usize = 4096;
    let mut salt: u8 = 0;
    for page in buf.chunks_mut(PAGE) {
        salt = salt.wrapping_add(1);
        page[0] = salt;
    }
}

/// (Explicit-only) Real-system RAM gate validation.
///
/// Allocates a large touched buffer owned by this process so Windows actually
/// reports dramatically less *available* RAM, waits until live detection reads
/// below the 4 GiB gate, resolves the plan for a "next batch" while pressured,
/// and asserts it collapses to Sequential (capacity 1). Then it releases the
/// buffer, waits for recovery, and proves the plan for the following batch
/// replans upward again.
///
/// The test is `#[ignore]`d because it mutates the machine's memory state; run
/// it explicitly:
///
/// ```text
/// cargo test --test stage3_1_ram_gating -- --ignored --test-threads=1 --nocapture
/// ```
#[tokio::test]
#[ignore]
async fn ram_pressure_forces_sequential_and_recovers_after_release() {
    let _l = TEST_LOCK.lock().await;
    let _env = NoCapacityOverride::acquire();

    // Baseline: live detection -> the plan a batch would get right now.
    let baseline = detect_system_resources();
    let plan_a = resolve_batch_start_plan(detect_system_resources);
    println!(
        "baseline: available {} MiB, {}-thread machine -> plan capacity {} ({:?})",
        baseline.available_memory_bytes / (1024 * 1024),
        baseline.logical_cpu_threads,
        plan_a.total_capacity,
        plan_a.mode
    );

    if plan_a.total_capacity <= 1 {
        println!(
            "SKIP: baseline plan is already Sequential ({} MiB available). No RAM-gating \
             transition can be demonstrated; nothing to prove on this machine state.",
            baseline.available_memory_bytes / (1024 * 1024)
        );
        return;
    }

    // How much RAM we must consume to cross the 4 GiB gate with ~1 GiB margin.
    let above_gate = baseline.available_memory_bytes.saturating_sub(FOUR_GB);
    let hog_goal_mib = (above_gate + 1024 * 1024 * 1024) / (1024 * 1024) + 512; // need above-gate + 1 GiB margin + 512 MiB slack
    let hog_goal = hog_goal_mib * 1024 * 1024;
    // Practical bounds: at least 512 MiB (meaningful), at most 6 GiB, never
    // more than available - 1 GiB (leave the machine a minute runway).
    let max_hog = baseline
        .available_memory_bytes
        .saturating_sub(1024 * 1024 * 1024);
    let hog_goal = hog_goal.min(max_hog).clamp(512 * 1024 * 1024, 6 * GB);
    println!(
        "allocating {:.1} GiB touched working set to push available RAM below the 4 GiB gate",
        hog_goal as f64 / GB as f64
    );

    let mut hog = vec![0u8; hog_goal as usize];
    touch_pages(&mut hog);

    // Poll live detection until the gate is actually crossed (standby-list
    // churn can delay the OS's accounting).
    let deadline = std::time::Instant::now() + Duration::from_secs(45);
    let mut pressured = None;
    while std::time::Instant::now() < deadline {
        touch_pages(&mut hog); // keep the pages dirty/committed
        let live = detect_system_resources();
        if live.available_memory_bytes < FOUR_GB {
            pressured = Some(live);
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let pressured = pressured.unwrap_or_else(|| {
        panic!(
            "could not push available RAM below the 4 GiB gate within 45s despite a {:.1} GiB \
             hog; another process may be freeing memory. Honest SKIP: the RAM-gate transition \
             could not be observed on this machine state.",
            hog_goal as f64 / GB as f64
        )
    });
    println!(
        "pressured: available {} MiB (< 4 GiB) -> resolving plan for the next batch",
        pressured.available_memory_bytes / (1024 * 1024)
    );

    let plan_b = resolve_batch_start_plan(detect_system_resources);
    assert_eq!(
        plan_b.total_capacity,
        1,
        "while live available RAM is below the 4 GiB gate the next batch's plan must be \
         Sequential (capacity 1); got capacity {} ({:?}) with {} MiB available",
        plan_b.total_capacity,
        plan_b.mode,
        pressured.available_memory_bytes / (1024 * 1024)
    );
    assert_eq!(plan_b.mode, ExecutionMode::Sequential);

    // Release the pressure and prove the *next* batch replans upward.
    drop(hog);
    let recovery_deadline = std::time::Instant::now() + Duration::from_secs(45);
    let mut recovered_plan = None;
    while std::time::Instant::now() < recovery_deadline {
        let live = detect_system_resources();
        if live.available_memory_bytes > FOUR_GB {
            recovered_plan = Some((live, resolve_batch_start_plan(|| live)));
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    let (recovered, recovered_capacity) = recovered_plan.unwrap_or_else(|| {
        panic!(
            "available RAM did not recover above the 4 GiB gate within 45s after releasing \
             the hog; the system may still be under external pressure"
        )
    });
    println!(
        "recovered: available {} MiB -> plan capacity {} ({:?})",
        recovered.available_memory_bytes / (1024 * 1024),
        recovered_capacity.total_capacity,
        recovered_capacity.mode
    );
    assert!(
        recovered_capacity.total_capacity >= plan_a.total_capacity,
        "after RAM recovery the next batch must replan back to at least its baseline \
         capacity {}; got {} ({:?})",
        plan_a.total_capacity,
        recovered_capacity.total_capacity,
        recovered_capacity.mode
    );
}
